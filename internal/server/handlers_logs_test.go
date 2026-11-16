package server

import (
	"context"
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/henry40408/liftlog/internal/models"
)

type wkLogWorld struct {
	ta       *testApp
	owner    signedIn
	intruder signedIn
	exercise models.Exercise
	workout  models.WorkoutSession
}

func newWkLogWorld(t *testing.T) wkLogWorld {
	t.Helper()
	ta := newTestApp(t, testAppOptions{})
	owner := ta.signInUser()
	return wkLogWorld{
		ta:       ta,
		owner:    owner,
		intruder: ta.signIn("intruder", models.RoleUser),
		exercise: ta.wkExercise(owner.user.ID, "Squat", "legs"),
		workout:  ta.wkWorkout(owner.user.ID, nil),
	}
}

func (w wkLogWorld) addPath() string { return "/workouts/" + w.workout.ID + "/logs" }

func (w wkLogWorld) log(number int, weight float64) models.WorkoutLog {
	return w.ta.wkLog(wkSet{workoutID: w.workout.ID, exerciseID: w.exercise.ID, number: number, reps: 5, weight: weight})
}

func (w wkLogWorld) stored(t *testing.T, id string) models.WorkoutLog {
	t.Helper()
	log, err := w.ta.app.workouts.FindLogByID(context.Background(), id)
	if err != nil {
		t.Fatal(err)
	}
	return log
}

func TestAddLogNumbersSetsPerExercise(t *testing.T) {
	w := newWkLogWorld(t)
	form := url.Values{"exercise_id": {w.exercise.ID}, "reps": {"5"}, "weight": {"82.5"}, "rpe": {"8"}}

	for range 2 {
		wantRedirect(t, w.ta.postForm(w.addPath(), form, w.owner.cookie), "/workouts/"+w.workout.ID)
	}

	if got := w.ta.count("SELECT COUNT(*) FROM workout_logs WHERE session_id = ? AND set_number = 2 AND weight = 82.5 AND rpe = 8", w.workout.ID); got != 1 {
		t.Errorf("second set rows = %d", got)
	}
}

func TestAddLogWithoutRPEStoresNone(t *testing.T) {
	w := newWkLogWorld(t)
	form := url.Values{"exercise_id": {w.exercise.ID}, "reps": {"5"}, "weight": {"100"}, "rpe": {""}}

	wantStatus(t, w.ta.postForm(w.addPath(), form, w.owner.cookie), http.StatusSeeOther)

	if w.ta.count("SELECT COUNT(*) FROM workout_logs WHERE rpe IS NULL") != 1 {
		t.Error("blank rpe was not stored as NULL")
	}
}

func TestAddLogRejectsBadNumbers(t *testing.T) {
	w := newWkLogWorld(t)
	good := func() url.Values {
		return url.Values{"exercise_id": {w.exercise.ID}, "reps": {"5"}, "weight": {"100"}}
	}
	for name, mutate := range map[string]func(url.Values){
		"reps text":      func(f url.Values) { f.Set("reps", "five") },
		"reps fraction":  func(f url.Values) { f.Set("reps", "5.5") },
		"weight text":    func(f url.Values) { f.Set("weight", "heavy") },
		"weight NaN":     func(f url.Values) { f.Set("weight", "NaN") },
		"weight inf":     func(f url.Values) { f.Set("weight", "inf") },
		"rpe text":       func(f url.Values) { f.Set("rpe", "hard") },
		"reps missing":   func(f url.Values) { f.Del("reps") },
		"weight missing": func(f url.Values) { f.Del("weight") },
	} {
		form := good()
		mutate(form)
		wantStatus(t, w.ta.postForm(w.addPath(), form, w.owner.cookie), http.StatusUnprocessableEntity)
		if w.ta.wkLogCount(w.workout.ID) != 0 {
			t.Fatalf("%s: a set was stored", name)
		}
	}
}

func TestAddLogChecksBothOwners(t *testing.T) {
	w := newWkLogWorld(t)
	foreignExercise := w.ta.wkExercise(w.intruder.user.ID, "Deadlift", "back")
	form := url.Values{"exercise_id": {w.exercise.ID}, "reps": {"5"}, "weight": {"100"}}

	wrongWorkout := w.ta.postForm(w.addPath(), form, w.intruder.cookie)
	wantStatus(t, wrongWorkout, http.StatusNotFound)
	wantContains(t, wrongWorkout, "Workout not found")

	form.Set("exercise_id", foreignExercise.ID)
	wrongExercise := w.ta.postForm(w.addPath(), form, w.owner.cookie)
	wantStatus(t, wrongExercise, http.StatusNotFound)
	wantContains(t, wrongExercise, "Exercise not found")
	if w.ta.wkLogCount(w.workout.ID) != 0 {
		t.Error("a set was stored")
	}
}

func TestDeleteLogConfirmationNamesTheSetAndActsOnlyOnPost(t *testing.T) {
	w := newWkLogWorld(t)
	log := w.log(1, 82.5)
	path := "/workouts/" + w.workout.ID + "/logs/" + log.ID + "/delete"

	page := w.ta.get(path, w.owner.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, "Set 1 of Squat — 82.5 kg × 5 — will be permanently deleted.")
	wantContains(t, page, `<form method="post" action="`+path+`">`)
	if w.ta.wkLogCount(w.workout.ID) != 1 {
		t.Fatal("viewing the confirmation page deleted the set")
	}
	wantRedirect(t, w.ta.postForm(path, url.Values{}, w.owner.cookie), "/workouts/"+w.workout.ID)
	if w.ta.wkLogCount(w.workout.ID) != 0 {
		t.Error("the POST did not delete the set")
	}
}

func TestDeleteLogRequiresOwnershipOfTheWorkoutAndTheLog(t *testing.T) {
	w := newWkLogWorld(t)
	log := w.log(1, 100)
	sibling := w.ta.wkWorkout(w.owner.user.ID, nil)
	siblingPath := "/workouts/" + sibling.ID + "/logs/" + log.ID + "/delete"
	path := "/workouts/" + w.workout.ID + "/logs/" + log.ID + "/delete"

	wantStatus(t, w.ta.get(path, w.intruder.cookie), http.StatusNotFound)
	wantStatus(t, w.ta.postForm(path, url.Values{}, w.intruder.cookie), http.StatusNotFound)
	// A log of another session of the same owner is not reachable either.
	missing := w.ta.get(siblingPath, w.owner.cookie)
	wantStatus(t, missing, http.StatusNotFound)
	wantContains(t, missing, "Set not found")
	wantRedirect(t, w.ta.postForm(siblingPath, url.Values{}, w.owner.cookie), "/workouts/"+sibling.ID)
	if w.ta.wkLogCount(w.workout.ID) != 1 {
		t.Error("a set was deleted through the wrong route")
	}
}

func TestEditLogPageAndUpdate(t *testing.T) {
	w := newWkLogWorld(t)
	log := w.log(1, 100)
	path := "/workouts/" + w.workout.ID + "/logs/" + log.ID

	page := w.ta.get(path+"/edit", w.owner.cookie)
	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, "Squat")

	form := url.Values{"reps": {"6"}, "weight": {"102.5"}, "rpe": {"9"}}
	wantRedirect(t, w.ta.postForm(path, form, w.owner.cookie), "/workouts/"+w.workout.ID)
	updated := w.stored(t, log.ID)
	if updated.Reps != 6 || updated.Weight != 102.5 || updated.RPE == nil || *updated.RPE != 9 {
		t.Errorf("log = %+v", updated)
	}
	if updated.ExerciseID != w.exercise.ID || updated.SetNumber != 1 {
		t.Errorf("the exercise or set number changed: %+v", updated)
	}
}

func TestEditLogRequiresOwnership(t *testing.T) {
	w := newWkLogWorld(t)
	log := w.log(1, 100)
	sibling := w.ta.wkWorkout(w.owner.user.ID, nil)
	path := "/workouts/" + w.workout.ID + "/logs/" + log.ID
	form := url.Values{"reps": {"1"}, "weight": {"1"}}

	wantStatus(t, w.ta.get(path+"/edit", w.intruder.cookie), http.StatusNotFound)
	wantStatus(t, w.ta.postForm(path, form, w.intruder.cookie), http.StatusNotFound)
	wrongSession := w.ta.get("/workouts/"+sibling.ID+"/logs/"+log.ID+"/edit", w.owner.cookie)
	wantStatus(t, wrongSession, http.StatusNotFound)
	wantContains(t, wrongSession, "Log not found")
	wantStatus(t, w.ta.get("/workouts/"+w.workout.ID+"/logs/missing/edit", w.owner.cookie), http.StatusNotFound)

	// The update is scoped to the session, so the log stays as it was.
	w.ta.postForm("/workouts/"+sibling.ID+"/logs/"+log.ID, form, w.owner.cookie)
	if got := w.stored(t, log.ID); got.Reps != 5 || got.Weight != 100 {
		t.Errorf("log = %+v", got)
	}
}

func TestUpdateLogAcceptsFractionalWeightAndRejectsGarbage(t *testing.T) {
	w := newWkLogWorld(t)
	log := w.log(1, 100)
	path := "/workouts/" + w.workout.ID + "/logs/" + log.ID

	wantStatus(t, w.ta.postForm(path, url.Values{"reps": {"5"}, "weight": {"2.5"}}, w.owner.cookie), http.StatusSeeOther)
	if w.stored(t, log.ID).Weight != 2.5 {
		t.Error("fractional weight not stored")
	}
	wantStatus(t, w.ta.postForm(path, url.Values{"reps": {"5"}, "weight": {"NaN"}}, w.owner.cookie), http.StatusUnprocessableEntity)
}

func TestSetRowTriggersCarryHrefAndDataAttributes(t *testing.T) {
	w := newWkLogWorld(t)
	log := w.log(1, 82.5)
	logPath := "/workouts/" + w.workout.ID + "/logs/" + log.ID

	body := w.ta.get("/workouts/"+w.workout.ID, w.owner.cookie).text()

	wantFragments(t, body,
		`href="`+logPath+`/delete"`, `data-confirm="Delete set 1 of Squat?"`,
		`href="?prefill=`+log.ID+`#add-set"`, `data-clone-exercise="`+w.exercise.ID+`"`,
		`data-clone-weight="82.5"`, `href="`+logPath+`/edit"`)
	if strings.Contains(body, "onsubmit") {
		t.Error("an inline onsubmit guard is back")
	}
}
