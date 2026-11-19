package views

import (
	"context"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/chart"
	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/repo"
	"github.com/henry40408/liftlog/internal/version"
	"github.com/henry40408/liftlog/migrations"
)

const wkGoldenVersion = "v0.30.1-1-g19c79d8"

// wkClock is the capture time of the goldens (testdata/README.txt).
var wkClock = time.Date(2026, 11, 16, 16, 40, 0, 0, time.UTC)

const (
	wkAlice = "b6db7e65-fdaf-5a9b-ad5c-644b892fafd3"
	wkBob   = "244f6aef-a7c9-5119-8ae6-7d18f318df5d"
	wkCarol = "595fe64c-d94c-5100-9761-4545d1563589"

	wkSquat    = "aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea"
	wkBobSquat = "22a1bd31-38fc-53db-9ce2-9d43243ea063"

	wkWorkoutShow    = "0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd"
	wkWorkoutEmpty   = "71cfcccf-b0aa-533b-84e4-55839cfbd941"
	wkWorkoutNotes   = "02782218-b94f-5b6c-9a5c-672691802f7e"
	wkWorkoutShared  = "4592a164-902e-551c-a8e9-a7cf803de5da"
	wkWorkoutForever = "d5cc92da-73eb-5a82-84bb-98b36109a2fd"
	wkWorkoutExpired = "d832baf8-56b4-5105-9e97-1be229372651"
	wkWorkoutBodyw   = "ecec20c9-a65f-5b32-b372-e344852aa532"
	wkWorkoutBob     = "c76f5408-51e6-55ce-b637-4c873b32afca"

	wkLogRPE     = "a1e7f98d-1592-55db-b31e-b0514388cde2"
	wkLogNoRPE   = "cfec4f55-8e9a-559e-9c6e-bcaee6254f90"
	wkLogForeign = "8cc802ff-bd30-5526-8da0-4886aa207173"

	wkShareActive  = "seed-share-active-0000000000000000000000000000000000000000"
	wkShareForever = "seed-share-forever-000000000000000000000000000000000000000"
)

var (
	wkAliceNav = NavUser{Username: "alice", IsAdmin: true}
	wkBobNav   = NavUser{Username: "bob"}
	wkCarolNav = NavUser{Username: "carol"}
)

type wkWorld struct {
	workouts  *repo.Workouts
	exercises *repo.Exercises
}

// wkSeeded is a migrated in-memory database with the golden seed applied.
func wkSeeded(t *testing.T) wkWorld {
	t.Helper()
	pool, err := db.OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = pool.Close() })
	ctx := context.Background()
	if err := migrations.RunForTests(ctx, pool); err != nil {
		t.Fatal(err)
	}
	seed, err := os.ReadFile(filepath.Join("testdata", "seed.sql"))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := pool.ExecContext(ctx, string(seed)); err != nil {
		t.Fatal(err)
	}
	return wkWorld{workouts: repo.NewWorkouts(pool), exercises: repo.NewExercises(pool)}
}

func wkAssertGolden(t *testing.T, name, got string) {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join("testdata", name+".golden.html"))
	if err != nil {
		t.Fatal(err)
	}
	want := strings.ReplaceAll(string(raw), wkGoldenVersion, version.Version())
	if got == want {
		return
	}
	i := 0
	for i < len(got) && i < len(want) && got[i] == want[i] {
		i++
	}
	t.Errorf("%s differs from the Rust golden at byte %d\ngot:  ...%s\nwant: ...%s",
		name, i, wkAround(got, i), wkAround(want, i))
}

func wkAround(s string, at int) string {
	return s[max(at-80, 0):min(at+160, len(s))]
}

func wkRecentSince() time.Time { return models.RecentPRWindowStart(wkClock) }

func (w wkWorld) session(t *testing.T, id, userID string) models.WorkoutSession {
	t.Helper()
	session, err := w.workouts.FindOwnedSession(context.Background(), id, userID)
	if err != nil {
		t.Fatal(err)
	}
	return session
}

// wkWeightSuggestions mirrors the handler: ascending, distinct, positive.
func wkWeightSuggestions(logs []models.WorkoutLogWithExercise, last []models.LastExerciseWeight) []float64 {
	var weights []float64
	for _, l := range logs {
		weights = append(weights, l.Weight)
	}
	for _, l := range last {
		weights = append(weights, l.Weight)
	}
	weights = slices.DeleteFunc(weights, func(w float64) bool { return w <= 0 })
	slices.Sort(weights)
	return slices.Compact(weights)
}

func wkLastWeightRows(exercises []models.Exercise, last []models.LastExerciseWeight) []LastWeightRow {
	var rows []LastWeightRow
	for _, ex := range exercises {
		i := slices.IndexFunc(last, func(l models.LastExerciseWeight) bool { return l.ExerciseID == ex.ID })
		if i < 0 {
			continue
		}
		rows = append(rows, LastWeightRow{ExerciseName: ex.Name, Weight: last[i].Weight, RPE: last[i].RPE, LoggedAt: last[i].LoggedAt})
	}
	slices.SortStableFunc(rows, func(a, b LastWeightRow) int { return strings.Compare(a.ExerciseName, b.ExerciseName) })
	return rows
}

type wkShowCase struct {
	golden    string
	user      NavUser
	userID    string
	workoutID string
	prefillID string
}

// showView mirrors the handler's assembly of the workout page.
func (w wkWorld) showView(t *testing.T, c wkShowCase) ShowWorkoutView {
	t.Helper()
	ctx := context.Background()
	workout := w.session(t, c.workoutID, c.userID)
	logs, err := w.workouts.FindLogsBySessionWithPR(ctx, c.workoutID, c.userID, wkRecentSince())
	if err != nil {
		t.Fatal(err)
	}
	exercises, err := w.exercises.FindAvailableForUser(ctx, c.userID)
	if err != nil {
		t.Fatal(err)
	}
	last, err := w.workouts.LastWeightPerExerciseByUser(ctx, c.userID)
	if err != nil {
		t.Fatal(err)
	}
	view := ShowWorkoutView{
		User: c.user, Workout: workout, Logs: logs, Exercises: exercises, Categories: models.Categories,
		ExerciseLastWeights: last, LastWeightRows: wkLastWeightRows(exercises, last),
		WeightSuggestions: wkWeightSuggestions(logs, last), RepSuggestions: models.RepSchemes,
	}
	for _, l := range logs {
		if l.ID == c.prefillID {
			view.Prefill = &PrefillSet{ExerciseID: l.ExerciseID, Weight: l.Weight, Reps: l.Reps, RPE: l.RPE}
		}
	}
	// The hourly sweep had already cleared the expired token when the goldens
	// were captured.
	if workout.ShareToken != nil && (workout.ShareExpiresAt == nil || workout.ShareExpiresAt.After(wkClock)) {
		view.Share = &ShareInfo{URL: "/shared/" + *workout.ShareToken, ExpiresAt: workout.ShareExpiresAt}
	}
	return view
}

func TestWorkoutsListGoldens(t *testing.T) {
	w := wkSeeded(t)
	ctx := context.Background()
	cases := []struct {
		golden string
		user   NavUser
		userID string
		page   int
	}{
		{"workouts-list", wkAliceNav, wkAlice, 1},
		{"workouts-list-page2", wkAliceNav, wkAlice, 2},
		{"workouts-list-page3", wkAliceNav, wkAlice, 3},
		{"workouts-list-page-beyond", wkAliceNav, wkAlice, 9},
		{"workouts-list-page-zero", wkAliceNav, wkAlice, 1},
		{"workouts-list-empty", wkCarolNav, wkCarol, 1},
	}
	const perPage = 10
	for _, c := range cases {
		workouts, err := w.workouts.FindSessionsByUserPaginated(ctx, c.userID, perPage, (c.page-1)*perPage)
		if err != nil {
			t.Fatal(err)
		}
		total, err := w.workouts.CountSessionsByUser(ctx, c.userID)
		if err != nil {
			t.Fatal(err)
		}
		view := WorkoutsListView{User: c.user, Workouts: workouts, Page: c.page, TotalPages: (total + perPage - 1) / perPage}
		wkAssertGolden(t, c.golden, WorkoutsList(view))
	}
}

func TestNewWorkoutGoldens(t *testing.T) {
	alice := NewWorkoutView{User: wkAliceNav, Today: time.Date(2026, 11, 16, 0, 0, 0, 0, time.UTC), UseBrowserToday: true}
	bob := NewWorkoutView{User: wkBobNav, Today: time.Date(2026, 11, 17, 0, 0, 0, 0, time.UTC)}
	wkAssertGolden(t, "workouts-new", NewWorkout(alice))
	wkAssertGolden(t, "workouts-new-timezone-set", NewWorkout(bob))
}

func TestNewWorkoutShowsTheEscapedError(t *testing.T) {
	got := NewWorkout(NewWorkoutView{User: wkAliceNav, Today: wkClock, Error: "Bad <date>"})
	if !strings.Contains(got, `<div class="error">Bad &lt;date&gt;</div>`) {
		t.Errorf("error box missing or unescaped: %s", got)
	}
}

func TestShowWorkoutGoldens(t *testing.T) {
	w := wkSeeded(t)
	cases := []wkShowCase{
		{"workout-show", wkAliceNav, wkAlice, wkWorkoutShow, ""},
		{"workout-show-prefill", wkAliceNav, wkAlice, wkWorkoutShow, wkLogNoRPE},
		{"workout-show-prefill-foreign", wkAliceNav, wkAlice, wkWorkoutShow, wkLogForeign},
		{"workout-show-prefill-unknown", wkAliceNav, wkAlice, wkWorkoutShow, "nope"},
		{"workout-show-prefill-rpe", wkAliceNav, wkAlice, wkWorkoutShow, wkLogRPE},
		{"workout-show-empty", wkAliceNav, wkAlice, wkWorkoutEmpty, ""},
		{"workout-show-notes", wkAliceNav, wkAlice, wkWorkoutNotes, ""},
		{"workout-show-shared", wkAliceNav, wkAlice, wkWorkoutShared, ""},
		{"workout-show-shared-forever", wkAliceNav, wkAlice, wkWorkoutForever, ""},
		{"workout-show-shared-expired", wkAliceNav, wkAlice, wkWorkoutExpired, ""},
		{"workout-show-bodyweight", wkAliceNav, wkAlice, wkWorkoutBodyw, ""},
		{"workout-show-timezone-set", wkBobNav, wkBob, wkWorkoutBob, ""},
	}
	for _, c := range cases {
		wkAssertGolden(t, c.golden, ShowWorkout(w.showView(t, c)))
	}
}

func TestEditWorkoutGoldens(t *testing.T) {
	w := wkSeeded(t)
	wkAssertGolden(t, "workout-edit", EditWorkout(wkAliceNav, w.session(t, wkWorkoutNotes, wkAlice), ""))
	wkAssertGolden(t, "workout-edit-no-notes", EditWorkout(wkAliceNav, w.session(t, wkWorkoutShow, wkAlice), ""))
}

func TestEditLogGoldens(t *testing.T) {
	w := wkSeeded(t)
	ctx := context.Background()
	workout := w.session(t, wkWorkoutShow, wkAlice)
	for golden, logID := range map[string]string{"workout-edit-log": wkLogRPE, "workout-edit-log-no-rpe": wkLogNoRPE} {
		log, err := w.workouts.FindLogByID(ctx, logID)
		if err != nil {
			t.Fatal(err)
		}
		exercise, err := w.exercises.FindByID(ctx, log.ExerciseID)
		if err != nil {
			t.Fatal(err)
		}
		wkAssertGolden(t, golden, EditLog(EditLogView{User: wkAliceNav, Workout: workout, Log: log, ExerciseName: exercise.Name}))
	}
}

func TestSharedWorkoutGoldens(t *testing.T) {
	w := wkSeeded(t)
	ctx := context.Background()
	for golden, token := range map[string]string{"shared": wkShareActive, "shared-forever": wkShareForever} {
		workout, err := w.workouts.FindSessionByShareToken(ctx, token)
		if err != nil {
			t.Fatal(err)
		}
		logs, err := w.workouts.FindLogsBySessionForShare(ctx, workout.ID)
		if err != nil {
			t.Fatal(err)
		}
		wkAssertGolden(t, golden, SharedWorkout(workout, logs, "alice"))
	}
}

func TestSharedWorkoutWithoutSets(t *testing.T) {
	got := SharedWorkout(models.WorkoutSession{Date: wkClock}, nil, "alice")
	if !strings.Contains(got, `<p class="muted">No sets recorded.</p>`) {
		t.Errorf("empty copy missing: %s", got)
	}
}

type wkUserCase struct {
	golden string
	user   NavUser
	userID string
}

func TestStatsIndexGoldens(t *testing.T) {
	w := wkSeeded(t)
	// Alice has no timezone, so the week and month windows are UTC dates.
	today := time.Date(2026, 11, 16, 0, 0, 0, 0, time.UTC)
	for _, c := range []wkUserCase{
		{"stats-index", wkAliceNav, wkAlice},
		{"stats-index-empty", wkCarolNav, wkCarol},
	} {
		view := w.statsView(t, c.userID, today)
		view.User = c.user
		wkAssertGolden(t, c.golden, StatsIndex(view))
	}
}

func TestPRsGoldens(t *testing.T) {
	w := wkSeeded(t)
	for _, c := range []wkUserCase{
		{"stats-prs", wkAliceNav, wkAlice},
		{"stats-prs-empty", wkCarolNav, wkCarol},
	} {
		prs, err := w.workouts.PRSummariesByUser(context.Background(), c.userID, wkRecentSince())
		if err != nil {
			t.Fatal(err)
		}
		wkAssertGolden(t, c.golden, PRs(c.user, prs))
	}
}

func (w wkWorld) statsView(t *testing.T, userID string, today time.Time) StatsView {
	t.Helper()
	ctx := context.Background()
	weekStart, monthStart := today.AddDate(0, 0, -6), today.AddDate(0, 0, -29)
	var view StatsView
	var err error
	if view.WorkoutsThisWeek, err = w.workouts.CountSessionsSince(ctx, userID, weekStart); err != nil {
		t.Fatal(err)
	}
	if view.WorkoutsThisMonth, err = w.workouts.CountSessionsSince(ctx, userID, monthStart); err != nil {
		t.Fatal(err)
	}
	if view.TotalVolume, err = w.workouts.TotalVolumeSince(ctx, userID, weekStart); err != nil {
		t.Fatal(err)
	}
	if view.TotalWorkouts, err = w.workouts.CountSessionsByUser(ctx, userID); err != nil {
		t.Fatal(err)
	}
	if view.PRs, err = w.workouts.PRSummariesByUser(ctx, userID, wkRecentSince()); err != nil {
		t.Fatal(err)
	}
	return view
}

type wkExerciseCase struct {
	golden     string
	user       NavUser
	userID     string
	exerciseID string
	metric     string
	rng        string
}

func (w wkWorld) exerciseView(t *testing.T, c wkExerciseCase) ExerciseStatsView {
	t.Helper()
	ctx := context.Background()
	exercise, err := w.exercises.FindOwned(ctx, c.exerciseID, c.userID)
	if err != nil {
		t.Fatal(err)
	}
	history, err := w.workouts.ExerciseHistoryWithPR(ctx, repo.ExerciseHistoryQuery{
		UserID: c.userID, ExerciseID: c.exerciseID, Limit: 50, Since: wkRecentSince(),
	})
	if err != nil {
		t.Fatal(err)
	}
	view := ExerciseStatsView{User: c.user, Exercise: exercise, History: history}
	if pr, err := w.workouts.MaxWeightForExercise(ctx, c.userID, c.exerciseID); err == nil {
		view.PR = &pr
	}
	metrics, err := w.workouts.SessionMetricsForExercise(ctx, c.userID, c.exerciseID)
	if err != nil {
		t.Fatal(err)
	}
	points := make([]models.ChartPoint, len(metrics))
	for i, m := range metrics {
		points[i] = models.NewChartPoint(m)
	}
	view.Metric, view.Range = chart.ParseMetric(c.metric), chart.ParseRange(c.rng)
	view.SessionCount = len(points)
	view.Chart = chart.Render(points, view.Metric, view.Range)
	view.ChartDataJSON = chart.EncodeData(points)
	return view
}

func TestExerciseStatsGoldens(t *testing.T) {
	w := wkSeeded(t)
	const (
		bench    = "fbe58868-1aa5-5ab6-ad18-9dd398a49b5b"
		deadlift = "f2efc5aa-2ac8-54b6-af56-933b6c5be8cf"
		ohp      = "9f62366c-2d2b-5ab4-ab90-a9a93d1b08de"
		pullup   = "de28bf00-4cb9-5486-877d-dd7f8f55f010"
		curl     = "9264ffa1-b941-569f-88ec-e1f12d75ad74"
		plank    = "fac81715-c51c-5bae-a239-e6359a311d8e"
	)
	alice := func(golden, exercise, metric, rng string) wkExerciseCase {
		return wkExerciseCase{golden, wkAliceNav, wkAlice, exercise, metric, rng}
	}
	cases := []wkExerciseCase{
		alice("stats-exercise-squat", wkSquat, "", ""),
		alice("stats-exercise-squat-top_set-20", wkSquat, "top_set", "20"),
		alice("stats-exercise-squat-top_set-all", wkSquat, "top_set", "all"),
		alice("stats-exercise-squat-e1rm-20", wkSquat, "e1rm", "20"),
		alice("stats-exercise-squat-e1rm-all", wkSquat, "e1rm", "all"),
		alice("stats-exercise-squat-volume-20", wkSquat, "volume", "20"),
		alice("stats-exercise-squat-volume-all", wkSquat, "volume", "all"),
		alice("stats-exercise-squat-unknown-query", wkSquat, "bogus", "bogus"),
		alice("stats-exercise-bench", bench, "e1rm", ""),
		alice("stats-exercise-deadlift", deadlift, "volume", ""),
		alice("stats-exercise-ohp", ohp, "", ""),
		alice("stats-exercise-pullup", pullup, "", ""),
		alice("stats-exercise-curl", curl, "", ""),
		alice("stats-exercise-plank", plank, "", ""),
		{"stats-exercise-bob", wkBobNav, wkBob, wkBobSquat, "", ""},
	}
	for _, c := range cases {
		wkAssertGolden(t, c.golden, ExerciseStats(w.exerciseView(t, c)))
	}
}
