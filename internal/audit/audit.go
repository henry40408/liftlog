// Package audit emits structured audit events (session lifecycle, auth
// failures, CSRF rejections) with the attribute target=liftlog::audit.
//
// Raw session tokens are never logged; events carry session_fp, a salted
// fingerprint (session.TokenFingerprint). The salt is per process, so
// fingerprints correlate within one run but not across restarts.
package audit

import (
	"context"
	"log/slog"
	"net/http"
	"net/netip"

	"github.com/henry40408/liftlog/internal/auth"
	"github.com/henry40408/liftlog/internal/config"
)

const target = "liftlog::audit"

// Caps on attacker-controlled, unvalidated fields, so one request cannot bloat
// log lines.
const (
	maxUserAgentLen = 256
	maxUsernameLen  = 256
	maxOriginLen    = 256
)

// RequestInfo is what every request-scoped event reports about the request.
type RequestInfo struct {
	ClientIP netip.Addr
	// UserAgent is empty when the header is absent.
	UserAgent string
	Path      string
}

// ClientSource says how to resolve the client address of a request.
type ClientSource struct {
	TrustedProxyHeader config.TrustedProxyHeader
	TrustedProxies     []netip.Addr
}

// NewRequestInfo describes r. A request without a parsable peer address is
// attributed to loopback.
func NewRequestInfo(r *http.Request, source ClientSource) RequestInfo {
	var peer netip.Addr
	if addrPort, err := netip.ParseAddrPort(r.RemoteAddr); err == nil {
		// A zone ("fe80::1%eth0") would leak into logs and break comparisons.
		peer = addrPort.Addr().WithZone("")
	}
	return RequestInfo{
		ClientIP:  auth.ClientIP(peer, r.Header, source.TrustedProxyHeader, source.TrustedProxies),
		UserAgent: truncateChars(r.UserAgent(), maxUserAgentLen),
		// Escaped, so a hostile path cannot forge log text and matches the raw
		// path the Rust server logged.
		Path: r.URL.EscapedPath(),
	}
}

// truncateChars counts characters, so non-ASCII input is never cut mid-rune.
func truncateChars(s string, maxChars int) string {
	count := 0
	for index := range s {
		if count == maxChars {
			return s[:index]
		}
		count++
	}
	return s
}

// Logger writes audit events to an injected slog.Logger.
type Logger struct {
	log *slog.Logger
}

func NewLogger(log *slog.Logger) *Logger {
	return &Logger{log: log}
}

func (l *Logger) emit(level slog.Level, message string, attrs ...slog.Attr) {
	all := append([]slog.Attr{slog.String("target", target)}, attrs...)
	l.log.LogAttrs(context.Background(), level, message, all...)
}

// requestAttrs are the trailing client_ip, user_agent and path attributes.
func requestAttrs(info RequestInfo) []slog.Attr {
	attrs := []slog.Attr{slog.String("client_ip", info.ClientIP.String())}
	if info.UserAgent != "" {
		attrs = append(attrs, slog.String("user_agent", info.UserAgent))
	}
	return append(attrs, slog.String("path", info.Path))
}

func event(name string, attrs ...slog.Attr) []slog.Attr {
	return append([]slog.Attr{slog.String("event", name)}, attrs...)
}

func join(groups ...[]slog.Attr) []slog.Attr {
	var all []slog.Attr
	for _, group := range groups {
		all = append(all, group...)
	}
	return all
}

// LoginFailed is warn: the primary brute-force signal. The attempted username
// is logged to identify the targeted account, accepting that a password typed
// into the username field lands in the log. backoffMS is the per-account delay
// applied to this attempt.
func (l *Logger) LoginFailed(info RequestInfo, username string, backoffMS int64) {
	l.emit(slog.LevelWarn, "login failed", join(
		event("auth.login.failed",
			slog.String("username", truncateChars(username, maxUsernameLen)),
			slog.Int64("backoff_ms", backoffMS)),
		requestAttrs(info))...)
}

// LoginThrottled is a refusal by the per-IP limiter before any credential was
// checked.
func (l *Logger) LoginThrottled(info RequestInfo, username string) {
	l.emit(slog.LevelWarn, "login throttled", join(
		event("auth.login.throttled",
			slog.String("username", truncateChars(username, maxUsernameLen))),
		requestAttrs(info))...)
}

// SetupThrottled is a refusal by the setup limiter before the token was
// checked.
func (l *Logger) SetupThrottled(info RequestInfo) {
	l.emit(slog.LevelWarn, "setup throttled", join(event("auth.setup.throttled"), requestAttrs(info))...)
}

// SetupTokenRejected is a wrong or missing first-run setup token.
func (l *Logger) SetupTokenRejected(info RequestInfo) {
	l.emit(slog.LevelWarn, "setup token rejected", join(event("auth.setup.token_rejected"), requestAttrs(info))...)
}

// Reauth identifies an authenticated password re-check: where a stolen-cookie
// holder would guess. Action names the route; one event family keeps alerting
// on one signal.
type Reauth struct {
	ActorSessionFP string
	UserID         string
	Action         string
}

func (r Reauth) attrs() []slog.Attr {
	return []slog.Attr{
		slog.String("actor_session_fp", r.ActorSessionFP),
		slog.String("user_id", r.UserID),
		slog.String("action", r.Action),
	}
}

// ReauthFailed is a wrong password on a re-check (password change, admin
// promote or delete).
func (l *Logger) ReauthFailed(info RequestInfo, reauth Reauth) {
	l.emit(slog.LevelWarn, "re-authentication rejected: password incorrect", join(
		event("auth.reauth.failed"), reauth.attrs(), requestAttrs(info))...)
}

// ReauthThrottled is a refusal by the per-user throttle, whose budget is shared
// across routes.
func (l *Logger) ReauthThrottled(info RequestInfo, reauth Reauth) {
	l.emit(slog.LevelWarn, "re-authentication throttled", join(
		event("auth.reauth.throttled"), reauth.attrs(), requestAttrs(info))...)
}

type SessionCreated struct {
	SessionFP string
	UserID    string
	Username  string
	Reason    string
}

func (l *Logger) SessionCreated(info RequestInfo, created SessionCreated) {
	l.emit(slog.LevelInfo, "session created", join(
		event("session.created",
			slog.String("session_fp", created.SessionFP),
			slog.String("user_id", created.UserID),
			slog.String("username", created.Username)),
		requestAttrs(info),
		[]slog.Attr{slog.String("reason", created.Reason)})...)
}

func (l *Logger) SessionRenewed(info RequestInfo, sessionFP, userID, username string) {
	l.emit(slog.LevelInfo, "session renewed", join(
		event("session.renewed",
			slog.String("session_fp", sessionFP),
			slog.String("user_id", userID),
			slog.String("username", username)),
		requestAttrs(info))...)
}

func (l *Logger) SessionDestroyed(info RequestInfo, sessionFP, userID, reason string) {
	l.emit(slog.LevelInfo, "session destroyed", join(
		event("session.destroyed",
			slog.String("session_fp", sessionFP),
			slog.String("user_id", userID)),
		requestAttrs(info),
		[]slog.Attr{slog.String("reason", reason)})...)
}

type BulkSessionsDestroyed struct {
	ActorSessionFP string
	UserID         string
	Count          int
	Reason         string
}

// SessionsDestroyedBulk has no session_fp: actor_session_fp is who acted, not
// a session that died.
func (l *Logger) SessionsDestroyedBulk(info RequestInfo, destroyed BulkSessionsDestroyed) {
	l.emit(slog.LevelInfo, "sessions destroyed (bulk)", join(
		event("session.destroyed",
			slog.String("actor_session_fp", destroyed.ActorSessionFP),
			slog.String("user_id", destroyed.UserID),
			slog.Int("count", destroyed.Count)),
		requestAttrs(info),
		[]slog.Attr{slog.String("reason", destroyed.Reason)})...)
}

func (l *Logger) SessionExpired(info RequestInfo, sessionFP, reason string) {
	l.emit(slog.LevelInfo, "session expired", join(
		event("session.expired", slog.String("session_fp", sessionFP)),
		requestAttrs(info),
		[]slog.Attr{slog.String("reason", reason)})...)
}

// SessionsExpiredSweep comes from the background sweep: no request context and
// no per-session fingerprint.
func (l *Logger) SessionsExpiredSweep(count int) {
	l.emit(slog.LevelInfo, "expired sessions retired by the background sweep", join(
		event("session.expired",
			slog.Int("count", count),
			slog.String("reason", "sweep")))...)
}

// SessionRejected is debug: scanners replaying random cookies would drown the
// useful events.
func (l *Logger) SessionRejected(info RequestInfo, sessionFP string) {
	l.emit(slog.LevelDebug, "session rejected", join(
		event("session.rejected", slog.String("session_fp", sessionFP)),
		requestAttrs(info),
		[]slog.Attr{slog.String("reason", "unknown_token")})...)
}

// CSRFRejected is a request refused by the CSRF guard. It is warn: either an
// attack or a misconfigured proxy. reason is sec_fetch_site (browser-declared)
// or origin_fallback (Origin/Host mismatch: suspect the proxy first). origin is
// empty when the request had no Origin header.
func (l *Logger) CSRFRejected(info RequestInfo, reason, method, origin string) {
	attrs := []slog.Attr{slog.String("reason", reason), slog.String("method", method)}
	if origin != "" {
		attrs = append(attrs, slog.String("origin", truncateChars(origin, maxOriginLen)))
	}
	l.emit(slog.LevelWarn, "cross-site request rejected", join(
		event("csrf.rejected", attrs...), requestAttrs(info))...)
}
