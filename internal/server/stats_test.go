package server

import (
	"fmt"
	"net/http"
	"regexp"
	"strings"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/repo"
)

// statsSet is one set to log, on a workout of its own dated Date.
type statsSet struct {
	userID     string
	exerciseID string
	date       time.Time
	reps       int
	weight     float64
}

func statsDate(year int, month time.Month, day int) time.Time {
	return time.Date(year, month, day, 0, 0, 0, 0, time.UTC)
}

func (ta *testApp) seedStatsExercise(userID, name, category string) models.Exercise {
	ta.t.Helper()
	exercise, err := ta.app.exercises.Create(ta.t.Context(), repo.NewExercise{Name: name, Category: category, UserID: userID})
	if err != nil {
		ta.t.Fatalf("create exercise: %v", err)
	}
	return exercise
}

func (ta *testApp) seedStatsWorkout(userID string, date time.Time, notes *string) models.WorkoutSession {
	ta.t.Helper()
	workout, err := ta.app.workouts.CreateSession(ta.t.Context(), userID, date, notes)
	if err != nil {
		ta.t.Fatalf("create workout: %v", err)
	}
	return workout
}

func (ta *testApp) seedStatsLog(workoutID, exerciseID string, reps int, weight float64) models.WorkoutLog {
	ta.t.Helper()
	log, err := ta.app.workouts.CreateLog(ta.t.Context(), repo.NewLog{
		SessionID: workoutID, ExerciseID: exerciseID, SetNumber: 1, Reps: reps, Weight: weight,
	})
	if err != nil {
		ta.t.Fatalf("create log: %v", err)
	}
	return log
}

func (ta *testApp) seedStatsSet(set statsSet) {
	ta.t.Helper()
	workout := ta.seedStatsWorkout(set.userID, set.date, nil)
	ta.seedStatsLog(workout.ID, set.exerciseID, set.reps, set.weight)
}

// seedStatsThreeSessions logs 100, 105 and 110 kg for five reps. The top-set
// axis lands in 99-111 and volume in 495-555, so a tick label proves which
// series was plotted.
func (ta *testApp) seedStatsThreeSessions(userID string) models.Exercise {
	ta.t.Helper()
	exercise := ta.seedStatsExercise(userID, "Bench Press", "chest")
	for i, weight := range []float64{100, 105, 110} {
		ta.seedStatsSet(statsSet{userID: userID, exerciseID: exercise.ID, date: statsDate(2024, 1, 10+i*2), reps: 5, weight: weight})
	}
	return exercise
}

var statValuePattern = regexp.MustCompile(`<div class="stat-value">([^<]*)</div>`)

func statValues(body string) []string {
	var values []string
	for _, match := range statValuePattern.FindAllStringSubmatch(body, -1) {
		values = append(values, match[1])
	}
	return values
}

func TestStatsPagesRequireSignIn(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	for _, path := range []string{"/", "/stats", "/stats/prs", "/stats/exercise/any"} {
		wantRedirect(t, ta.get(path, ""), "/auth/login")
	}
}

func TestStatsIndexShowsWorkoutCountsAndVolume(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	ta.app.now = func() time.Time { return time.Date(2026, 11, 17, 12, 0, 0, 0, time.UTC) }
	user := ta.signInUser()
	today := statsDate(2026, 11, 17)
	exercise := ta.seedStatsExercise(user.user.ID, "Bench Press", "chest")
	ta.seedStatsSet(statsSet{userID: user.user.ID, exerciseID: exercise.ID, date: today, reps: 10, weight: 100})
	ta.seedStatsWorkout(user.user.ID, today.AddDate(0, 0, -2), nil)
	ta.seedStatsWorkout(user.user.ID, today.AddDate(0, 0, -20), nil)
	ta.seedStatsWorkout(user.user.ID, today.AddDate(0, 0, -60), nil)

	page := ta.get("/stats", user.cookie)

	wantStatus(t, page, http.StatusOK)
	// Total, this week, this month, week volume.
	if got, want := statValues(page.text()), []string{"4", "2", "3", "1000"}; fmt.Sprint(got) != fmt.Sprint(want) {
		t.Fatalf("stat values = %v, want %v", got, want)
	}
	wantContains(t, page, "Bench Press")
}

func TestStatsIndexShowsAllTimeAndRecentPRsSeparately(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	now := time.Date(2026, 11, 17, 12, 0, 0, 0, time.UTC)
	ta.app.now = func() time.Time { return now }
	user := ta.signInUser()
	exercise := ta.seedStatsExercise(user.user.ID, "Deadlift", "back")
	oldWorkout := ta.seedStatsWorkout(user.user.ID, statsDate(2026, 9, 1), nil)
	oldLog := ta.seedStatsLog(oldWorkout.ID, exercise.ID, 5, 200)
	ta.mustExec("UPDATE workout_logs SET created_at = ? WHERE id = ?", db.FormatTime(now.AddDate(0, 0, -60)), oldLog.ID)
	recentWorkout := ta.seedStatsWorkout(user.user.ID, statsDate(2026, 11, 16), nil)
	recentLog := ta.seedStatsLog(recentWorkout.ID, exercise.ID, 5, 120)
	ta.mustExec("UPDATE workout_logs SET created_at = ? WHERE id = ?", db.FormatTime(now.AddDate(0, 0, -1)), recentLog.ID)

	page := ta.get("/stats", user.cookie)

	wantStatus(t, page, http.StatusOK)
	row := tableRowContaining(t, page.text(), "Deadlift")
	if !strings.Contains(row, "200") || !strings.Contains(row, "120") {
		t.Fatalf("the row lacks the all-time (200) or the 1M (120) PR: %s", row)
	}
}

func TestPRsListShowsEveryExercise(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	bench := ta.seedStatsExercise(user.user.ID, "Bench Press", "chest")
	squat := ta.seedStatsExercise(user.user.ID, "Squat", "legs")
	workout := ta.seedStatsWorkout(user.user.ID, statsDate(2024, 1, 15), nil)
	ta.seedStatsLog(workout.ID, bench.ID, 5, 100)
	ta.seedStatsLog(workout.ID, squat.ID, 5, 150)

	page := ta.get("/stats/prs", user.cookie)

	wantStatus(t, page, http.StatusOK)
	for _, fragment := range []string{"Bench Press", "Squat", "100", "150"} {
		wantContains(t, page, fragment)
	}
}

func TestPRsListShowsTheEmptyState(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()

	page := ta.get("/stats/prs", user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, "No personal records yet")
}

func tableRowContaining(t *testing.T, body, name string) string {
	t.Helper()
	for _, row := range strings.Split(body, "<tr>") {
		if strings.Contains(row, name) {
			return row
		}
	}
	t.Fatalf("no row for %s in:\n%s", name, body)
	return ""
}

func TestPRsListSeparatesAllTimeAndRecentWindows(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	bench := ta.seedStatsExercise(user.user.ID, "Bench Press", "chest")
	squat := ta.seedStatsExercise(user.user.ID, "Squat", "legs")
	workout := ta.seedStatsWorkout(user.user.ID, statsDate(2024, 1, 15), nil)
	ta.seedStatsLog(workout.ID, bench.ID, 5, 100)
	oldSquat := ta.seedStatsLog(workout.ID, squat.ID, 5, 150)
	// Squat only outside the one-month window.
	ta.mustExec("UPDATE workout_logs SET created_at = ? WHERE id = ?",
		db.FormatTime(ta.app.now().UTC().AddDate(0, 0, -60)), oldSquat.ID)

	page := ta.get("/stats/prs", user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, "PR (All)")
	wantContains(t, page, "PR (1M)")
	squatRow := tableRowContaining(t, page.text(), "Squat")
	if !strings.Contains(squatRow, "150") || !strings.Contains(squatRow, "&mdash;") {
		t.Fatalf("squat row: %s", squatRow)
	}
	benchRow := tableRowContaining(t, page.text(), "Bench Press")
	if !strings.Contains(benchRow, "100") || strings.Contains(benchRow, "&mdash;") {
		t.Fatalf("bench row: %s", benchRow)
	}
}

func TestExerciseStatsShowsHistory(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.seedStatsExercise(user.user.ID, "Bench Press", "chest")
	ta.seedStatsSet(statsSet{userID: user.user.ID, exerciseID: exercise.ID, date: statsDate(2024, 1, 15), reps: 10, weight: 100})
	ta.seedStatsSet(statsSet{userID: user.user.ID, exerciseID: exercise.ID, date: statsDate(2024, 1, 20), reps: 8, weight: 110})

	page := ta.get("/stats/exercise/"+exercise.ID, user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, "Bench Press")
	wantContains(t, page, "110")
}

func TestExerciseStatsOfAMissingExerciseIs404(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()

	page := ta.get("/stats/exercise/nonexistent-id", user.cookie)

	wantStatus(t, page, http.StatusNotFound)
	if page.text() != "Exercise not found" {
		t.Fatalf("body = %q", page.text())
	}
}

func TestExerciseStatsRefusesAnotherUsersExercise(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	snooper := ta.signIn("snooper", models.RoleUser)
	victim := ta.signIn("victim", models.RoleUser)
	exercise := ta.seedStatsExercise(victim.user.ID, "Victim Deadlift", "back")

	page := ta.get("/stats/exercise/"+exercise.ID, snooper.cookie)

	wantStatus(t, page, http.StatusNotFound)
	if strings.Contains(page.text(), "Victim Deadlift") {
		t.Fatalf("the other user's exercise name leaked:\n%s", page.text())
	}
}

func TestExerciseStatsChartNeedsTwoSessions(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	empty := ta.seedStatsExercise(user.user.ID, "Squat", "legs")
	sparse := ta.seedStatsExercise(user.user.ID, "Deadlift", "back")
	ta.seedStatsSet(statsSet{userID: user.user.ID, exerciseID: sparse.ID, date: statsDate(2024, 1, 15), reps: 5, weight: 100})

	emptyPage := ta.get("/stats/exercise/"+empty.ID, user.cookie)
	wantContains(t, emptyPage, "No progress data yet")
	sparsePage := ta.get("/stats/exercise/"+sparse.ID, user.cookie)
	wantContains(t, sparsePage, "Need at least 2 sessions")
	for _, page := range []response{emptyPage, sparsePage} {
		if strings.Contains(page.text(), "<polyline") {
			t.Fatalf("a chart was drawn:\n%s", page.text())
		}
	}
	if strings.Contains(emptyPage.text(), `id="exercise-chart-data"`) {
		t.Fatal("the empty state embeds chart data")
	}
}

func TestExerciseStatsChartEmbedsEverySession(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.seedStatsThreeSessions(user.user.ID)

	page := ta.get("/stats/exercise/"+exercise.ID, user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, "<polyline")
	wantContains(t, page, `id="chart-line"`)
	_, data, found := strings.Cut(page.text(), `id="exercise-chart-data">`)
	if !found {
		t.Fatal("chart data script missing")
	}
	data, _, _ = strings.Cut(data, "</script>")
	if got := strings.Count(data, `"date"`); got != 3 {
		t.Fatalf("%d points embedded, want 3: %s", got, data)
	}
	for _, field := range []string{"top_weight", "top_reps", "volume", "e1rm"} {
		if !strings.Contains(data, `"`+field+`"`) {
			t.Fatalf("field %s missing: %s", field, data)
		}
	}
}

func TestExerciseStatsChartMarksTheRunningBestAsPR(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.seedStatsExercise(user.user.ID, "Bench Press", "chest")
	// Running-max PRs at indices 0, 2, 4.
	for i, weight := range []float64{100, 100, 110, 105, 120} {
		ta.seedStatsSet(statsSet{userID: user.user.ID, exerciseID: exercise.ID, date: statsDate(2024, 1, 10+i), reps: 5, weight: weight})
	}

	page := ta.get("/stats/exercise/"+exercise.ID, user.cookie)

	if got := strings.Count(page.text(), `class="ll-dot-pr"`); got != 3 {
		t.Fatalf("%d PR dots, want 3", got)
	}
	if got := strings.Count(page.text(), `class="ll-dot"`); got != 2 {
		t.Fatalf("%d plain dots, want 2", got)
	}
}

func TestExerciseStatsChartPlotsTheRequestedMetric(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.seedStatsThreeSessions(user.user.ID)
	path := "/stats/exercise/" + exercise.ID

	volume := ta.get(path+"?metric=volume", user.cookie)
	wantContains(t, volume, ">555</text>")
	wantContains(t, volume, ">495</text>")
	wantContains(t, volume, `class="btn btn-sm btn-tab is-active" data-metric="volume"`)
	if strings.Contains(volume.text(), ">111</text>") {
		t.Fatal("the top-set axis survived into the volume chart")
	}

	topSet := ta.get(path, user.cookie)
	wantContains(t, topSet, ">111</text>")
	wantContains(t, topSet, ">99</text>")
}

func TestExerciseStatsChartPlotsE1RM(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.seedStatsExercise(user.user.ID, "Deadlift", "back")
	// Ten reps put the e1RM axis where neither top set nor volume can.
	for i, weight := range []float64{100, 110, 120} {
		ta.seedStatsSet(statsSet{userID: user.user.ID, exerciseID: exercise.ID, date: statsDate(2024, 1, 10+i*2), reps: 10, weight: weight})
	}

	page := ta.get("/stats/exercise/"+exercise.ID+"?metric=e1rm", user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, ">163</text>")
	wantContains(t, page, ">131</text>")
	wantContains(t, page, `class="btn btn-sm btn-tab is-active" data-metric="e1rm"`)
}

func TestExerciseStatsChartHasNativeHoverTitles(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.seedStatsThreeSessions(user.user.ID)

	page := ta.get("/stats/exercise/"+exercise.ID, user.cookie)

	// Scoped to the hit-area group: the document's own <title> is in <head>.
	_, bands, found := strings.Cut(page.text(), `<g id="chart-hit-areas">`)
	if !found {
		t.Fatal("hit-area group missing")
	}
	bands, _, _ = strings.Cut(bands, "</g>")
	if got := strings.Count(bands, "<title>"); got != 3 {
		t.Fatalf("%d hover bands, want 3:\n%s", got, bands)
	}
	for _, fragment := range []string{"Top: 100 kg × 5", "Volume: 500 kg", "e1RM: 116.7 kg"} {
		if !strings.Contains(bands, fragment) {
			t.Fatalf("bands lack %q:\n%s", fragment, bands)
		}
	}
}

func TestExerciseStatsChartRangeControlsTheWindow(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.seedStatsExercise(user.user.ID, "Squat", "legs")
	for day := 1; day <= 22; day++ {
		ta.seedStatsSet(statsSet{userID: user.user.ID, exerciseID: exercise.ID, date: statsDate(2024, 1, day), reps: 5, weight: float64(100 + day)})
	}
	path := "/stats/exercise/" + exercise.ID

	defaultPage := ta.get(path, user.cookie)
	if strings.Contains(defaultPage.text(), ">01-01</text>") {
		t.Fatal("the last-20 window reaches the first session")
	}
	allPage := ta.get(path+"?range=all", user.cookie)
	wantContains(t, allPage, ">01-01</text>")
	wantContains(t, allPage, `data-range="all"`)
}

func TestExerciseStatsQueryIsForgivingAndTabLinksCompose(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.seedStatsThreeSessions(user.user.ID)
	path := "/stats/exercise/" + exercise.ID

	bogus := ta.get(path+"?metric=bogus&range=bogus", user.cookie)
	wantStatus(t, bogus, http.StatusOK)
	wantContains(t, bogus, ">111</text>")

	composed := ta.get(path+"?metric=volume&range=all", user.cookie)
	wantContains(t, composed, `href="?metric=volume&amp;range=20"`)
	wantContains(t, composed, `href="?metric=e1rm&amp;range=all"`)
}

func TestDashboardListsRecentWorkoutsWithTheirNotes(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	notes := "Heavy singles"
	ta.seedStatsWorkout(user.user.ID, statsDate(2024, 1, 15), &notes)

	page := ta.get("/", user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, notes)
}

func TestDashboardShowsOnlyTheFiveNewestWorkouts(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	for day := 1; day <= 6; day++ {
		note := fmt.Sprintf("note-day-%02d", day)
		ta.seedStatsWorkout(user.user.ID, statsDate(2024, 3, day), &note)
	}

	page := ta.get("/", user.cookie)

	wantContains(t, page, "note-day-06")
	if strings.Contains(page.text(), "note-day-01") {
		t.Fatal("the sixth newest workout is listed")
	}
}

func TestDashboardPromptsForTimezoneUntilOneIsSaved(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()

	wantContains(t, ta.get("/", user.cookie), "Your timezone is not set")

	if _, err := ta.app.users.SetTimezone(t.Context(), user.user.ID, "Asia/Taipei"); err != nil {
		t.Fatal(err)
	}
	if strings.Contains(ta.get("/", user.cookie).text(), "Your timezone is not set") {
		t.Fatal("the prompt stays after a timezone is saved")
	}
}

// 20:00 UTC on 11-17 is already 11-18 in Taipei, so the seven-day window starts
// on 11-12 there and on 11-11 in UTC.
func TestDashboardWindowsFollowTheUsersTimezone(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	ta.app.now = func() time.Time { return time.Date(2026, 11, 17, 20, 0, 0, 0, time.UTC) }
	user := ta.signInUser()
	exercise := ta.seedStatsExercise(user.user.ID, "Bench Press", "chest")
	ta.seedStatsSet(statsSet{userID: user.user.ID, exerciseID: exercise.ID, date: statsDate(2026, 11, 11), reps: 10, weight: 50})

	utcValues := statValues(ta.get("/", user.cookie).text())
	if want := []string{"1", "1", "500"}; fmt.Sprint(utcValues) != fmt.Sprint(want) {
		t.Fatalf("UTC stats = %v, want %v", utcValues, want)
	}

	if _, err := ta.app.users.SetTimezone(t.Context(), user.user.ID, "Asia/Taipei"); err != nil {
		t.Fatal(err)
	}
	taipeiValues := statValues(ta.get("/", user.cookie).text())
	if want := []string{"0", "1", "0"}; fmt.Sprint(taipeiValues) != fmt.Sprint(want) {
		t.Fatalf("Taipei stats = %v, want %v", taipeiValues, want)
	}
}
