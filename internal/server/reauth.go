package server

import (
	"errors"
	"net/http"

	"github.com/henry40408/liftlog/internal/audit"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/repo"
)

type reauthOutcome int

const (
	reauthVerified reauthOutcome = iota
	reauthThrottled
	reauthWrongPassword
)

const (
	reauthThrottledMessage = "Too many attempts. Please try again later."
	reauthWrongMessage     = "Password is incorrect"
)

// checkPassword makes a signed-in user prove they know their password, behind
// the per-user throttle. It is keyed by user id so a stolen cookie gains no
// budget from new IPs, and the attempt is charged before verifying. The caller
// refunds it with throttles.SensitiveAction.Release as soon as the password
// verifies, before the action runs, so only failures stay charged.
//
// action is the audit name of what is being confirmed.
func (a *App) checkPassword(r *http.Request, user ValidatedSession, password, action string) (reauthOutcome, error) {
	reauth := audit.Reauth{
		ActorSessionFP: a.fingerprint(user.SessionToken),
		UserID:         user.UserID,
		Action:         action,
	}
	if !a.throttles.SensitiveAction.TryAcquire(user.UserID) {
		a.audit.ReauthThrottled(a.requestInfo(r), reauth)
		return reauthThrottled, nil
	}

	_, verified, err := a.verifyPassword(r, user.Username, password)
	if err != nil {
		return reauthWrongPassword, err
	}
	if !verified {
		a.audit.ReauthFailed(a.requestInfo(r), reauth)
		return reauthWrongPassword, nil
	}
	return reauthVerified, nil
}

// verifyPassword returns the user when the password is right. An unknown
// username is verified against a dummy hash, so timing does not reveal which
// usernames exist.
func (a *App) verifyPassword(r *http.Request, username, password string) (models.User, bool, error) {
	user, err := a.users.FindByUsername(r.Context(), username)
	if errors.Is(err, repo.ErrNotFound) {
		// The result and any error are discarded: failing here would turn an
		// unknown account into a 500 and reveal that it does not exist.
		_, _ = a.hasher.Verify(password, a.hasher.DummyHash())
		return models.User{}, false, nil
	}
	if err != nil {
		return models.User{}, false, err
	}
	ok, err := a.hasher.Verify(password, user.PasswordHash)
	if err != nil {
		return models.User{}, false, internalError(err)
	}
	return user, ok, nil
}
