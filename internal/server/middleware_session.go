package server

import (
	"net/http"

	"github.com/henry40408/liftlog/internal/repo"
	"github.com/henry40408/liftlog/internal/session"
)

// sessionMiddleware validates the session cookie on every request, slides its
// expiry, and stores the result for requireUser/requireAdmin. It never blocks a
// request itself; handlers decide what an anonymous caller may do.
func (a *App) sessionMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		state := &requestState{}
		r = withState(r, state)
		a.identify(r, state)

		next.ServeHTTP(&sessionWriter{ResponseWriter: w, app: a, state: state}, r)
	})
}

// identify fills state from the session cookie, if there is a valid one.
func (a *App) identify(r *http.Request, state *requestState) {
	token, ok := session.TokenFromRequest(r, a.cfg.CookieSecure)
	if !ok || token == "" {
		return
	}
	info := a.requestInfo(r)
	fingerprint := a.fingerprint(token)

	result, err := a.sessions.ValidateAndTouch(r.Context(), token)
	if err != nil {
		a.logger.Warn("session validation failed", "err", err)
		return
	}
	switch result.Kind {
	case repo.ValidateValid:
		state.user = &ValidatedSession{
			UserID:       result.UserID,
			Username:     result.Username,
			Role:         result.Role,
			SessionToken: token,
		}
		if result.NewExpiresAt != nil {
			state.refreshToken = token
			a.audit.SessionRenewed(info, fingerprint, result.UserID, result.Username)
		}
	case repo.ValidateExpiredIdle:
		a.audit.SessionExpired(info, fingerprint, "idle")
	case repo.ValidateExpiredAbsolute:
		a.audit.SessionExpired(info, fingerprint, "absolute")
	case repo.ValidateUnknown:
		a.audit.SessionRejected(info, fingerprint)
	}
}

// sessionWriter adds the session-dependent headers just before the response
// headers go out, because handlers decide late whether to suppress a refresh.
type sessionWriter struct {
	http.ResponseWriter
	app         *App
	state       *requestState
	headersSent bool
}

func (w *sessionWriter) WriteHeader(status int) {
	w.prepare()
	w.ResponseWriter.WriteHeader(status)
}

func (w *sessionWriter) Write(body []byte) (int, error) {
	w.prepare()
	return w.ResponseWriter.Write(body)
}

// Unwrap lets http.ResponseController reach the real writer.
func (w *sessionWriter) Unwrap() http.ResponseWriter { return w.ResponseWriter }

func (w *sessionWriter) prepare() {
	if w.headersSent {
		return
	}
	w.headersSent = true
	if w.state.refreshToken != "" && !w.state.suppressRefresh {
		http.SetCookie(w.ResponseWriter, session.NewCookie(w.state.refreshToken, w.app.cfg.CookieSecure))
	}
	if w.state.user != nil && w.Header().Get("Cache-Control") == "" {
		setNoStore(w.Header())
	}
}

// setNoStore keeps authenticated pages out of shared and browser caches.
func setNoStore(header http.Header) {
	header.Set("Cache-Control", "no-cache, no-store, must-revalidate")
	header.Set("Pragma", "no-cache")
}
