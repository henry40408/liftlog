package server

import (
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/henry40408/liftlog/internal/views"
)

const (
	minShareDays = 1
	maxShareDays = 365
)

// parseShareLifetime reads the optional link lifetime. An absent or blank
// field means the link never expires, which is a nil result.
func parseShareLifetime(form formValues) (*time.Duration, error) {
	raw := strings.TrimSpace(form.optional("expires_in_days"))
	if raw == "" {
		return nil, nil
	}
	days, err := strconv.ParseInt(raw, 10, 64)
	if err != nil {
		return nil, deserializeError("expires_in_days")
	}
	if days < minShareDays || days > maxShareDays {
		return nil, badRequest("Share link expiry must be between 1 and 365 days")
	}
	lifetime := time.Duration(days) * 24 * time.Hour
	return &lifetime, nil
}

func (a *App) shareWorkout(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	form, err := readForm(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	lifetime, err := parseShareLifetime(form)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	workout, err := a.ownedWorkout(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if _, err := a.workouts.SetShareToken(r.Context(), workout.ID, user.UserID, lifetime); err != nil {
		a.fail(w, r, orNotFound(err, workoutNotFound))
		return
	}
	redirect(w, "/workouts/"+workout.ID)
}

func (a *App) confirmRevokeShare(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	workout, err := a.ownedWorkout(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeConfirmPage(w, user, confirmPage{
		ActionLabel: "Revoke share link",
		Consequence: "The existing share link will stop working for anyone who already has it. " +
			"Sharing this workout again will produce a different link.",
		FormAction: "/workouts/" + workout.ID + "/revoke-share",
		CancelURL:  "/workouts/" + workout.ID,
	})
}

func (a *App) revokeShare(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	workout, err := a.ownedWorkout(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if _, err := a.workouts.RevokeShareToken(r.Context(), workout.ID, user.UserID); err != nil {
		a.fail(w, r, err)
		return
	}
	redirect(w, "/workouts/"+workout.ID)
}

// viewShared is public: the token is the credential. Unknown and expired
// tokens are the same 404.
func (a *App) viewShared(w http.ResponseWriter, r *http.Request) {
	workout, err := a.workouts.FindSessionByShareToken(r.Context(), r.PathValue("token"))
	if err != nil {
		a.fail(w, r, orNotFound(err, "Shared workout not found"))
		return
	}
	logs, err := a.workouts.FindLogsBySessionForShare(r.Context(), workout.ID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	owner, err := a.users.FindByID(r.Context(), workout.UserID)
	if err != nil {
		a.fail(w, r, orNotFound(err, "User not found"))
		return
	}
	writeHTML(w, http.StatusOK, views.SharedWorkout(workout, logs, owner.Username))
}
