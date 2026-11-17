package audit

import (
	"bytes"
	"encoding/json"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"net/netip"
	"strings"
	"testing"

	"github.com/henry40408/liftlog/internal/config"
)

func newRequest(t *testing.T, remoteAddr string, userAgent string) *http.Request {
	t.Helper()
	request := httptest.NewRequest(http.MethodGet, "/", nil)
	request.RemoteAddr = remoteAddr
	request.Header.Del("User-Agent")
	if userAgent != "" {
		request.Header.Set("User-Agent", userAgent)
	}
	return request
}

func TestRequestInfoTruncatesAHostileUserAgent(t *testing.T) {
	info := NewRequestInfo(newRequest(t, "127.0.0.1:1", strings.Repeat("a", 5000)), ClientSource{})
	if got := len([]rune(info.UserAgent)); got != maxUserAgentLen {
		t.Fatalf("user agent has %d characters", got)
	}
}

func TestRequestInfoUserAgentIsEmptyWhenAbsent(t *testing.T) {
	if got := NewRequestInfo(newRequest(t, "127.0.0.1:1", ""), ClientSource{}).UserAgent; got != "" {
		t.Fatalf("got %q", got)
	}
}

func TestRequestInfoFallsBackToLoopbackWithoutAPeer(t *testing.T) {
	info := NewRequestInfo(newRequest(t, "", ""), ClientSource{})
	if info.ClientIP != netip.MustParseAddr("127.0.0.1") {
		t.Fatalf("got %v", info.ClientIP)
	}
}

func TestRequestInfoUsesThePeer(t *testing.T) {
	info := NewRequestInfo(newRequest(t, "203.0.113.9:1234", ""), ClientSource{})
	if info.ClientIP != netip.MustParseAddr("203.0.113.9") {
		t.Fatalf("got %v", info.ClientIP)
	}
	if info.Path != "/" {
		t.Fatalf("path = %q", info.Path)
	}
}

func TestRequestInfoHonoursTheTrustedProxyHeader(t *testing.T) {
	request := newRequest(t, "127.0.0.1:1", "")
	request.Header.Set("X-Forwarded-For", "198.51.100.7")
	info := NewRequestInfo(request, ClientSource{TrustedProxyHeader: config.TrustXForwardedFor})
	if info.ClientIP != netip.MustParseAddr("198.51.100.7") {
		t.Fatalf("got %v", info.ClientIP)
	}
}

func TestTruncateCharsHandlesMultiByteInputWithoutSplittingRunes(t *testing.T) {
	got := truncateChars(strings.Repeat("台", 5000), maxUserAgentLen)
	if len([]rune(got)) != maxUserAgentLen || !strings.HasSuffix(got, "台") {
		t.Fatalf("got %d runes", len([]rune(got)))
	}
	if truncateChars("short", 10) != "short" {
		t.Fatal("short input was changed")
	}
}

func capture(t *testing.T, level slog.Level) (*Logger, func() map[string]any) {
	t.Helper()
	var out bytes.Buffer
	logger := NewLogger(slog.New(slog.NewJSONHandler(&out, &slog.HandlerOptions{Level: level})))
	return logger, func() map[string]any {
		t.Helper()
		var record map[string]any
		if err := json.Unmarshal(out.Bytes(), &record); err != nil {
			t.Fatalf("no JSON record in %q: %v", out.String(), err)
		}
		return record
	}
}

var testInfo = RequestInfo{ClientIP: netip.MustParseAddr("203.0.113.7"), UserAgent: "agent", Path: "/auth/login"}

func expect(t *testing.T, record map[string]any, want map[string]any) {
	t.Helper()
	for key, value := range want {
		if got := record[key]; got != value {
			t.Errorf("%s = %v (%T), want %v", key, got, got, value)
		}
	}
}

func TestEventsCarryTargetEventAndRequestFields(t *testing.T) {
	logger, read := capture(t, slog.LevelInfo)
	logger.LoginFailed(testInfo, "alice", 2000)
	expect(t, read(), map[string]any{
		"level": "WARN", "msg": "login failed", "target": "liftlog::audit", "event": "auth.login.failed",
		"username": "alice", "backoff_ms": float64(2000),
		"client_ip": "203.0.113.7", "user_agent": "agent", "path": "/auth/login",
	})
}

func TestAbsentOptionalFieldsAreOmitted(t *testing.T) {
	logger, read := capture(t, slog.LevelInfo)
	logger.CSRFRejected(RequestInfo{ClientIP: testInfo.ClientIP, Path: "/x"}, "origin_fallback", "POST", "")
	record := read()
	for _, key := range []string{"user_agent", "origin"} {
		if _, present := record[key]; present {
			t.Errorf("%s should be omitted", key)
		}
	}
	expect(t, record, map[string]any{"event": "csrf.rejected", "reason": "origin_fallback", "method": "POST"})
}

func TestCSRFRejectionTruncatesTheOrigin(t *testing.T) {
	logger, read := capture(t, slog.LevelInfo)
	logger.CSRFRejected(testInfo, "sec_fetch_site", "POST", strings.Repeat("o", 1000))
	if got := read()["origin"].(string); len(got) != maxOriginLen {
		t.Fatalf("origin has %d characters", len(got))
	}
}

func TestLoginEventsTruncateTheUsername(t *testing.T) {
	logger, read := capture(t, slog.LevelInfo)
	logger.LoginThrottled(testInfo, strings.Repeat("u", 1000))
	record := read()
	if got := record["username"].(string); len(got) != maxUsernameLen {
		t.Fatalf("username has %d characters", len(got))
	}
	expect(t, record, map[string]any{"event": "auth.login.throttled", "level": "WARN"})
}

func TestEventNamesAndLevels(t *testing.T) {
	reauth := Reauth{ActorSessionFP: "fp", UserID: "u1", Action: "password_change"}
	cases := []struct {
		name  string
		emit  func(*Logger)
		event string
		level string
		extra map[string]any
	}{
		{"setup throttled", func(l *Logger) { l.SetupThrottled(testInfo) }, "auth.setup.throttled", "WARN", nil},
		{"setup token rejected", func(l *Logger) { l.SetupTokenRejected(testInfo) }, "auth.setup.token_rejected", "WARN", nil},
		{"reauth failed", func(l *Logger) { l.ReauthFailed(testInfo, reauth) }, "auth.reauth.failed", "WARN",
			map[string]any{"actor_session_fp": "fp", "user_id": "u1", "action": "password_change"}},
		{"reauth throttled", func(l *Logger) { l.ReauthThrottled(testInfo, reauth) }, "auth.reauth.throttled", "WARN",
			map[string]any{"actor_session_fp": "fp"}},
		{"session created", func(l *Logger) {
			l.SessionCreated(testInfo, SessionCreated{SessionFP: "fp", UserID: "u1", Username: "alice", Reason: "login"})
		}, "session.created", "INFO", map[string]any{"session_fp": "fp", "username": "alice", "reason": "login"}},
		{"session renewed", func(l *Logger) { l.SessionRenewed(testInfo, "fp", "u1", "alice") }, "session.renewed", "INFO",
			map[string]any{"session_fp": "fp", "user_id": "u1"}},
		{"session destroyed", func(l *Logger) { l.SessionDestroyed(testInfo, "fp", "u1", "logout") }, "session.destroyed", "INFO",
			map[string]any{"session_fp": "fp", "reason": "logout"}},
		{"bulk destroyed", func(l *Logger) {
			l.SessionsDestroyedBulk(testInfo, BulkSessionsDestroyed{ActorSessionFP: "fp", UserID: "u1", Count: 3, Reason: "revoke_all"})
		}, "session.destroyed", "INFO", map[string]any{"actor_session_fp": "fp", "count": float64(3)}},
		{"session expired", func(l *Logger) { l.SessionExpired(testInfo, "fp", "idle") }, "session.expired", "INFO",
			map[string]any{"session_fp": "fp", "reason": "idle"}},
		{"sweep", func(l *Logger) { l.SessionsExpiredSweep(4) }, "session.expired", "INFO",
			map[string]any{"count": float64(4), "reason": "sweep"}},
		{"session rejected", func(l *Logger) { l.SessionRejected(testInfo, "fp") }, "session.rejected", "DEBUG",
			map[string]any{"reason": "unknown_token"}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			logger, read := capture(t, slog.LevelDebug)
			tc.emit(logger)
			record := read()
			expect(t, record, map[string]any{"event": tc.event, "level": tc.level, "target": "liftlog::audit"})
			expect(t, record, tc.extra)
		})
	}
}

func TestSweepHasNoRequestFieldsAndBulkHasNoSessionFingerprint(t *testing.T) {
	logger, read := capture(t, slog.LevelInfo)
	logger.SessionsExpiredSweep(1)
	if _, present := read()["client_ip"]; present {
		t.Error("the sweep has no request context")
	}

	logger, read = capture(t, slog.LevelInfo)
	logger.SessionsDestroyedBulk(testInfo, BulkSessionsDestroyed{ActorSessionFP: "fp", UserID: "u1", Count: 1, Reason: "r"})
	if _, present := read()["session_fp"]; present {
		t.Error("a bulk event names the actor, not a dead session")
	}
}

func TestSessionRejectedIsHiddenAtInfoLevel(t *testing.T) {
	var out bytes.Buffer
	NewLogger(slog.New(slog.NewJSONHandler(&out, nil))).SessionRejected(testInfo, "fp")
	if out.Len() != 0 {
		t.Fatalf("debug event logged at info level: %s", out.String())
	}
}

func TestRequestInfoStripsTheIPv6Zone(t *testing.T) {
	info := NewRequestInfo(newRequest(t, "[fe80::1%eth0]:1", ""), ClientSource{})
	if got := info.ClientIP.String(); got != "fe80::1" {
		t.Fatalf("client ip = %q", got)
	}
}

func TestRequestInfoLogsTheEscapedPath(t *testing.T) {
	request := newRequest(t, "127.0.0.1:1", "")
	request.URL.Path = "/a\nb/c d"
	request.URL.RawPath = ""
	if got := NewRequestInfo(request, ClientSource{}).Path; got != "/a%0Ab/c%20d" {
		t.Fatalf("path = %q", got)
	}
}
