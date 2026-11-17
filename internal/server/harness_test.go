package server

import (
	"bytes"
	"context"
	"database/sql"
	"encoding/json"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"net/netip"
	"net/url"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/auth"
	"github.com/henry40408/liftlog/internal/config"
	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/repo"
	"github.com/henry40408/liftlog/migrations"
)

const (
	// strongPassword passes the password policy for any username.
	strongPassword = "purple-monkey-dishwasher"
	testSetupToken = "ABCDE-FGHJK"
	// generousLimit keeps throttles out of the way of tests that are not about them.
	generousLimit = 100
)

var testLogSalt = bytes.Repeat([]byte{7}, 32)

// testAppOptions are the knobs tests turn; the zero value is the default app:
// generous throttles, plain-HTTP cookies, no proxy, no HSTS.
type testAppOptions struct {
	loginLimit     uint32
	sensitiveLimit uint32
	setupLimit     uint32
	loginBackoff   *auth.BackoffSchedule
	cookieSecure   bool
	proxyHeader    config.TrustedProxyHeader
	trustedProxies []netip.Addr
	hstsMaxAge     uint64
	hstsSubdomains bool
	noSetupToken   bool
	pool           *sql.DB
}

type testApp struct {
	t       *testing.T
	app     *App
	handler http.Handler
	pool    *sql.DB
	logs    *syncBuffer
}

// syncBuffer is a log sink safe for concurrent handlers.
type syncBuffer struct {
	mu  sync.Mutex
	buf bytes.Buffer
}

func (b *syncBuffer) Write(p []byte) (int, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.buf.Write(p)
}

func (b *syncBuffer) String() string {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.buf.String()
}

func newMemoryPool(t *testing.T) *sql.DB {
	t.Helper()
	pool, err := db.OpenMemory()
	if err != nil {
		t.Fatalf("open memory db: %v", err)
	}
	t.Cleanup(func() { _ = pool.Close() })
	if err := migrations.RunForTests(context.Background(), pool); err != nil {
		t.Fatalf("migrate: %v", err)
	}
	return pool
}

// newFilePool is for tests that need several connections at once.
func newFilePool(t *testing.T) *sql.DB {
	t.Helper()
	path := filepath.Join(t.TempDir(), "liftlog.db")
	pool, err := db.Open("sqlite:" + path)
	if err != nil {
		t.Fatalf("open file db: %v", err)
	}
	t.Cleanup(func() { _ = pool.Close() })
	if err := migrations.RunForTests(context.Background(), pool); err != nil {
		t.Fatalf("migrate: %v", err)
	}
	return pool
}

func orGenerous(limit uint32) uint32 {
	if limit == 0 {
		return generousLimit
	}
	return limit
}

func newTestApp(t *testing.T, options testAppOptions) *testApp {
	t.Helper()
	pool := options.pool
	if pool == nil {
		pool = newMemoryPool(t)
	}
	schedule := auth.BackoffSchedule{FreeAttempts: 3, Base: 0, Max: 0, Window: time.Minute}
	if options.loginBackoff != nil {
		schedule = *options.loginBackoff
	}
	throttles := Throttles{
		Login:           auth.NewRateLimiter[netip.Addr](orGenerous(options.loginLimit), time.Minute),
		LoginBackoff:    auth.NewFailureBackoff[string](schedule),
		SensitiveAction: auth.NewRateLimiter[string](orGenerous(options.sensitiveLimit), time.Minute),
		Setup:           auth.NewRateLimiter[netip.Addr](orGenerous(options.setupLimit), time.Minute),
	}
	logs := &syncBuffer{}
	logger := slog.New(slog.NewJSONHandler(logs, &slog.HandlerOptions{Level: slog.LevelDebug}))
	setupToken := &auth.SetupToken{}
	if !options.noSetupToken {
		setupToken = auth.NewSetupTokenWithValue(testSetupToken)
	}
	app := New(Options{
		DB: pool,
		Config: config.Config{
			CookieSecure:          options.cookieSecure,
			TrustedProxyHeader:    options.proxyHeader,
			TrustedProxies:        options.trustedProxies,
			HSTSMaxAge:            options.hstsMaxAge,
			HSTSIncludeSubdomains: options.hstsSubdomains,
		},
		Logger:     logger,
		SetupToken: setupToken,
		Throttles:  &throttles,
		LogSalt:    testLogSalt,
	})
	return &testApp{t: t, app: app, handler: app.Handler(), pool: pool, logs: logs}
}

// response is a recorded reply with the accessors tests keep needing.
type response struct {
	*httptest.ResponseRecorder
}

func (r response) status() int { return r.Code }

func (r response) text() string { return r.Body.String() }

func (r response) location() string { return r.Header().Get("Location") }

// setCookies are the raw Set-Cookie lines.
func (r response) setCookies() []string { return r.Header().Values("Set-Cookie") }

// sessionCookie is the Set-Cookie line for the session cookie, or "".
func (r response) sessionCookie() string {
	for _, line := range r.setCookies() {
		if strings.HasPrefix(line, "session=") || strings.HasPrefix(line, "__Host-session=") {
			return line
		}
	}
	return ""
}

// cookieHeader turns the session Set-Cookie into a Cookie request header.
func (r response) cookieHeader() string {
	line := r.sessionCookie()
	pair, _, _ := strings.Cut(line, ";")
	return pair
}

func (ta *testApp) do(req *http.Request) response {
	ta.t.Helper()
	if req.RemoteAddr == "" {
		req.RemoteAddr = "127.0.0.1:40000"
	}
	recorder := httptest.NewRecorder()
	ta.handler.ServeHTTP(recorder, req)
	return response{recorder}
}

func (ta *testApp) get(path, cookie string) response {
	ta.t.Helper()
	return ta.do(newRequest(http.MethodGet, path, "", cookie))
}

// postForm sends an urlencoded form; cookie may be "".
func (ta *testApp) postForm(path string, fields url.Values, cookie string) response {
	ta.t.Helper()
	return ta.do(newRequest(http.MethodPost, path, fields.Encode(), cookie))
}

func newRequest(method, path, body, cookie string) *http.Request {
	req := httptest.NewRequest(method, path, strings.NewReader(body))
	if method == http.MethodPost {
		req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	}
	if cookie != "" {
		req.Header.Set("Cookie", cookie)
	}
	return req
}

// withPeer sets the TCP peer, which is the client IP unless a proxy header is trusted.
func withPeer(req *http.Request, peer string) *http.Request {
	req.RemoteAddr = peer
	return req
}

func withHeader(req *http.Request, name, value string) *http.Request {
	req.Header.Set(name, value)
	return req
}

func (ta *testApp) createUser(username, password string, role models.UserRole) models.User {
	ta.t.Helper()
	hash, err := ta.app.hasher.Hash(password)
	if err != nil {
		ta.t.Fatalf("hash password: %v", err)
	}
	user, err := ta.app.users.Create(context.Background(), repo.NewUser{Username: username, PasswordHash: hash, Role: role})
	if err != nil {
		ta.t.Fatalf("create user %q: %v", username, err)
	}
	return user
}

func (ta *testApp) createSession(userID string) string {
	ta.t.Helper()
	token, err := ta.app.sessions.Create(context.Background(), userID)
	if err != nil {
		ta.t.Fatalf("create session: %v", err)
	}
	return token
}

// signedIn is a user with a live session, as the Rust test app seeded.
type signedIn struct {
	user   models.User
	token  string
	cookie string
}

func (ta *testApp) signIn(username string, role models.UserRole) signedIn {
	ta.t.Helper()
	user := ta.createUser(username, strongPassword, role)
	token := ta.createSession(user.ID)
	return signedIn{user: user, token: token, cookie: ta.cookieFor(token)}
}

func (ta *testApp) signInAdmin() signedIn { return ta.signIn("admin", models.RoleAdmin) }

func (ta *testApp) signInUser() signedIn { return ta.signIn("member", models.RoleUser) }

// cookieFor is the Cookie header a browser would send for token.
func (ta *testApp) cookieFor(token string) string {
	if ta.app.cfg.CookieSecure {
		return "__Host-session=" + token
	}
	return "session=" + token
}

func (ta *testApp) mustExec(query string, args ...any) {
	ta.t.Helper()
	if _, err := ta.pool.Exec(query, args...); err != nil {
		ta.t.Fatalf("exec %q: %v", query, err)
	}
}

func (ta *testApp) count(query string, args ...any) int {
	ta.t.Helper()
	var count int
	if err := ta.pool.QueryRow(query, args...).Scan(&count); err != nil {
		ta.t.Fatalf("count %q: %v", query, err)
	}
	return count
}

// ageSessionTouch pretends the session was last touched hours ago.
func (ta *testApp) ageSessionTouch(token string, hours int) {
	ta.t.Helper()
	ta.mustExec("UPDATE sessions SET last_touched_at = datetime('now', ?) WHERE token = ?",
		"-"+strconv.Itoa(hours)+" hours", token)
}

// ageSessionCreation pretends the session was created days ago.
func (ta *testApp) ageSessionCreation(token string, days int) {
	ta.t.Helper()
	ta.mustExec("UPDATE sessions SET created_at = datetime('now', ?) WHERE token = ?",
		"-"+strconv.Itoa(days)+" days", token)
}

// expireSession makes the idle timeout pass.
func (ta *testApp) expireSession(token string) {
	ta.t.Helper()
	ta.mustExec("UPDATE sessions SET expires_at = datetime('now', '-1 hours') WHERE token = ?", token)
}

// auditEvents are the audit log lines, parsed, in order.
func (ta *testApp) auditEvents() []map[string]any {
	ta.t.Helper()
	var events []map[string]any
	for _, line := range strings.Split(ta.logs.String(), "\n") {
		if line == "" {
			continue
		}
		var entry map[string]any
		if err := json.Unmarshal([]byte(line), &entry); err != nil {
			ta.t.Fatalf("log line is not JSON: %q", line)
		}
		if entry["target"] == "liftlog::audit" {
			events = append(events, entry)
		}
	}
	return events
}

// auditEventsNamed filters auditEvents by the event attribute.
func (ta *testApp) auditEventsNamed(name string) []map[string]any {
	ta.t.Helper()
	var named []map[string]any
	for _, entry := range ta.auditEvents() {
		if entry["event"] == name {
			named = append(named, entry)
		}
	}
	return named
}

func (ta *testApp) onlyAuditEvent(name string) map[string]any {
	ta.t.Helper()
	events := ta.auditEventsNamed(name)
	if len(events) != 1 {
		ta.t.Fatalf("want exactly one %s event, got %d in:\n%s", name, len(events), ta.logs.String())
	}
	return events[0]
}
