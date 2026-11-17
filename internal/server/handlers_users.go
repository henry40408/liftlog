package server

import (
	"errors"
	"net/http"
	"strings"

	"github.com/henry40408/liftlog/internal/audit"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/repo"
	"github.com/henry40408/liftlog/internal/views"
)

func (a *App) userRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /users", a.usersList)
	mux.HandleFunc("GET /users/new", a.newUserPage)
	mux.HandleFunc("POST /users/new", a.newUserSubmit)
	mux.HandleFunc("GET /users/{id}/delete", a.confirmDeletePage)
	mux.HandleFunc("POST /users/{id}/delete", a.deleteUser)
	mux.HandleFunc("GET /users/{id}/promote", a.confirmPromotePage)
	mux.HandleFunc("POST /users/{id}/promote", a.promoteUser)
}

func (a *App) usersList(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	users, err := a.users.FindAll(r.Context())
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, http.StatusOK, views.UsersList(views.UsersListView{
		User:     user.Nav(),
		ViewerID: user.UserID,
		Users:    users,
	}))
}

func (a *App) newUserPage(w http.ResponseWriter, r *http.Request) {
	admin, ok := requireAdmin(w, r)
	if !ok {
		return
	}
	writeHTML(w, http.StatusOK, views.NewUser(admin.Nav(), ""))
}

func (a *App) newUserSubmit(w http.ResponseWriter, r *http.Request) {
	admin, ok := requireAdmin(w, r)
	if !ok {
		return
	}
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

	if message := validateCredentials(username, password); message != "" {
		writeHTML(w, http.StatusOK, views.NewUser(admin.Nav(), message))
		return
	}
	username = strings.TrimSpace(username)
	_, err = a.users.FindByUsername(r.Context(), username)
	if err == nil {
		writeHTML(w, http.StatusOK, views.NewUser(admin.Nav(), "Username already exists"))
		return
	}
	if !errors.Is(err, repo.ErrNotFound) {
		a.fail(w, r, err)
		return
	}

	hash, err := a.hasher.Hash(password)
	if err != nil {
		a.fail(w, r, internalError(err))
		return
	}
	_, err = a.users.Create(r.Context(), repo.NewUser{Username: username, PasswordHash: hash, Role: models.RoleUser})
	if err != nil {
		a.fail(w, r, err)
		return
	}
	redirect(w, "/users")
}

// sensitiveAction keeps each action's wording, form target and audit name in
// one place.
type sensitiveAction struct {
	label       string
	consequence func(username string) string
	formAction  func(userID string) string
	auditName   string
}

var promoteUserAction = sensitiveAction{
	label: "Promote to admin",
	consequence: func(username string) string {
		return username + " will gain full administrative access, including the ability to create and delete other users. They will be signed out of every device and must log in again."
	},
	formAction: func(userID string) string { return "/users/" + userID + "/promote" },
	auditName:  "promote_user",
}

var deleteUserAction = sensitiveAction{
	label: "Delete user",
	consequence: func(username string) string {
		return username + " and all of their workouts, exercises and sessions will be permanently deleted. This cannot be undone."
	},
	formAction: func(userID string) string { return "/users/" + userID + "/delete" },
	auditName:  "delete_user",
}

// confirmRequest is one rendering of the re-authentication page.
type confirmRequest struct {
	admin  ValidatedSession
	action sensitiveAction
	// errorMessage is empty on the first visit.
	errorMessage string
	status       int
}

// renderConfirmPage shows the re-authentication page. A nonexistent target is
// a 404. The status is 200 unless a failed password check says otherwise.
func (a *App) renderConfirmPage(w http.ResponseWriter, r *http.Request, request confirmRequest) {
	targetID := r.PathValue("id")
	target, err := a.users.FindByID(r.Context(), targetID)
	if errors.Is(err, repo.ErrNotFound) {
		a.fail(w, r, notFound("User not found"))
		return
	}
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, request.status, views.ConfirmAction(views.ConfirmActionView{
		User:        request.admin.Nav(),
		ActionLabel: request.action.label,
		Consequence: request.action.consequence(target.Username),
		FormAction:  request.action.formAction(targetID),
		Error:       request.errorMessage,
	}))
}

// reauthRequest is a sensitive action and the password that must confirm it.
type reauthRequest struct {
	admin    ValidatedSession
	action   sensitiveAction
	password string
}

// requireReauth turns "has the cookie" into "knows the password" before a
// sensitive action. It shares the per-user throttle with password change. When
// it returns false the confirmation page, with the error, is already written.
func (a *App) requireReauth(w http.ResponseWriter, r *http.Request, request reauthRequest) bool {
	outcome, err := a.checkPassword(r, request.admin, request.password, request.action.auditName)
	if err != nil {
		a.fail(w, r, err)
		return false
	}
	switch outcome {
	case reauthVerified:
		// Only a proven password hands the attempt back, so failures stay charged.
		a.throttles.SensitiveAction.Release(request.admin.UserID)
		return true
	case reauthThrottled:
		a.renderConfirmPage(w, r, confirmRequest{admin: request.admin, action: request.action, errorMessage: reauthThrottledMessage, status: http.StatusTooManyRequests})
	default:
		a.renderConfirmPage(w, r, confirmRequest{admin: request.admin, action: request.action, errorMessage: reauthWrongMessage, status: http.StatusOK})
	}
	return false
}

// readReauthPassword reads the admin's re-entered password.
func readReauthPassword(w http.ResponseWriter, r *http.Request) (string, error) {
	form, err := readForm(w, r)
	if err != nil {
		return "", err
	}
	return form.required("current_password")
}

func (a *App) confirmPromotePage(w http.ResponseWriter, r *http.Request) {
	admin, ok := requireAdmin(w, r)
	if !ok {
		return
	}
	a.renderConfirmPage(w, r, confirmRequest{admin: admin, action: promoteUserAction, status: http.StatusOK})
}

func (a *App) confirmDeletePage(w http.ResponseWriter, r *http.Request) {
	admin, ok := requireAdmin(w, r)
	if !ok {
		return
	}
	if admin.UserID == r.PathValue("id") {
		a.fail(w, r, badRequest("Cannot delete your own account"))
		return
	}
	a.renderConfirmPage(w, r, confirmRequest{admin: admin, action: deleteUserAction, status: http.StatusOK})
}

func (a *App) deleteUser(w http.ResponseWriter, r *http.Request) {
	admin, ok := requireAdmin(w, r)
	if !ok {
		return
	}
	password, err := readReauthPassword(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	userID := r.PathValue("id")
	if admin.UserID == userID {
		a.fail(w, r, badRequest("Cannot delete your own account"))
		return
	}
	if !a.requireReauth(w, r, reauthRequest{admin: admin, action: deleteUserAction, password: password}) {
		return
	}

	// Count before the delete: ON DELETE CASCADE removes the sessions with the
	// user row, leaving nothing to count afterwards.
	sessionsDestroyed, err := a.sessions.CountForUser(r.Context(), userID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	// User row first, so a failure leaves sessions intact.
	existed, err := a.users.Delete(r.Context(), userID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if existed {
		// Normally a no-op after the cascade; covers a connection with
		// foreign_keys off.
		if _, err := a.sessions.DeleteAllForUser(r.Context(), userID); err != nil {
			a.fail(w, r, err)
			return
		}
		a.audit.SessionsDestroyedBulk(a.requestInfo(r), audit.BulkSessionsDestroyed{
			ActorSessionFP: a.fingerprint(admin.SessionToken),
			UserID:         userID,
			Count:          sessionsDestroyed,
			Reason:         "admin_user_delete",
		})
	}
	redirect(w, "/users")
}

func (a *App) promoteUser(w http.ResponseWriter, r *http.Request) {
	admin, ok := requireAdmin(w, r)
	if !ok {
		return
	}
	password, err := readReauthPassword(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	userID := r.PathValue("id")
	if !a.requireReauth(w, r, reauthRequest{admin: admin, action: promoteUserAction, password: password}) {
		return
	}

	promoted, err := a.users.UpdateRole(r.Context(), userID, models.RoleAdmin)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	// Renew sessions on privilege change (OWASP): a token stolen before the
	// promotion must not inherit admin. Gated on promoted so a missing id logs
	// nothing.
	if promoted {
		destroyed, err := a.sessions.DeleteAllForUser(r.Context(), userID)
		if err != nil {
			a.fail(w, r, err)
			return
		}
		a.audit.SessionsDestroyedBulk(a.requestInfo(r), audit.BulkSessionsDestroyed{
			ActorSessionFP: a.fingerprint(admin.SessionToken),
			UserID:         userID,
			Count:          int(destroyed),
			Reason:         "role_change",
		})
	}
	redirect(w, "/users")
}
