// Package session holds the session lifetime rules and the session cookie.
package session

import (
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"time"
)

const (
	cookieNamePlain = "session"
	cookieNameHost  = "__Host-session"

	IdleTTL = 7 * 24 * time.Hour

	// TouchThrottle bounds writes to at most one `last_touched_at` update per
	// session per window.
	TouchThrottle = time.Hour

	// AbsoluteTTL is the lifetime ceiling from `created_at`, not extendable by
	// activity. Far above OWASP's 4-8h on purpose: a gym journal that logs out
	// mid-workout gets worked around; what matters is that a ceiling exists.
	AbsoluteTTL = 90 * 24 * time.Hour
)

func AbsoluteCap(createdAt time.Time) time.Time {
	return createdAt.Add(AbsoluteTTL)
}

type TouchKind int

const (
	// TouchNothing: inside the touch-throttle window, write nothing.
	TouchNothing TouchKind = iota
	// TouchOnly: outside the throttle, but `expires_at` is already pinned to
	// the absolute cap, so record the activity without extending the lifetime.
	TouchOnly
	// TouchSlide: outside the throttle and below the cap; slide `expires_at`
	// to TouchAction.ExpiresAt, which is never past the absolute cap.
	TouchSlide
)

// TouchAction is what a touch on a session writes back to its row.
type TouchAction struct {
	Kind      TouchKind
	ExpiresAt time.Time
}

// SessionTimes are the stored instants a touch decision depends on.
type SessionTimes struct {
	CreatedAt     time.Time
	LastTouchedAt time.Time
	ExpiresAt     time.Time
}

// ComputeTouchAction checks the throttle first, even for a session pinned at
// the cap, or pinned sessions would write on every request. A pinned session
// still gets TouchOnly so /settings shows its real "last active".
func ComputeTouchAction(s SessionTimes, now time.Time) TouchAction {
	if now.Sub(s.LastTouchedAt) <= TouchThrottle {
		return TouchAction{Kind: TouchNothing}
	}

	limit := AbsoluteCap(s.CreatedAt)
	if !s.ExpiresAt.Before(limit) {
		return TouchAction{Kind: TouchOnly}
	}

	slid := now.Add(IdleTTL)
	if slid.After(limit) {
		slid = limit
	}
	return TouchAction{Kind: TouchSlide, ExpiresAt: slid}
}

// CookieName uses `__Host-` only when secure: over plain HTTP the browser
// silently drops a `__Host-` cookie, and nobody can log in.
func CookieName(secure bool) string {
	if secure {
		return cookieNameHost
	}
	return cookieNamePlain
}

func NewCookie(token string, secure bool) *http.Cookie {
	return sessionCookie(token, secure, int(IdleTTL/time.Second))
}

// RemovalCookie must carry the attributes of NewCookie: a `__Host-` removal
// lacking `Secure`/`Path=/`, or a `Secure` one over HTTP, is discarded and the
// cookie survives logout.
func RemovalCookie(secure bool) *http.Cookie {
	return sessionCookie("", secure, -1) // negative MaxAge renders as Max-Age=0
}

func sessionCookie(value string, secure bool, maxAge int) *http.Cookie {
	return &http.Cookie{
		Name:     CookieName(secure),
		Value:    value,
		Path:     "/",
		MaxAge:   maxAge,
		HttpOnly: true,
		Secure:   secure,
		SameSite: http.SameSiteLaxMode,
	}
}

// TokenFromRequest does not fall back to the other cookie name: accepting a
// bare `session` under `secure` would let a sibling subdomain inject one.
// Flipping the setting logs everyone out once.
func TokenFromRequest(r *http.Request, secure bool) (string, bool) {
	cookies := r.CookiesNamed(CookieName(secure))
	if len(cookies) == 0 {
		return "", false
	}
	// The last one wins, like the Rust cookie jar.
	return cookies[len(cookies)-1].Value, true
}

// TokenFingerprint is a salted SHA-256, truncated to 16 hex chars, so logs
// can correlate a session without containing its token.
func TokenFingerprint(token string, salt []byte) string {
	hash := sha256.New()
	hash.Write(salt)
	hash.Write([]byte(token))
	return hex.EncodeToString(hash.Sum(nil)[:8])
}
