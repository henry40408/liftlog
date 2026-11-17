package server

import (
	"context"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/repo"
)

var wkDay = time.Date(2024, 1, 15, 0, 0, 0, 0, time.UTC)

func (ta *testApp) wkExercise(userID, name, category string) models.Exercise {
	ta.t.Helper()
	exercise, err := ta.app.exercises.Create(context.Background(), repo.NewExercise{Name: name, Category: category, UserID: userID})
	if err != nil {
		ta.t.Fatal(err)
	}
	return exercise
}

func (ta *testApp) wkWorkout(userID string, notes *string) models.WorkoutSession {
	ta.t.Helper()
	workout, err := ta.app.workouts.CreateSession(context.Background(), userID, wkDay, notes)
	if err != nil {
		ta.t.Fatal(err)
	}
	return workout
}

type wkSet struct {
	workoutID  string
	exerciseID string
	number     int
	reps       int
	weight     float64
	rpe        *int
}

func (ta *testApp) wkLog(set wkSet) models.WorkoutLog {
	ta.t.Helper()
	log, err := ta.app.workouts.CreateLog(context.Background(), repo.NewLog{
		SessionID: set.workoutID, ExerciseID: set.exerciseID, SetNumber: set.number,
		Reps: set.reps, Weight: set.weight, RPE: set.rpe,
	})
	if err != nil {
		ta.t.Fatal(err)
	}
	return log
}

func (ta *testApp) wkLogCount(workoutID string) int {
	return ta.count("SELECT COUNT(*) FROM workout_logs WHERE session_id = ?", workoutID)
}

func wkInputTag(t *testing.T, body, id string) string {
	t.Helper()
	match := regexp.MustCompile(`<input[^>]*\bid="` + id + `"[^>]*>`).FindString(body)
	if match == "" {
		t.Fatalf("no <input id=%q> in page", id)
	}
	return match
}

func wkIntPtr(n int) *int { return &n }

func TestWorkoutPagesRequireSignIn(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	owner := ta.signInUser()
	workout := ta.wkWorkout(owner.user.ID, nil)
	id := workout.ID

	for _, path := range []string{
		"/workouts", "/workouts/new", "/workouts/" + id, "/workouts/" + id + "/edit",
		"/workouts/" + id + "/delete", "/workouts/" + id + "/logs/x/delete",
		"/workouts/" + id + "/logs/x/edit", "/workouts/" + id + "/revoke-share",
	} {
		wantRedirect(t, ta.get(path, ""), "/auth/login")
	}
	for _, path := range []string{
		"/workouts", "/workouts/" + id, "/workouts/" + id + "/delete", "/workouts/" + id + "/logs",
		"/workouts/" + id + "/logs/x", "/workouts/" + id + "/logs/x/delete",
		"/workouts/" + id + "/share", "/workouts/" + id + "/revoke-share",
	} {
		wantRedirect(t, ta.postForm(path, url.Values{}, ""), "/auth/login")
	}
	if ta.count("SELECT COUNT(*) FROM workout_sessions") != 1 {
		t.Error("an anonymous request changed the workouts")
	}
}

func TestCreateWorkoutRedirectsToItsPage(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()

	response := ta.postForm("/workouts", url.Values{"date": {"2024-01-15"}, "notes": {"Leg day"}}, user.cookie)

	wantStatus(t, response, http.StatusSeeOther)
	workouts, err := ta.app.workouts.FindSessionsByUserPaginated(context.Background(), user.user.ID, 10, 0)
	if err != nil || len(workouts) != 1 {
		t.Fatalf("workouts = %v, err = %v", workouts, err)
	}
	wantRedirect(t, response, "/workouts/"+workouts[0].ID)
	if workouts[0].Notes == nil || *workouts[0].Notes != "Leg day" {
		t.Errorf("notes = %v", workouts[0].Notes)
	}
}

func TestCreateWorkoutRejectsBadForms(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()

	missing := ta.postForm("/workouts", url.Values{}, user.cookie)
	wantStatus(t, missing, http.StatusUnprocessableEntity)
	wantContains(t, missing, "missing field `date`")
	badDate := ta.postForm("/workouts", url.Values{"date": {"15/01/2024"}}, user.cookie)
	wantStatus(t, badDate, http.StatusUnprocessableEntity)
	wantContains(t, badDate, "date: invalid date")
}

func TestNewWorkoutDefaultsToTheUsersToday(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()

	unset := ta.get("/workouts/new", user.cookie)
	wantStatus(t, unset, http.StatusOK)
	if !strings.Contains(unset.text(), "formatLocalDate") {
		t.Error("a user without a timezone gets no browser-today script")
	}

	zone := "Pacific/Kiritimati"
	if _, err := ta.app.users.SetTimezone(context.Background(), user.user.ID, zone); err != nil {
		t.Fatal(err)
	}
	set := ta.get("/workouts/new", user.cookie)
	wantStatus(t, set, http.StatusOK)
	today := ta.app.now().In(time.FixedZone("UTC+14", 14*3600)).Format("2006-01-02")
	if !strings.Contains(set.text(), `value="`+today+`"`) {
		t.Errorf("the date field is not prefilled with %s in the user's zone", today)
	}
	if strings.Contains(set.text(), "formatLocalDate") {
		t.Error("a user with a timezone still gets the browser-today script")
	}
}

func TestWorkoutsListShowsOnlyOwnWorkoutsAndPaginates(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	other := ta.createUser("other", strongPassword, models.RoleUser)
	mine := "my-own-note"
	theirs := "their-private-note"
	for range 12 {
		ta.wkWorkout(user.user.ID, &mine)
	}
	ta.wkWorkout(other.ID, &theirs)

	first := ta.get("/workouts", user.cookie)
	wantStatus(t, first, http.StatusOK)
	wantContains(t, first, mine)
	if strings.Contains(first.text(), theirs) {
		t.Error("another user's workout is listed")
	}
	if got := strings.Count(first.text(), "/workouts/"); got < workoutsPerPage {
		t.Errorf("page 1 links %d workouts", got)
	}
	wantStatus(t, ta.get("/workouts?page=2", user.cookie), http.StatusOK)
	wantStatus(t, ta.get("/workouts?page=0", user.cookie), http.StatusOK)
	wantStatus(t, ta.get("/workouts?page=-4", user.cookie), http.StatusOK)
	wantStatus(t, ta.get("/workouts?page=99", user.cookie), http.StatusOK)
	wantStatus(t, ta.get("/workouts?page=abc", user.cookie), http.StatusBadRequest)
	// Past int64 the number does not parse; at the edge it is clamped, so the
	// offset cannot overflow.
	wantStatus(t, ta.get("/workouts?page=9223372036854775807", user.cookie), http.StatusOK)
	wantStatus(t, ta.get("/workouts?page=9223372036854775808", user.cookie), http.StatusBadRequest)
	wantContains(t, ta.get("/workouts?page=abc", user.cookie), "page: invalid number")
}

func TestShowWorkoutOwnership(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	owner := ta.signInUser()
	intruder := ta.signIn("intruder", models.RoleUser)
	workout := ta.wkWorkout(owner.user.ID, nil)

	wantStatus(t, ta.get("/workouts/"+workout.ID, owner.cookie), http.StatusOK)
	for _, path := range []string{"/workouts/" + workout.ID, "/workouts/" + workout.ID + "/edit", "/workouts/missing"} {
		response := ta.get(path, intruder.cookie)
		wantStatus(t, response, http.StatusNotFound)
		wantContains(t, response, "Workout not found")
	}
}

func TestUpdateWorkout(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	owner := ta.signInUser()
	intruder := ta.signIn("intruder", models.RoleUser)
	workout := ta.wkWorkout(owner.user.ID, nil)
	path := "/workouts/" + workout.ID
	form := url.Values{"date": {"2024-02-20"}, "notes": {"Updated"}}

	wantStatus(t, ta.get(path+"/edit", owner.cookie), http.StatusOK)
	wantRedirect(t, ta.postForm(path, form, owner.cookie), path)
	updated, err := ta.app.workouts.FindOwnedSession(context.Background(), workout.ID, owner.user.ID)
	if err != nil {
		t.Fatal(err)
	}
	if updated.Date.Format("2006-01-02") != "2024-02-20" || updated.Notes == nil || *updated.Notes != "Updated" {
		t.Errorf("workout = %+v", updated)
	}

	// The update is scoped to the owner, so an intruder changes nothing.
	ta.postForm(path, url.Values{"date": {"2030-01-01"}, "notes": {"Hacked"}}, intruder.cookie)
	again, _ := ta.app.workouts.FindOwnedSession(context.Background(), workout.ID, owner.user.ID)
	if again.Notes == nil || *again.Notes != "Updated" {
		t.Errorf("an intruder changed the workout: %+v", again)
	}
}

func TestDeleteWorkoutConfirmationNamesTheCascadeAndActsOnlyOnPost(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	squat := ta.wkExercise(user.user.ID, "Squat", "legs")
	workout := ta.wkWorkout(user.user.ID, nil)
	path := "/workouts/" + workout.ID + "/delete"

	empty := ta.get(path, user.cookie)
	wantStatus(t, empty, http.StatusOK)
	wantContains(t, empty, "It has no sets recorded.")

	ta.wkLog(wkSet{workoutID: workout.ID, exerciseID: squat.ID, number: 1, reps: 5, weight: 100})
	one := ta.get(path, user.cookie)
	wantContains(t, one, "Its 1 recorded set will be deleted with it.")

	ta.wkLog(wkSet{workoutID: workout.ID, exerciseID: squat.ID, number: 2, reps: 5, weight: 105})
	two := ta.get(path, user.cookie)
	wantContains(t, two, "The workout on 2024-01-15 will be permanently deleted.")
	wantContains(t, two, "All 2 of its recorded sets will be deleted with it.")
	wantContains(t, two, `<form method="post" action="`+path+`">`)

	if ta.count("SELECT COUNT(*) FROM workout_sessions") != 1 || ta.wkLogCount(workout.ID) != 2 {
		t.Fatal("viewing the confirmation page deleted something")
	}
	wantRedirect(t, ta.postForm(path, url.Values{}, user.cookie), "/workouts")
	if ta.count("SELECT COUNT(*) FROM workout_sessions") != 0 || ta.wkLogCount(workout.ID) != 0 {
		t.Error("the POST did not delete the workout and its sets")
	}
}

func TestDeleteWorkoutIsScopedToTheOwner(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	owner := ta.signInUser()
	intruder := ta.signIn("intruder", models.RoleUser)
	workout := ta.wkWorkout(owner.user.ID, nil)
	path := "/workouts/" + workout.ID + "/delete"

	wantStatus(t, ta.get(path, intruder.cookie), http.StatusNotFound)
	wantRedirect(t, ta.postForm(path, url.Values{}, intruder.cookie), "/workouts")
	if ta.count("SELECT COUNT(*) FROM workout_sessions") != 1 {
		t.Error("an intruder deleted the workout")
	}
}

func TestWorkoutPageDeleteTriggerCarriesBothConfirmationPaths(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	workout := ta.wkWorkout(user.user.ID, nil)

	body := ta.get("/workouts/"+workout.ID, user.cookie).text()

	wantFragments(t, body, `href="/workouts/`+workout.ID+`/delete"`, `data-confirm="Delete this workout?`)
	if strings.Contains(body, "onsubmit") {
		t.Error("an inline onsubmit guard is back")
	}
}

func wantFragments(t *testing.T, body string, fragments ...string) {
	t.Helper()
	for _, fragment := range fragments {
		if !strings.Contains(body, fragment) {
			t.Errorf("page lacks %q", fragment)
		}
	}
}

func TestShowWorkoutPrefillsOnlyFromItsOwnLogs(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	intruder := ta.signIn("intruder", models.RoleUser)
	bench := ta.wkExercise(user.user.ID, "Bench Press", "chest")
	workout := ta.wkWorkout(user.user.ID, nil)
	log := ta.wkLog(wkSet{workoutID: workout.ID, exerciseID: bench.ID, number: 1, reps: 8, weight: 82.5, rpe: wkIntPtr(7)})
	path := "/workouts/" + workout.ID

	filled := ta.get(path+"?prefill="+log.ID, user.cookie)
	wantStatus(t, filled, http.StatusOK)
	wantContains(t, filled, `<option value="`+bench.ID+`" selected>`)
	for field, value := range map[string]string{"weight": "82.5", "reps": "8", "rpe": "7"} {
		if tag := wkInputTag(t, filled.text(), field); !strings.Contains(tag, `value="`+value+`"`) {
			t.Errorf("%s not prefilled: %s", field, tag)
		}
	}

	if strings.Contains(ta.get(path, user.cookie).text(), `<option value="`+bench.ID+`" selected>`) {
		t.Error("a plain visit preselects an exercise")
	}

	// A log id from another user's workout fills nothing and leaks nothing.
	foreignWorkout := ta.wkWorkout(intruder.user.ID, nil)
	foreignExercise := ta.wkExercise(intruder.user.ID, "Squat", "legs")
	foreignLog := ta.wkLog(wkSet{workoutID: foreignWorkout.ID, exerciseID: foreignExercise.ID, number: 1, reps: 3, weight: 321.5})
	leak := ta.get(path+"?prefill="+foreignLog.ID, user.cookie)
	wantStatus(t, leak, http.StatusOK)
	if strings.Contains(leak.text(), "321.5") || strings.Contains(leak.text(), foreignExercise.ID) {
		t.Error("another user's log leaked through ?prefill")
	}
	// Also not from another workout of the same user.
	other := ta.wkWorkout(user.user.ID, nil)
	siblingLog := ta.wkLog(wkSet{workoutID: other.ID, exerciseID: bench.ID, number: 1, reps: 4, weight: 77.25})
	sibling := ta.get(path+"?prefill="+siblingLog.ID, user.cookie).text()
	if strings.Contains(wkInputTag(t, sibling, "weight"), "77.25") {
		t.Error("a log of another workout was prefilled")
	}
}

func TestShowWorkoutSuggestionsAreAscendingDistinctAndOwnedOnly(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	other := ta.signIn("other", models.RoleUser)
	bench := ta.wkExercise(user.user.ID, "Bench Press", "chest")
	workout := ta.wkWorkout(user.user.ID, nil)
	for i, weight := range []float64{100, 82.5, 100, 0} {
		ta.wkLog(wkSet{workoutID: workout.ID, exerciseID: bench.ID, number: i + 1, reps: 5, weight: weight})
	}
	foreignExercise := ta.wkExercise(other.user.ID, "Squat", "legs")
	foreignWorkout := ta.wkWorkout(other.user.ID, nil)
	ta.wkLog(wkSet{workoutID: foreignWorkout.ID, exerciseID: foreignExercise.ID, number: 1, reps: 5, weight: 999.5})

	body := ta.get("/workouts/"+workout.ID, user.cookie).text()

	list := regexp.MustCompile(`(?s)<datalist id="weight-list">(.*?)</datalist>`).FindStringSubmatch(body)
	if list == nil {
		t.Fatalf("no weight datalist in page")
	}
	values := regexp.MustCompile(`value="([^"]*)"`).FindAllStringSubmatch(list[1], -1)
	var got []string
	for _, v := range values {
		got = append(got, v[1])
	}
	if strings.Join(got, ",") != "82.5,100" {
		t.Errorf("weight suggestions = %v", got)
	}
	if strings.Contains(body, "999.5") {
		t.Error("another user's weight is suggested")
	}
}

func TestShowWorkoutWithoutHistoryHasNoWeightDatalist(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	workout := ta.wkWorkout(user.user.ID, nil)

	if strings.Contains(ta.get("/workouts/"+workout.ID, user.cookie).text(), `id="weight-list"`) {
		t.Error("an empty history renders an empty datalist")
	}
}

func TestWeightSuggestionsHelper(t *testing.T) {
	logged := func(weight float64) models.WorkoutLogWithExercise {
		return models.WorkoutLogWithExercise{Weight: weight}
	}
	last := func(weight float64) models.LastExerciseWeight { return models.LastExerciseWeight{Weight: weight} }

	got := weightSuggestions(
		[]models.WorkoutLogWithExercise{logged(100), logged(82.5), logged(100), logged(0), logged(-5)},
		[]models.LastExerciseWeight{last(60), last(100)},
	)
	want := []float64{60, 82.5, 100}
	if len(got) != len(want) {
		t.Fatalf("got %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("got %v, want %v", got, want)
		}
	}
	if len(weightSuggestions(nil, nil)) != 0 {
		t.Error("a first-ever workout has suggestions")
	}
}

func TestLastWeightRowsAreSortedByExerciseName(t *testing.T) {
	exercises := []models.Exercise{
		{ID: "1", Name: "Squat"}, {ID: "2", Name: "Bench"}, {ID: "3", Name: "Never logged"},
	}
	last := []models.LastExerciseWeight{{ExerciseID: "1", Weight: 100}, {ExerciseID: "2", Weight: 60}}

	rows := lastWeightRows(exercises, last)

	if len(rows) != 2 || rows[0].ExerciseName != "Bench" || rows[1].ExerciseName != "Squat" {
		t.Errorf("rows = %+v", rows)
	}
}

func TestShowWorkoutListsLastWeightsForScriptlessBrowsers(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	bench := ta.wkExercise(user.user.ID, "Bench Press", "chest")
	past := ta.wkWorkout(user.user.ID, nil)
	ta.wkLog(wkSet{workoutID: past.ID, exerciseID: bench.ID, number: 1, reps: 5, weight: 62.5})
	current := ta.wkWorkout(user.user.ID, nil)

	body := ta.get("/workouts/"+current.ID, user.cookie).text()

	wantFragments(t, body, "<noscript>", "Bench Press", "62.5")
}

func TestShareInfoIgnoresExpiredLinks(t *testing.T) {
	now := time.Date(2026, 11, 17, 12, 0, 0, 0, time.UTC)
	token := "tok"
	past, future := now.Add(-time.Hour), now.Add(time.Hour)

	if shareInfo(models.WorkoutSession{}, now) != nil {
		t.Error("no token, yet share info")
	}
	if shareInfo(models.WorkoutSession{ShareToken: &token, ShareExpiresAt: &past}, now) != nil {
		t.Error("expired token shown as shared")
	}
	live := shareInfo(models.WorkoutSession{ShareToken: &token, ShareExpiresAt: &future}, now)
	if live == nil || live.URL != "/shared/tok" {
		t.Errorf("live = %+v", live)
	}
	forever := shareInfo(models.WorkoutSession{ShareToken: &token}, now)
	if forever == nil || forever.ExpiresAt != nil {
		t.Errorf("forever = %+v", forever)
	}
}

func TestWorkoutPageBadgesAOneMonthPRBelowTheAllTimeBest(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	now := time.Date(2026, 11, 17, 12, 0, 0, 0, time.UTC)
	ta.app.now = func() time.Time { return now }
	user := ta.signInUser()
	exercise := ta.wkExercise(user.user.ID, "Squat", "legs")
	old := ta.wkLog(wkSet{workoutID: ta.wkWorkout(user.user.ID, nil).ID, exerciseID: exercise.ID, number: 1, reps: 5, weight: 200})
	ta.mustExec("UPDATE workout_logs SET created_at = ? WHERE id = ?", db.FormatTime(now.AddDate(0, 0, -60)), old.ID)
	recentWorkout := ta.wkWorkout(user.user.ID, nil)
	recent := ta.wkLog(wkSet{workoutID: recentWorkout.ID, exerciseID: exercise.ID, number: 1, reps: 5, weight: 100})
	ta.mustExec("UPDATE workout_logs SET created_at = ? WHERE id = ?", db.FormatTime(now.AddDate(0, 0, -1)), recent.ID)

	page := ta.get("/workouts/"+recentWorkout.ID, user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, "pr-badge-recent")
	wantContains(t, page, "PR 1M")
}
