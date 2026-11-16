package server

import (
	"context"
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/henry40408/liftlog/internal/models"
)

func (ta *testApp) wkExerciseCount(userID string) int {
	return ta.count("SELECT COUNT(*) FROM exercises WHERE user_id = ?", userID)
}

func TestExercisePagesRequireSignIn(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	owner := ta.signInUser()
	exercise := ta.wkExercise(owner.user.ID, "Squat", "legs")
	id := exercise.ID

	for _, path := range []string{"/exercises", "/exercises/new", "/exercises/" + id + "/edit", "/exercises/" + id + "/delete"} {
		wantRedirect(t, ta.get(path, ""), "/auth/login")
	}
	for _, path := range []string{"/exercises", "/exercises/" + id, "/exercises/" + id + "/delete"} {
		wantRedirect(t, ta.postForm(path, url.Values{"name": {"x"}, "category": {"core"}}, ""), "/auth/login")
	}
	if ta.wkExerciseCount(owner.user.ID) != 1 {
		t.Error("an anonymous request changed the exercises")
	}
}

func TestCreateExercise(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()

	wantStatus(t, ta.get("/exercises/new", user.cookie), http.StatusOK)
	wantRedirect(t, ta.postForm("/exercises", url.Values{"name": {"Bench Press"}, "category": {"chest"}}, user.cookie), "/exercises")

	list := ta.get("/exercises", user.cookie)
	wantStatus(t, list, http.StatusOK)
	wantContains(t, list, "Bench Press")
}

func TestCreateExerciseWithBlankNameRerendersWithTheError(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()

	for _, name := range []string{"", "   "} {
		response := ta.postForm("/exercises", url.Values{"name": {name}, "category": {"chest"}}, user.cookie)
		wantStatus(t, response, http.StatusOK)
		wantContains(t, response, "Exercise name is required")
	}
	wantStatus(t, ta.postForm("/exercises", url.Values{"name": {"x"}}, user.cookie), http.StatusUnprocessableEntity)
	if ta.wkExerciseCount(user.user.ID) != 0 {
		t.Error("an invalid exercise was stored")
	}
}

func TestExercisesListShowsOnlyOwn(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	other := ta.signIn("other", models.RoleUser)
	ta.wkExercise(user.user.ID, "Squat", "legs")
	ta.wkExercise(other.user.ID, "Secret Curl", "arms")

	body := ta.get("/exercises", user.cookie).text()

	wantFragments(t, body, "Squat")
	if strings.Contains(body, "Secret Curl") {
		t.Error("another user's exercise is listed")
	}
}

func TestEditAndUpdateExercise(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.wkExercise(user.user.ID, "Squat", "legs")
	path := "/exercises/" + exercise.ID

	edit := ta.get(path+"/edit", user.cookie)
	wantStatus(t, edit, http.StatusOK)
	wantContains(t, edit, "Squat")

	wantRedirect(t, ta.postForm(path, url.Values{"name": {"Front Squat"}, "category": {"legs"}}, user.cookie), "/exercises")
	updated, err := ta.app.exercises.FindOwned(context.Background(), exercise.ID, user.user.ID)
	if err != nil || updated.Name != "Front Squat" {
		t.Errorf("exercise = %+v, err = %v", updated, err)
	}

	blank := ta.postForm(path, url.Values{"name": {" "}, "category": {"legs"}}, user.cookie)
	wantStatus(t, blank, http.StatusOK)
	wantContains(t, blank, "Exercise name is required")
}

func TestExerciseRoutesAreNotFoundForForeignAndMissingIDs(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	owner := ta.signInUser()
	intruder := ta.signIn("intruder", models.RoleUser)
	exercise := ta.wkExercise(owner.user.ID, "Squat", "legs")
	form := url.Values{"name": {"Hacked"}, "category": {"core"}}

	for _, id := range []string{exercise.ID, "missing"} {
		path := "/exercises/" + id
		for _, request := range []response{
			ta.get(path+"/edit", intruder.cookie),
			ta.get(path+"/delete", intruder.cookie),
			ta.postForm(path, form, intruder.cookie),
			ta.postForm(path+"/delete", url.Values{}, intruder.cookie),
		} {
			wantStatus(t, request, http.StatusNotFound)
			wantContains(t, request, "Exercise not found")
		}
	}
	got, err := ta.app.exercises.FindOwned(context.Background(), exercise.ID, owner.user.ID)
	if err != nil || got.Name != "Squat" {
		t.Errorf("the exercise changed: %+v, err = %v", got, err)
	}
}

func TestDeleteExerciseConfirmationNamesItAndActsOnlyOnPost(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.wkExercise(user.user.ID, "Squat", "legs")
	path := "/exercises/" + exercise.ID + "/delete"

	page := ta.get(path, user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantFragments(t, page.text(), "Squat will be permanently deleted.", `<form method="post" action="`+path+`">`, `href="/exercises"`)
	if ta.wkExerciseCount(user.user.ID) != 1 {
		t.Fatal("viewing the confirmation page deleted the exercise")
	}
	wantRedirect(t, ta.postForm(path, url.Values{}, user.cookie), "/exercises")
	if ta.wkExerciseCount(user.user.ID) != 0 {
		t.Error("the POST did not delete the exercise")
	}
}

func TestDeleteExerciseInUseIsRefusedWithTheRepositoryMessage(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.wkExercise(user.user.ID, "Squat", "legs")
	workout := ta.wkWorkout(user.user.ID, nil)
	ta.wkLog(wkSet{workoutID: workout.ID, exerciseID: exercise.ID, number: 1, reps: 5, weight: 100})

	response := ta.postForm("/exercises/"+exercise.ID+"/delete", url.Values{}, user.cookie)

	wantStatus(t, response, http.StatusBadRequest)
	if response.text() != "This exercise is used by existing workout logs and cannot be deleted" {
		t.Errorf("body = %q", response.text())
	}
	if ta.wkExerciseCount(user.user.ID) != 1 {
		t.Error("an exercise in use was deleted")
	}
}

func TestExercisesListTriggersCarryBothConfirmationPaths(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.wkExercise(user.user.ID, "Squat", "legs")

	body := ta.get("/exercises", user.cookie).text()

	wantFragments(t, body, `href="/exercises/`+exercise.ID+`/delete"`, `data-confirm="Delete Squat?"`)
}
