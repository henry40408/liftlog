package server

import (
	"net/http"
	"strings"
	"time"

	"github.com/henry40408/liftlog/internal/audit"
	"github.com/henry40408/liftlog/internal/auth"
	"github.com/henry40408/liftlog/internal/views"
)

func (a *App) authRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /auth/login", a.loginPage)
	mux.HandleFunc("POST /auth/login", a.loginSubmit)
	mux.HandleFunc("GET /auth/setup", a.setupPage)
	mux.HandleFunc("POST /auth/setup", a.setupSubmit)
	mux.HandleFunc("POST /auth/logout", a.logout)
}

// validateCredentials is the message for a bad username or password, or "".
// Setup, user creation and password change share one password policy.
func validateCredentials(username, password string) string {
	if message := auth.UsernameError(username); message != "" {
		return message
	}
	return auth.PasswordPolicyError(password, "Password", []string{strings.TrimSpace(username)})
}

func (a *App) loginPage(w http.ResponseWriter, r *http.Request) {
	if _, signedIn := currentUser(r); signedIn {
		redirect(w, "/")
		return
	}
	count, err := a.users.Count(r.Context())
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if count == 0 {
		redirect(w, "/auth/setup")
		return
	}
	writeHTML(w, http.StatusOK, views.Login(""))
}

func writeLoginError(w http.ResponseWriter, status int, message string) {
	writeHTML(w, status, views.Login(message))
}

// redirectSignedIn answers a login or setup success. Those responses set the
// session cookie but carry no session yet, so the session middleware does not
// mark them uncacheable; do it here. A caller who is already signed in may be
// due a slide, so the old token's refresh cookie is suppressed: it would
// follow the new one and, as the last cookie, win in the browser.
func (a *App) redirectSignedIn(w http.ResponseWriter, r *http.Request, token string) {
	suppressSessionRefresh(r)
	a.setSessionCookie(w, token)
	setNoStore(w.Header())
	redirect(w, "/")
}

func (a *App) loginSubmit(w http.ResponseWriter, r *http.Request) {
	form, err := readForm(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	username, err := form.required("username")
	if err != nil {
		a.fail(w, r, err)
		return
	}
	password, err := form.required("password")
	if err != nil {
		a.fail(w, r, err)
		return
	}

	info := a.requestInfo(r)
	ip := info.ClientIP
	// The username is deliberately not trimmed: accounts created before
	// creation trimmed names may carry spaces and must keep logging in.
	if !a.throttles.Login.TryAcquire(ip) {
		a.audit.LoginThrottled(info, username)
		writeLoginError(w, http.StatusTooManyRequests, "Too many login attempts. Please try again later.")
		return
	}

	// The delay applies to any submitted username, real or not, so it cannot
	// reveal account existence; holding the connection is what rate-limits.
	backoff := a.throttles.LoginBackoff.DelayFor(username)
	if !sleepContext(r, backoff) {
		return
	}

	user, verified, err := a.verifyPassword(r, username, password)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if !verified {
		// One event for unknown user and wrong password alike, so the log is
		// not an enumeration oracle.
		a.throttles.LoginBackoff.RecordFailure(username)
		a.audit.LoginFailed(info, username, backoff.Milliseconds())
		writeLoginError(w, http.StatusOK, "Invalid username or password")
		return
	}

	a.throttles.LoginBackoff.Reset(username)
	// Create the session before releasing the reservation, so a failed create
	// stays charged.
	token, err := a.sessions.Create(r.Context(), user.ID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	a.throttles.Login.Release(ip)
	a.audit.SessionCreated(info, audit.SessionCreated{
		SessionFP: a.fingerprint(token),
		UserID:    user.ID,
		Username:  user.Username,
		Reason:    "login",
	})
	a.redirectSignedIn(w, r, token)
}

// sleepContext waits for delay and is false when the client left first.
func sleepContext(r *http.Request, delay time.Duration) bool {
	if delay <= 0 {
		return true
	}
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-timer.C:
		return true
	case <-r.Context().Done():
		return false
	}
}

func (a *App) setupPage(w http.ResponseWriter, r *http.Request) {
	count, err := a.users.Count(r.Context())
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if count > 0 {
		redirect(w, "/auth/login")
		return
	}
	writeHTML(w, http.StatusOK, views.Setup(""))
}

func writeSetupError(w http.ResponseWriter, status int, message string) {
	writeHTML(w, status, views.Setup(message))
}

func (a *App) setupSubmit(w http.ResponseWriter, r *http.Request) {
	form, err := readForm(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	username, err := form.required("username")
	if err != nil {
		a.fail(w, r, err)
		return
	}
	password, err := form.required("password")
	if err != nil {
		a.fail(w, r, err)
		return
	}
	setupToken := form.optional("setup_token")

	count, err := a.users.Count(r.Context())
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if count > 0 {
		redirect(w, "/auth/login")
		return
	}

	info := a.requestInfo(r)
	ip := info.ClientIP
	// Refunded only after the token checked out: the token is guessable only
	// online, so a wrong guess always stays charged.
	if !a.throttles.Setup.TryAcquire(ip) {
		a.audit.SetupThrottled(info)
		writeSetupError(w, http.StatusTooManyRequests, "Too many setup attempts. Please try again later.")
		return
	}
	if !a.setupToken.Verify(setupToken) {
		a.audit.SetupTokenRejected(info)
		writeSetupError(w, http.StatusForbidden, "Invalid setup token. Find it on the server's stderr.")
		return
	}

	username = strings.TrimSpace(username)
	if message := validateCredentials(username, password); message != "" {
		// The token was right, so this attempt guessed nothing; refund it so a
		// typo does not burn the budget.
		a.throttles.Setup.Release(ip)
		writeSetupError(w, http.StatusOK, message)
		return
	}

	hash, err := a.hasher.Hash(password)
	if err != nil {
		a.fail(w, r, internalError(err))
		return
	}
	// The count check above is only a fast path; this insert is the real gate.
	user, created, err := a.users.CreateFirstAdmin(r.Context(), username, hash)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if !created {
		redirect(w, "/auth/login")
		return
	}
	a.setupToken.Clear()

	token, err := a.sessions.Create(r.Context(), user.ID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	a.audit.SessionCreated(info, audit.SessionCreated{
		SessionFP: a.fingerprint(token),
		UserID:    user.ID,
		Username:  user.Username,
		Reason:    "setup",
	})
	a.redirectSignedIn(w, r, token)
}

func (a *App) logout(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	if err := a.sessions.Delete(r.Context(), user.SessionToken); err != nil {
		a.logger.Warn("logout: session delete failed", "err", err)
	} else {
		a.audit.SessionDestroyed(a.requestInfo(r), a.fingerprint(user.SessionToken), user.UserID, "logout")
	}
	a.clearSessionCookie(w)
	suppressSessionRefresh(r)
	// The values must be quoted or browsers ignore the header. "executionContexts"
	// is omitted: its reload clashes with the redirect. Browsers ignore the
	// header on insecure origins, so it is not gated on cookie_secure.
	w.Header().Set("Clear-Site-Data", `"cache", "cookies", "storage"`)
	redirect(w, "/auth/login")
}
