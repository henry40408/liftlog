package server

import (
	"context"
	"net/http"

	"github.com/henry40408/liftlog/internal/audit"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/session"
	"github.com/henry40408/liftlog/internal/views"
)

// ValidatedSession is the signed-in user's identity, taken from the session
// row the middleware validated, so handlers need no users lookup.
type ValidatedSession struct {
	UserID       string
	Username     string
	Role         models.UserRole
	SessionToken string
}

func (s ValidatedSession) IsAdmin() bool { return s.Role.IsAdmin() }

// Nav is what the views' navigation bar shows.
func (s ValidatedSession) Nav() views.NavUser {
	return views.NavUser{Username: s.Username, IsAdmin: s.IsAdmin()}
}

// requestState is the per-request session state the middleware shares with
// handlers. It is created for every request.
type requestState struct {
	user *ValidatedSession
	// refreshToken is set when the session cookie must be re-issued.
	refreshToken string
	// suppressRefresh stops the re-issue, so logout's removal cookie survives.
	suppressRefresh bool
}

type requestStateKey struct{}

func stateOf(r *http.Request) *requestState {
	state, _ := r.Context().Value(requestStateKey{}).(*requestState)
	if state == nil {
		// A silent fallback would make every caller look anonymous.
		panic("server: request has no session state; sessionMiddleware did not run")
	}
	return state
}

func withState(r *http.Request, state *requestState) *http.Request {
	return r.WithContext(context.WithValue(r.Context(), requestStateKey{}, state))
}

// currentUser is the signed-in user, if any.
func currentUser(r *http.Request) (ValidatedSession, bool) {
	user := stateOf(r).user
	if user == nil {
		return ValidatedSession{}, false
	}
	return *user, true
}

// requireUser redirects anonymous requests to the login page. When it returns
// false the response is already written.
func requireUser(w http.ResponseWriter, r *http.Request) (ValidatedSession, bool) {
	user, ok := currentUser(r)
	if !ok {
		redirect(w, "/auth/login")
		return ValidatedSession{}, false
	}
	return user, true
}

// requireAdmin is requireUser plus a 403 for non-admins. When it returns false
// the response is already written.
func requireAdmin(w http.ResponseWriter, r *http.Request) (ValidatedSession, bool) {
	user, ok := requireUser(w, r)
	if !ok {
		return ValidatedSession{}, false
	}
	if !user.IsAdmin() {
		writeText(w, http.StatusForbidden, "Admin access required")
		return ValidatedSession{}, false
	}
	return user, true
}

// suppressSessionRefresh keeps the middleware from re-issuing the session
// cookie on this response.
func suppressSessionRefresh(r *http.Request) {
	stateOf(r).suppressRefresh = true
}

// requestInfo describes r for audit events.
func (a *App) requestInfo(r *http.Request) audit.RequestInfo {
	return audit.NewRequestInfo(r, a.clientSource())
}

// fingerprint is the audit-safe stand-in for a session token.
func (a *App) fingerprint(token string) string {
	return session.TokenFingerprint(token, a.logSalt)
}
