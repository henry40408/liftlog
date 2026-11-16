package server

import (
	"fmt"
	"net/http"
	"strings"

	"github.com/henry40408/liftlog/internal/audit"
	"github.com/henry40408/liftlog/internal/auth"
	"github.com/henry40408/liftlog/internal/session"
	"github.com/henry40408/liftlog/internal/timezone"
	"github.com/henry40408/liftlog/internal/version"
	"github.com/henry40408/liftlog/internal/views"
)

const (
	unknownTimezoneMessage      = "Unknown timezone. Pick a name such as Asia/Taipei."
	passwordThrottledMessage    = "Too many password change attempts. Please try again later."
	passwordChangedMessage      = "Password changed successfully. All other sessions have been logged out."
	loggedOutOthersMessage      = "Logged out of all other devices."
	timezoneUpdatedMessage      = "Timezone updated."
	passwordMismatchMessage     = "New passwords do not match"
	passwordUnchangedMessage    = "New password must be different from the current password"
	wrongCurrentPasswordMessage = "Current password is incorrect"
)

// settingsNotice is what the settings page shows above its sections. Status
// 0 means 200, so a throttled change can read as refused.
type settingsNotice struct {
	status  int
	error   string
	success string
}

func (a *App) settingsPage(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	a.renderSettings(w, r, user, settingsNotice{})
}

// renderSettings lists the user's sessions marked by user.SessionToken, so
// callers that rotated the token pass the new one.
func (a *App) renderSettings(w http.ResponseWriter, r *http.Request, user ValidatedSession, notice settingsNotice) {
	rows, err := a.sessions.ListForUser(r.Context(), user.UserID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	zone, err := a.users.FindTimezone(r.Context(), user.UserID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	status := notice.status
	if status == 0 {
		status = http.StatusOK
	}
	sessions := make([]views.SessionRow, 0, len(rows))
	for _, row := range rows {
		sessions = append(sessions, views.SessionRow{Token: row.Token, CreatedAt: row.CreatedAt, LastTouchedAt: row.LastTouchedAt})
	}
	view := views.SettingsView{
		User:                user.Nav(),
		TimezoneNames:       timezone.Names(),
		GitVersion:          version.Version(),
		Error:               notice.error,
		Success:             notice.success,
		Sessions:            sessions,
		CurrentSessionToken: user.SessionToken,
	}
	if zone != nil {
		view.Timezone = *zone
	}
	writeHTML(w, status, views.Settings(view))
}

func (a *App) changeTimezone(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	form, err := readForm(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	submitted, err := form.required("timezone")
	if err != nil {
		a.fail(w, r, err)
		return
	}
	name := strings.TrimSpace(submitted)
	if _, valid := timezone.Parse(name); !valid {
		a.renderSettings(w, r, user, settingsNotice{status: http.StatusBadRequest, error: unknownTimezoneMessage})
		return
	}
	if _, err := a.users.SetTimezone(r.Context(), user.UserID, name); err != nil {
		a.fail(w, r, err)
		return
	}
	a.renderSettings(w, r, user, settingsNotice{success: timezoneUpdatedMessage})
}

// passwordChange is the submitted change-password form.
type passwordChange struct {
	current string
	next    string
	confirm string
}

func readPasswordChange(w http.ResponseWriter, r *http.Request) (passwordChange, error) {
	form, err := readForm(w, r)
	if err != nil {
		return passwordChange{}, err
	}
	var change passwordChange
	if change.current, err = form.required("current_password"); err != nil {
		return passwordChange{}, err
	}
	if change.next, err = form.required("new_password"); err != nil {
		return passwordChange{}, err
	}
	if change.confirm, err = form.required("confirm_password"); err != nil {
		return passwordChange{}, err
	}
	return change, nil
}

// newPasswordError is the first reason the new password is refused, or "".
func (c passwordChange) newPasswordError(username string) string {
	if c.next != c.confirm {
		return passwordMismatchMessage
	}
	if c.next == c.current {
		// Otherwise a no-op "change" would report success and drop sessions.
		return passwordUnchangedMessage
	}
	return auth.PasswordPolicyError(c.next, "New password", []string{username})
}

func (a *App) changePassword(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	change, err := readPasswordChange(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if message := change.newPasswordError(user.Username); message != "" {
		a.renderSettings(w, r, user, settingsNotice{error: message})
		return
	}
	if !a.reauthForPasswordChange(w, r, user, change.current) {
		return
	}

	hash, err := a.hasher.Hash(change.next)
	if err != nil {
		a.fail(w, r, internalError(err))
		return
	}
	if _, err := a.users.ChangePassword(r.Context(), user.UserID, hash); err != nil {
		a.fail(w, r, err)
		return
	}
	a.throttles.SensitiveAction.Release(user.UserID)

	newToken, err := a.rotateSessionAfterPasswordChange(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	// The page marks "This device" by token, so render with the new one. The
	// middleware's refresh of the deleted old token would overwrite our cookie
	// and sign the user out, so it is suppressed.
	http.SetCookie(w, session.NewCookie(newToken, a.cfg.CookieSecure))
	suppressSessionRefresh(r)
	user.SessionToken = newToken
	a.renderSettings(w, r, user, settingsNotice{success: passwordChangedMessage})
}

// reauthForPasswordChange checks the current password. When it returns false
// the settings page, with the reason, is already written.
func (a *App) reauthForPasswordChange(w http.ResponseWriter, r *http.Request, user ValidatedSession, password string) bool {
	outcome, err := a.checkPassword(r, user, password, "password_change")
	if err != nil {
		a.fail(w, r, err)
		return false
	}
	switch outcome {
	case reauthVerified:
		return true
	case reauthThrottled:
		a.renderSettings(w, r, user, settingsNotice{status: http.StatusTooManyRequests, error: passwordThrottledMessage})
	default:
		a.renderSettings(w, r, user, settingsNotice{error: wrongCurrentPasswordMessage})
	}
	return false
}

// rotateSessionAfterPasswordChange rotates the current token too (OWASP: renew
// after risk events), so a token captured before the change stops working.
// Create first: if it fails the user keeps their old session. Excluding only
// the new token retires the old one with every other device in one statement.
func (a *App) rotateSessionAfterPasswordChange(r *http.Request, user ValidatedSession) (string, error) {
	newToken, err := a.sessions.Create(r.Context(), user.UserID)
	if err != nil {
		return "", err
	}
	deleted, err := a.sessions.DeleteAllForUserExcept(r.Context(), user.UserID, newToken)
	if err != nil {
		return "", err
	}
	info := a.requestInfo(r)
	// The count includes the rotated-away session.
	a.audit.SessionsDestroyedBulk(info, audit.BulkSessionsDestroyed{
		ActorSessionFP: a.fingerprint(user.SessionToken),
		UserID:         user.UserID,
		Count:          int(deleted),
		Reason:         "password_change",
	})
	a.audit.SessionCreated(info, audit.SessionCreated{
		SessionFP: a.fingerprint(newToken),
		UserID:    user.UserID,
		Username:  user.Username,
		Reason:    "password_change_rotation",
	})
	return newToken, nil
}

func (a *App) confirmLogoutOthers(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	rows, err := a.sessions.ListForUser(r.Context(), user.UserID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	others := 0
	for _, row := range rows {
		if row.Token != user.SessionToken {
			others++
		}
	}
	writeConfirmPage(w, user, confirmPage{
		ActionLabel: "Log out other devices",
		Consequence: logoutOthersConsequence(others),
		FormAction:  "/settings/logout-others",
		CancelURL:   "/settings",
	})
}

func logoutOthersConsequence(others int) string {
	switch others {
	case 0:
		return "No other device is signed in, so nothing will be logged out."
	case 1:
		return "1 other signed-in device will be logged out. This device stays signed in."
	}
	return fmt.Sprintf("%d other signed-in devices will be logged out. This device stays signed in.", others)
}

func (a *App) logoutOthers(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	deleted, err := a.sessions.DeleteAllForUserExcept(r.Context(), user.UserID, user.SessionToken)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	a.audit.SessionsDestroyedBulk(a.requestInfo(r), audit.BulkSessionsDestroyed{
		ActorSessionFP: a.fingerprint(user.SessionToken),
		UserID:         user.UserID,
		Count:          int(deleted),
		Reason:         "logout_others",
	})
	a.renderSettings(w, r, user, settingsNotice{success: loggedOutOthersMessage})
}
