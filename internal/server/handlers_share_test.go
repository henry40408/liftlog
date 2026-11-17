package server

import (
	"context"
	"net/http"
	"net/url"
	"strings"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/models"
)

type wkShareWorld struct {
	ta       *testApp
	owner    signedIn
	intruder signedIn
	workout  models.WorkoutSession
}

func newWkShareWorld(t *testing.T) wkShareWorld {
	t.Helper()
	ta := newTestApp(t, testAppOptions{})
	owner := ta.signInUser()
	note := "Shared notes"
	return wkShareWorld{
		ta:       ta,
		owner:    owner,
		intruder: ta.signIn("intruder", models.RoleUser),
		workout:  ta.wkWorkout(owner.user.ID, &note),
	}
}

func (w wkShareWorld) sharePath() string  { return "/workouts/" + w.workout.ID + "/share" }
func (w wkShareWorld) revokePath() string { return "/workouts/" + w.workout.ID + "/revoke-share" }

func (w wkShareWorld) reload(t *testing.T) models.WorkoutSession {
	t.Helper()
	workout, err := w.ta.app.workouts.FindOwnedSession(context.Background(), w.workout.ID, w.owner.user.ID)
	if err != nil {
		t.Fatal(err)
	}
	return workout
}

func (w wkShareWorld) share(t *testing.T, days string) models.WorkoutSession {
	t.Helper()
	form := url.Values{}
	if days != "-" {
		form.Set("expires_in_days", days)
	}
	wantRedirect(t, w.ta.postForm(w.sharePath(), form, w.owner.cookie), "/workouts/"+w.workout.ID)
	shared := w.reload(t)
	if shared.ShareToken == nil {
		t.Fatal("no share token stored")
	}
	return shared
}

func TestShareWithoutExpiryNeverExpires(t *testing.T) {
	for _, days := range []string{"-", "", "   "} {
		w := newWkShareWorld(t)

		shared := w.share(t, days)

		if shared.ShareExpiresAt != nil {
			t.Errorf("days %q: expires at %v", days, shared.ShareExpiresAt)
		}
		wantStatus(t, w.ta.get("/shared/"+*shared.ShareToken, ""), http.StatusOK)
	}
}

func TestShareWithExpiryStoresTheDeadline(t *testing.T) {
	w := newWkShareWorld(t)

	shared := w.share(t, " 7 ")

	if shared.ShareExpiresAt == nil {
		t.Fatal("no expiry stored")
	}
	if left := time.Until(*shared.ShareExpiresAt); left < 7*24*time.Hour-time.Minute || left > 7*24*time.Hour+time.Minute {
		t.Errorf("expires in %v", left)
	}
}

func TestShareRejectsOutOfRangeAndMalformedExpiry(t *testing.T) {
	w := newWkShareWorld(t)

	for _, days := range []string{"0", "-1", "366", "10000"} {
		response := w.ta.postForm(w.sharePath(), url.Values{"expires_in_days": {days}}, w.owner.cookie)
		wantStatus(t, response, http.StatusBadRequest)
		wantContains(t, response, "Share link expiry must be between 1 and 365 days")
	}
	for _, days := range []string{"abc", "1.5"} {
		wantStatus(t, w.ta.postForm(w.sharePath(), url.Values{"expires_in_days": {days}}, w.owner.cookie), http.StatusUnprocessableEntity)
	}
	wantStatus(t, w.ta.postForm(w.sharePath(), url.Values{"expires_in_days": {"1"}}, w.owner.cookie), http.StatusSeeOther)
	wantStatus(t, w.ta.postForm(w.sharePath(), url.Values{"expires_in_days": {"365"}}, w.owner.cookie), http.StatusSeeOther)
	if w.reload(t).ShareToken == nil {
		t.Error("the valid bounds were refused")
	}
}

func TestSharedPageIsPublicAndShowsTheWorkout(t *testing.T) {
	w := newWkShareWorld(t)
	squat := w.ta.wkExercise(w.owner.user.ID, "Squat", "legs")
	w.ta.wkLog(wkSet{workoutID: w.workout.ID, exerciseID: squat.ID, number: 1, reps: 5, weight: 100})
	shared := w.share(t, "-")

	response := w.ta.get("/shared/"+*shared.ShareToken, "")

	wantStatus(t, response, http.StatusOK)
	wantFragments(t, response.text(), "Shared notes", "Squat", "member")
	if response.Header().Get("X-Content-Type-Options") != "nosniff" {
		t.Error("the public page lacks the baseline headers")
	}
	if strings.Contains(response.text(), "/workouts/"+w.workout.ID+"/delete") {
		t.Error("the public page offers owner actions")
	}
}

func TestUnknownAndExpiredTokensAreTheSame404(t *testing.T) {
	w := newWkShareWorld(t)
	shared := w.share(t, "7")
	w.ta.mustExec("UPDATE workout_sessions SET share_expires_at = '2000-01-01T00:00:00Z' WHERE id = ?", w.workout.ID)

	for _, token := range []string{"nonexistent", *shared.ShareToken} {
		response := w.ta.get("/shared/"+token, "")
		wantStatus(t, response, http.StatusNotFound)
		wantContains(t, response, "Shared workout not found")
	}
}

func TestExpiredLinkIsNotShownAsSharedOnTheOwnersPage(t *testing.T) {
	w := newWkShareWorld(t)
	shared := w.share(t, "7")
	w.ta.mustExec("UPDATE workout_sessions SET share_expires_at = '2000-01-01T00:00:00Z' WHERE id = ?", w.workout.ID)

	body := w.ta.get("/workouts/"+w.workout.ID, w.owner.cookie).text()

	if strings.Contains(body, *shared.ShareToken) {
		t.Error("the dead link is still offered")
	}
}

func TestRevokeClearsTokenAndExpiryAndReshareMakesANewToken(t *testing.T) {
	w := newWkShareWorld(t)
	first := w.share(t, "30")

	wantRedirect(t, w.ta.postForm(w.revokePath(), url.Values{}, w.owner.cookie), "/workouts/"+w.workout.ID)

	revoked := w.reload(t)
	if revoked.ShareToken != nil || revoked.ShareExpiresAt != nil {
		t.Errorf("after revoke: %+v", revoked)
	}
	wantStatus(t, w.ta.get("/shared/"+*first.ShareToken, ""), http.StatusNotFound)
	second := w.share(t, "-")
	if *second.ShareToken == *first.ShareToken {
		t.Error("re-sharing reused the revoked token")
	}
	wantStatus(t, w.ta.get("/shared/"+*second.ShareToken, ""), http.StatusOK)
}

func TestRevokeConfirmationWarnsWithoutRevoking(t *testing.T) {
	w := newWkShareWorld(t)
	shared := w.share(t, "-")

	page := w.ta.get(w.revokePath(), w.owner.cookie)

	wantStatus(t, page, http.StatusOK)
	wantFragments(t, page.text(), "The existing share link will stop working", `<form method="post" action="`+w.revokePath()+`">`)
	if w.reload(t).ShareToken == nil {
		t.Fatal("viewing the confirmation page revoked the link")
	}
	wantStatus(t, w.ta.get("/shared/"+*shared.ShareToken, ""), http.StatusOK)
}

func TestShareRoutesRequireOwnership(t *testing.T) {
	w := newWkShareWorld(t)
	shared := w.share(t, "-")

	for _, request := range []response{
		w.ta.postForm(w.sharePath(), url.Values{}, w.intruder.cookie),
		w.ta.postForm(w.revokePath(), url.Values{}, w.intruder.cookie),
		w.ta.get(w.revokePath(), w.intruder.cookie),
	} {
		wantStatus(t, request, http.StatusNotFound)
		wantContains(t, request, "Workout not found")
	}
	if got := w.reload(t); got.ShareToken == nil || *got.ShareToken != *shared.ShareToken {
		t.Error("an intruder changed the share state")
	}
}

func TestOwnerPageShowsShareFormThenLinkWithBothConfirmationPaths(t *testing.T) {
	w := newWkShareWorld(t)
	before := w.ta.get("/workouts/"+w.workout.ID, w.owner.cookie).text()
	wantFragments(t, before, `action="`+w.sharePath()+`"`, `name="expires_in_days"`)
	if strings.Contains(before, "revoke-share") {
		t.Error("a revoke trigger without a link")
	}

	shared := w.share(t, "-")
	after := w.ta.get("/workouts/"+w.workout.ID, w.owner.cookie).text()

	wantFragments(t, after,
		"/shared/"+*shared.ShareToken,
		`href="`+w.revokePath()+`"`, `data-confirm="Revoke the share link?`,
		// The clipboard button needs scripts, so it ships hidden.
		"hidden", "data-requires-js", "data-copy-url")
}
