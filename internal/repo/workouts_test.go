package repo

import (
	"context"
	"database/sql"
	"errors"
	"math"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
)

type workoutFixture struct {
	pool *sql.DB
	repo *Workouts
	ctx  context.Context
}

func newWorkoutFixture(t *testing.T, userIDs ...string) workoutFixture {
	t.Helper()
	pool := setupTestDB(t)
	for _, id := range userIDs {
		createTestUser(t, pool, id)
	}
	return workoutFixture{pool: pool, repo: NewWorkouts(pool), ctx: context.Background()}
}

func (f workoutFixture) session(t *testing.T, userID string, date time.Time, notes *string) models.WorkoutSession {
	t.Helper()
	session, err := f.repo.CreateSession(f.ctx, userID, date, notes)
	if err != nil {
		t.Fatal(err)
	}
	return session
}

func (f workoutFixture) log(t *testing.T, sessionID, exerciseID string, set, reps int, weight float64, rpe *int) models.WorkoutLog {
	t.Helper()
	log, err := f.repo.CreateLog(f.ctx, NewLog{
		SessionID: sessionID, ExerciseID: exerciseID, SetNumber: set, Reps: reps, Weight: weight, RPE: rpe,
	})
	if err != nil {
		t.Fatal(err)
	}
	return log
}

// backdateLog moves a log out of the recent PR window.
func (f workoutFixture) backdateLog(t *testing.T, logID string, at time.Time) {
	t.Helper()
	result, err := f.pool.Exec("UPDATE workout_logs SET created_at = ? WHERE id = ?", db.FormatTime(at), logID)
	if err != nil {
		t.Fatal(err)
	}
	if n, _ := result.RowsAffected(); n != 1 {
		t.Fatalf("backdate targeted %d rows", n)
	}
}

func recentWindowStart() time.Time {
	return models.RecentPRWindowStart(time.Now())
}

func TestWorkoutsCreateSession(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	session := f.session(t, "user1", day(2024, 1, 15), ptr("Leg day"))

	if session.UserID != "user1" || !session.Date.Equal(day(2024, 1, 15)) || session.ID == "" {
		t.Errorf("unexpected session %+v", session)
	}
	if session.Notes == nil || *session.Notes != "Leg day" {
		t.Errorf("notes = %v", session.Notes)
	}
}

func TestWorkoutsFindSessionByID(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	created := f.session(t, "user1", day(2024, 1, 15), nil)

	found, err := f.repo.FindSessionByID(f.ctx, created.ID)
	if err != nil || found.ID != created.ID {
		t.Fatalf("FindSessionByID = %+v, %v", found, err)
	}
	if found.Notes != nil || found.ShareToken != nil || found.ShareExpiresAt != nil {
		t.Errorf("nullable columns not nil: %+v", found)
	}
	if !found.Date.Equal(day(2024, 1, 15)) || !found.CreatedAt.Equal(created.CreatedAt) {
		t.Errorf("dates differ: %+v vs %+v", found, created)
	}
	if _, err := f.repo.FindSessionByID(f.ctx, "nonexistent"); !errors.Is(err, ErrNotFound) {
		t.Errorf("missing: err = %v, want ErrNotFound", err)
	}
}

func TestWorkoutsFindOwnedSessionHidesOtherUsersSessions(t *testing.T) {
	f := newWorkoutFixture(t, "user1", "user2")
	session := f.session(t, "user1", day(2024, 1, 15), nil)

	if _, err := f.repo.FindOwnedSession(f.ctx, session.ID, "user1"); err != nil {
		t.Fatalf("owner: %v", err)
	}
	if _, err := f.repo.FindOwnedSession(f.ctx, session.ID, "user2"); !errors.Is(err, ErrNotFound) {
		t.Errorf("other user: err = %v, want ErrNotFound", err)
	}
}

func TestWorkoutsFindSessionsByUserOrderedByDateDesc(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	for _, d := range []int{10, 15, 12} {
		f.session(t, "user1", day(2024, 1, d), nil)
	}

	sessions, err := f.repo.FindSessionsByUserPaginated(f.ctx, "user1", 10, 0)
	if err != nil {
		t.Fatal(err)
	}
	if len(sessions) != 3 {
		t.Fatalf("got %d sessions", len(sessions))
	}
	for i, want := range []int{15, 12, 10} {
		if !sessions[i].Date.Equal(day(2024, 1, want)) {
			t.Errorf("sessions[%d].Date = %v, want Jan %d", i, sessions[i].Date, want)
		}
	}
}

func TestWorkoutsFindSessionsByUserPaginated(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	for d := 1; d <= 5; d++ {
		f.session(t, "user1", day(2024, 1, d), nil)
	}

	for offset, want := range map[int]int{0: 2, 2: 2, 4: 1} {
		page, err := f.repo.FindSessionsByUserPaginated(f.ctx, "user1", 2, offset)
		if err != nil || len(page) != want {
			t.Errorf("offset %d: got %d sessions, %v; want %d", offset, len(page), err, want)
		}
	}
}

func TestWorkoutsCountSessionsByUser(t *testing.T) {
	f := newWorkoutFixture(t, "user1", "user2")
	f.session(t, "user1", day(2024, 1, 15), nil)
	f.session(t, "user1", day(2024, 1, 15), nil)
	f.session(t, "user2", day(2024, 1, 15), nil)

	count, err := f.repo.CountSessionsByUser(f.ctx, "user1")
	if err != nil || count != 2 {
		t.Fatalf("count = %d, %v; want 2", count, err)
	}
}

func TestWorkoutsUpdateSession(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)

	updated, err := f.repo.UpdateSession(f.ctx, SessionEdit{
		ID: session.ID, UserID: "user1", Date: ptr(day(2024, 1, 20)), Notes: ptr("Updated notes"),
	})
	if err != nil || !updated {
		t.Fatalf("UpdateSession = %v, %v", updated, err)
	}
	found, _ := f.repo.FindSessionByID(f.ctx, session.ID)
	if !found.Date.Equal(day(2024, 1, 20)) || found.Notes == nil || *found.Notes != "Updated notes" {
		t.Errorf("found %+v", found)
	}
}

func TestWorkoutsUpdateSessionWithoutDateKeepsDateAndClearsNotes(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	session := f.session(t, "user1", day(2024, 1, 15), ptr("old"))

	updated, err := f.repo.UpdateSession(f.ctx, SessionEdit{ID: session.ID, UserID: "user1"})
	if err != nil || !updated {
		t.Fatalf("UpdateSession = %v, %v", updated, err)
	}
	found, _ := f.repo.FindSessionByID(f.ctx, session.ID)
	if !found.Date.Equal(day(2024, 1, 15)) || found.Notes != nil {
		t.Errorf("found %+v", found)
	}
}

func TestWorkoutsUpdateSessionWrongUser(t *testing.T) {
	f := newWorkoutFixture(t, "user1", "user2")
	session := f.session(t, "user1", day(2024, 1, 15), nil)

	updated, err := f.repo.UpdateSession(f.ctx, SessionEdit{ID: session.ID, UserID: "user2", Notes: ptr("x")})
	if err != nil || updated {
		t.Fatalf("UpdateSession = %v, %v; want false, nil", updated, err)
	}
}

func TestWorkoutsDeleteSession(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)

	deleted, err := f.repo.DeleteSession(f.ctx, session.ID, "user1")
	if err != nil || !deleted {
		t.Fatalf("DeleteSession = %v, %v", deleted, err)
	}
	if _, err := f.repo.FindSessionByID(f.ctx, session.ID); !errors.Is(err, ErrNotFound) {
		t.Errorf("session still found: %v", err)
	}
}

func TestWorkoutsDeleteSessionWrongUser(t *testing.T) {
	f := newWorkoutFixture(t, "user1", "user2")
	session := f.session(t, "user1", day(2024, 1, 15), nil)

	deleted, err := f.repo.DeleteSession(f.ctx, session.ID, "user2")
	if err != nil || deleted {
		t.Fatalf("DeleteSession = %v, %v; want false, nil", deleted, err)
	}
}

func TestWorkoutsCreateLog(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)

	log := f.log(t, session.ID, "ex-bench-press", 1, 10, 100, ptr(8))
	if log.SessionID != session.ID || log.ExerciseID != "ex-bench-press" || log.SetNumber != 1 ||
		log.Reps != 10 || log.Weight != 100 || log.RPE == nil || *log.RPE != 8 {
		t.Errorf("unexpected log %+v", log)
	}

	found, err := f.repo.FindLogByID(f.ctx, log.ID)
	if err != nil {
		t.Fatal(err)
	}
	if found.Weight != 100 || found.RPE == nil || *found.RPE != 8 || !found.CreatedAt.Equal(log.CreatedAt) {
		t.Errorf("found %+v, want %+v", found, log)
	}
}

func TestWorkoutsFindLogByIDNotFound(t *testing.T) {
	f := newWorkoutFixture(t)
	if _, err := f.repo.FindLogByID(f.ctx, "nonexistent"); !errors.Is(err, ErrNotFound) {
		t.Errorf("err = %v, want ErrNotFound", err)
	}
}

func TestWorkoutsFindLogsBySessionWithPR(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	createTestExercise(t, f.pool, "ex-squat", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)
	f.log(t, session.ID, "ex-bench-press", 2, 8, 105, nil)
	f.log(t, session.ID, "ex-squat", 1, 5, 120, nil)

	logs, err := f.repo.FindLogsBySessionWithPR(f.ctx, session.ID, "user1", recentWindowStart())
	if err != nil {
		t.Fatal(err)
	}
	if len(logs) != 3 {
		t.Fatalf("got %d logs", len(logs))
	}
	// Newest first: 120 squat (PR), 105 bench (PR), 100 bench.
	wantPR := []bool{true, true, false}
	for i, want := range wantPR {
		if logs[i].IsPR != want || logs[i].IsRecentPR != want {
			t.Errorf("logs[%d] IsPR=%v IsRecentPR=%v, want both %v", i, logs[i].IsPR, logs[i].IsRecentPR, want)
		}
	}
	if logs[0].ExerciseName != "Test Exercise" {
		t.Errorf("exercise name = %q", logs[0].ExerciseName)
	}
}

func TestWorkoutsFindLogsMarksRecentPRBelowTheAllTimeBest(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	oldSession := f.session(t, "user1", day(2024, 1, 15), nil)
	recentSession := f.session(t, "user1", day(2024, 1, 15), nil)

	// An all-time best set from well outside the window.
	oldLog := f.log(t, oldSession.ID, "ex-bench-press", 1, 3, 140, nil)
	f.backdateLog(t, oldLog.ID, time.Now().Add(-90*24*time.Hour))
	// A lighter set today: best within the window only.
	f.log(t, recentSession.ID, "ex-bench-press", 1, 8, 110, nil)

	recent, err := f.repo.FindLogsBySessionWithPR(f.ctx, recentSession.ID, "user1", recentWindowStart())
	if err != nil || len(recent) != 1 {
		t.Fatalf("recent logs = %d, %v", len(recent), err)
	}
	if recent[0].IsPR || !recent[0].IsRecentPR {
		t.Errorf("recent set IsPR=%v IsRecentPR=%v, want false/true", recent[0].IsPR, recent[0].IsRecentPR)
	}

	old, err := f.repo.FindLogsBySessionWithPR(f.ctx, oldSession.ID, "user1", recentWindowStart())
	if err != nil || len(old) != 1 {
		t.Fatalf("old logs = %d, %v", len(old), err)
	}
	if !old[0].IsPR || old[0].IsRecentPR {
		t.Errorf("old set IsPR=%v IsRecentPR=%v, want true/false", old[0].IsPR, old[0].IsRecentPR)
	}
}

func TestWorkoutsDeleteLog(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	log := f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)

	deleted, err := f.repo.DeleteLog(f.ctx, log.ID, session.ID)
	if err != nil || !deleted {
		t.Fatalf("DeleteLog = %v, %v", deleted, err)
	}
	if _, err := f.repo.FindLogByID(f.ctx, log.ID); !errors.Is(err, ErrNotFound) {
		t.Errorf("log still found: %v", err)
	}
}

func TestWorkoutsDeleteLogWrongSession(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	log := f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)

	deleted, err := f.repo.DeleteLog(f.ctx, log.ID, "wrong-session")
	if err != nil || deleted {
		t.Fatalf("DeleteLog = %v, %v; want false, nil", deleted, err)
	}
}

func TestWorkoutsUpdateLog(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	log := f.log(t, session.ID, "ex-bench-press", 1, 10, 100, ptr(7))

	updated, err := f.repo.UpdateLog(f.ctx, LogEdit{ID: log.ID, SessionID: session.ID, Reps: 12, Weight: 110, RPE: ptr(8)})
	if err != nil || !updated {
		t.Fatalf("UpdateLog = %v, %v", updated, err)
	}
	found, _ := f.repo.FindLogByID(f.ctx, log.ID)
	if found.Reps != 12 || found.Weight != 110 || found.RPE == nil || *found.RPE != 8 {
		t.Errorf("found %+v", found)
	}
}

func TestWorkoutsUpdateLogClearsRPE(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	log := f.log(t, session.ID, "ex-bench-press", 1, 10, 100, ptr(7))

	if _, err := f.repo.UpdateLog(f.ctx, LogEdit{ID: log.ID, SessionID: session.ID, Reps: 10, Weight: 100}); err != nil {
		t.Fatal(err)
	}
	if found, _ := f.repo.FindLogByID(f.ctx, log.ID); found.RPE != nil {
		t.Errorf("rpe = %d, want nil", *found.RPE)
	}
}

func TestWorkoutsUpdateLogWrongSession(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	log := f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)

	updated, err := f.repo.UpdateLog(f.ctx, LogEdit{ID: log.ID, SessionID: "wrong-session", Reps: 12, Weight: 110, RPE: ptr(8)})
	if err != nil || updated {
		t.Fatalf("UpdateLog = %v, %v; want false, nil", updated, err)
	}
	found, _ := f.repo.FindLogByID(f.ctx, log.ID)
	if found.Reps != 10 || found.Weight != 100 {
		t.Errorf("log changed: %+v", found)
	}
}

func TestWorkoutsNextSetNumber(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)

	next, err := f.repo.NextSetNumber(f.ctx, session.ID, "ex-bench-press")
	if err != nil || next != 1 {
		t.Fatalf("empty: next = %d, %v; want 1", next, err)
	}
	f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)
	next, err = f.repo.NextSetNumber(f.ctx, session.ID, "ex-bench-press")
	if err != nil || next != 2 {
		t.Fatalf("after one set: next = %d, %v; want 2", next, err)
	}
}

func TestWorkoutsPRSummariesByUser(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	createTestExercise(t, f.pool, "ex-squat", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)
	f.log(t, session.ID, "ex-bench-press", 2, 8, 110, nil)
	f.log(t, session.ID, "ex-squat", 1, 5, 150, nil)

	prs, err := f.repo.PRSummariesByUser(f.ctx, "user1", time.Now().Add(-30*24*time.Hour))
	if err != nil || len(prs) != 2 {
		t.Fatalf("got %d PRs, %v", len(prs), err)
	}
	byID := map[string]models.PersonalRecordSummary{}
	for _, pr := range prs {
		byID[pr.ExerciseID] = pr
	}
	bench, squat := byID["ex-bench-press"], byID["ex-squat"]
	if bench.AllTimeValue != 110 || squat.AllTimeValue != 150 {
		t.Errorf("all-time = %v, %v", bench.AllTimeValue, squat.AllTimeValue)
	}
	if bench.RecentValue == nil || *bench.RecentValue != 110 || !bench.RecentIsAllTime() {
		t.Errorf("bench recent = %v", bench.RecentValue)
	}
	// The newest PR comes first.
	if prs[0].ExerciseID != "ex-squat" {
		t.Errorf("first PR = %s, want ex-squat", prs[0].ExerciseID)
	}
}

func TestWorkoutsPRSummariesRecentWindowExcludesOlderLogs(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	createTestExercise(t, f.pool, "ex-squat", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)

	// Bench: the all-time best is old, a lighter set is inside the window.
	oldBench := f.log(t, session.ID, "ex-bench-press", 1, 3, 140, nil)
	f.backdateLog(t, oldBench.ID, time.Now().Add(-90*24*time.Hour))
	f.log(t, session.ID, "ex-bench-press", 2, 8, 110, nil)
	// Squat: only trained outside the window.
	oldSquat := f.log(t, session.ID, "ex-squat", 1, 5, 150, nil)
	f.backdateLog(t, oldSquat.ID, time.Now().Add(-45*24*time.Hour))

	prs, err := f.repo.PRSummariesByUser(f.ctx, "user1", time.Now().Add(-30*24*time.Hour))
	if err != nil {
		t.Fatal(err)
	}
	byID := map[string]models.PersonalRecordSummary{}
	for _, pr := range prs {
		byID[pr.ExerciseID] = pr
	}

	bench := byID["ex-bench-press"]
	if bench.AllTimeValue != 140 || bench.RecentValue == nil || *bench.RecentValue != 110 ||
		bench.RecentIsAllTime() || bench.RecentAchievedAt == nil {
		t.Errorf("bench = %+v", bench)
	}
	squat := byID["ex-squat"]
	if squat.AllTimeValue != 150 || squat.RecentValue != nil || squat.RecentAchievedAt != nil || squat.RecentIsAllTime() {
		t.Errorf("squat = %+v", squat)
	}
}

func TestWorkoutsPRSummariesByUserEmpty(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	prs, err := f.repo.PRSummariesByUser(f.ctx, "user1", time.Now().Add(-30*24*time.Hour))
	if err != nil || len(prs) != 0 {
		t.Fatalf("got %d PRs, %v; want none", len(prs), err)
	}
}

func TestWorkoutsMaxWeightForExercise(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)
	f.log(t, session.ID, "ex-bench-press", 2, 8, 110, nil)
	f.log(t, session.ID, "ex-bench-press", 3, 5, 105, nil)

	pr, err := f.repo.MaxWeightForExercise(f.ctx, "user1", "ex-bench-press")
	if err != nil || pr.Value != 110 || pr.ExerciseName != "Test Exercise" || pr.AchievedAt.IsZero() {
		t.Fatalf("pr = %+v, %v", pr, err)
	}
	if _, err := f.repo.MaxWeightForExercise(f.ctx, "user1", "ex-unknown"); !errors.Is(err, ErrNotFound) {
		t.Errorf("unknown exercise: err = %v, want ErrNotFound", err)
	}
}

func TestWorkoutsDynamicPRUpdatesWhenHeavierSetAdded(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)

	logs, _ := f.repo.FindLogsBySessionWithPR(f.ctx, session.ID, "user1", recentWindowStart())
	if !logs[0].IsPR {
		t.Error("the only set must be a PR")
	}

	f.log(t, session.ID, "ex-bench-press", 2, 8, 110, nil)
	logs, _ = f.repo.FindLogsBySessionWithPR(f.ctx, session.ID, "user1", recentWindowStart())
	if !logs[0].IsPR || logs[1].IsPR {
		t.Errorf("IsPR = %v, %v; want newest (110) true, older (100) false", logs[0].IsPR, logs[1].IsPR)
	}
}

func TestWorkoutsDynamicPRUpdatesWhenPRDeleted(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)
	heavy := f.log(t, session.ID, "ex-bench-press", 2, 8, 110, nil)

	if _, err := f.repo.DeleteLog(f.ctx, heavy.ID, session.ID); err != nil {
		t.Fatal(err)
	}
	logs, _ := f.repo.FindLogsBySessionWithPR(f.ctx, session.ID, "user1", recentWindowStart())
	if len(logs) != 1 || !logs[0].IsPR {
		t.Errorf("logs = %+v; want the 100 set to be a PR again", logs)
	}
}

func TestWorkoutsCountSessionsSinceUsesCalendarDates(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	for _, d := range []int{2, 3, 9} {
		f.session(t, "user1", day(2026, 11, d), nil)
	}

	// The window start is inclusive and supplied by the caller.
	for since, want := range map[int]int{3: 2, 2: 3, 10: 0} {
		count, err := f.repo.CountSessionsSince(f.ctx, "user1", day(2026, 11, since))
		if err != nil || count != want {
			t.Errorf("since Nov %d: count = %d, %v; want %d", since, count, err, want)
		}
	}
}

func TestWorkoutsTotalVolumeSince(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex1", "user1")

	volume, err := f.repo.TotalVolumeSince(f.ctx, "user1", day(2026, 11, 3))
	if err != nil || volume != 0 {
		t.Fatalf("empty volume = %v, %v; want 0", volume, err)
	}

	early := f.session(t, "user1", day(2026, 11, 2), nil)
	late := f.session(t, "user1", day(2026, 11, 5), nil)
	f.log(t, early.ID, "ex1", 1, 10, 100, nil)
	f.log(t, late.ID, "ex1", 1, 5, 100, nil)
	f.log(t, late.ID, "ex1", 2, 5, 50, nil)

	volume, err = f.repo.TotalVolumeSince(f.ctx, "user1", day(2026, 11, 3))
	if err != nil || volume != 750 {
		t.Fatalf("volume = %v, %v; want 750", volume, err)
	}
}

func TestWorkoutsLastWeightPerExerciseByUser(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	createTestExercise(t, f.pool, "ex-squat", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	// Bench: the latest set (105) is lower than the max (110).
	f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)
	f.log(t, session.ID, "ex-bench-press", 2, 8, 110, nil)
	f.log(t, session.ID, "ex-bench-press", 3, 5, 105, ptr(8))
	// Squat: a single set with no RPE.
	f.log(t, session.ID, "ex-squat", 1, 5, 150, nil)

	entries, err := f.repo.LastWeightPerExerciseByUser(f.ctx, "user1")
	if err != nil || len(entries) != 2 {
		t.Fatalf("got %d entries, %v", len(entries), err)
	}
	byID := map[string]models.LastExerciseWeight{}
	for _, entry := range entries {
		byID[entry.ExerciseID] = entry
	}
	bench, squat := byID["ex-bench-press"], byID["ex-squat"]
	if bench.Weight != 105 || squat.Weight != 150 {
		t.Errorf("weights = %v, %v; want the last set, not the max", bench.Weight, squat.Weight)
	}
	if bench.RPE == nil || *bench.RPE != 8 || squat.RPE != nil {
		t.Errorf("rpe = %v, %v", bench.RPE, squat.RPE)
	}
	if bench.LoggedAt.IsZero() {
		t.Error("LoggedAt not parsed")
	}
}

func TestWorkoutsLastWeightPerExerciseByUserEmpty(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	entries, err := f.repo.LastWeightPerExerciseByUser(f.ctx, "user1")
	if err != nil || len(entries) != 0 {
		t.Fatalf("got %d entries, %v; want none", len(entries), err)
	}
}

func TestWorkoutsSessionMetricsEmpty(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	metrics, err := f.repo.SessionMetricsForExercise(f.ctx, "user1", "ex-bench-press")
	if err != nil || len(metrics) != 0 {
		t.Fatalf("got %d metrics, %v; want none", len(metrics), err)
	}
}

func near(a, b float64) bool { return math.Abs(a-b) < 1e-9 }

func TestWorkoutsSessionMetricsSingleSessionAggregates(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	// 100x10, 110x8, 105x5: top weight 110 with 8 reps; volume 1000+880+525.
	f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)
	f.log(t, session.ID, "ex-bench-press", 2, 8, 110, nil)
	f.log(t, session.ID, "ex-bench-press", 3, 5, 105, nil)

	metrics, err := f.repo.SessionMetricsForExercise(f.ctx, "user1", "ex-bench-press")
	if err != nil || len(metrics) != 1 {
		t.Fatalf("got %d metrics, %v", len(metrics), err)
	}
	m := metrics[0]
	if !m.Date.Equal(day(2024, 1, 15)) || !near(m.TopWeight, 110) || m.TopReps != 8 || !near(m.Volume, 2405) {
		t.Errorf("metric = %+v", m)
	}
}

func TestWorkoutsSessionMetricsOrderedOldestFirst(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	// Inserted out of order: middle, oldest, newest.
	for _, spec := range []struct {
		date   time.Time
		weight float64
	}{{day(2024, 1, 12), 100}, {day(2024, 1, 10), 90}, {day(2024, 1, 20), 110}} {
		session := f.session(t, "user1", spec.date, nil)
		f.log(t, session.ID, "ex-bench-press", 1, 5, spec.weight, nil)
	}

	metrics, err := f.repo.SessionMetricsForExercise(f.ctx, "user1", "ex-bench-press")
	if err != nil || len(metrics) != 3 {
		t.Fatalf("got %d metrics, %v", len(metrics), err)
	}
	for i, want := range []time.Time{day(2024, 1, 10), day(2024, 1, 12), day(2024, 1, 20)} {
		if !metrics[i].Date.Equal(want) {
			t.Errorf("metrics[%d].Date = %v, want %v", i, metrics[i].Date, want)
		}
	}
}

func TestWorkoutsSessionMetricsExcludesOtherExercises(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	createTestExercise(t, f.pool, "ex-squat", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	f.log(t, session.ID, "ex-bench-press", 1, 10, 100, nil)
	f.log(t, session.ID, "ex-squat", 1, 5, 200, nil)

	metrics, err := f.repo.SessionMetricsForExercise(f.ctx, "user1", "ex-bench-press")
	if err != nil || len(metrics) != 1 {
		t.Fatalf("got %d metrics, %v", len(metrics), err)
	}
	if m := metrics[0]; !near(m.TopWeight, 100) || m.TopReps != 10 || !near(m.Volume, 1000) {
		t.Errorf("metric = %+v", m)
	}
}

func TestWorkoutsSessionMetricsUserIsolation(t *testing.T) {
	f := newWorkoutFixture(t, "user1", "user2")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	s1 := f.session(t, "user1", day(2024, 1, 15), nil)
	s2 := f.session(t, "user2", day(2024, 1, 15), nil)
	f.log(t, s1.ID, "ex-bench-press", 1, 5, 100, nil)
	f.log(t, s2.ID, "ex-bench-press", 1, 5, 200, nil)

	metrics, err := f.repo.SessionMetricsForExercise(f.ctx, "user1", "ex-bench-press")
	if err != nil || len(metrics) != 1 || !near(metrics[0].TopWeight, 100) {
		t.Fatalf("metrics = %+v, %v", metrics, err)
	}
}

func TestWorkoutsSessionMetricsTopSetTiePicksHigherReps(t *testing.T) {
	f := newWorkoutFixture(t, "user1")
	createTestExercise(t, f.pool, "ex-bench-press", "user1")
	session := f.session(t, "user1", day(2024, 1, 15), nil)
	// Same max weight, three rep counts: the tie-break must select 8.
	f.log(t, session.ID, "ex-bench-press", 1, 5, 100, nil)
	f.log(t, session.ID, "ex-bench-press", 2, 8, 100, nil)
	f.log(t, session.ID, "ex-bench-press", 3, 6, 100, nil)

	metrics, err := f.repo.SessionMetricsForExercise(f.ctx, "user1", "ex-bench-press")
	if err != nil || len(metrics) != 1 {
		t.Fatalf("metrics = %+v, %v", metrics, err)
	}
	if !near(metrics[0].TopWeight, 100) || metrics[0].TopReps != 8 {
		t.Errorf("metric = %+v", metrics[0])
	}
}

func TestWorkoutsExerciseHistoryWithPR(t *testing.T) {
	f := newWorkoutFixture(t, "user1", "user2")
	createTestExercise(t, f.pool, "ex1", "user1")
	older := f.session(t, "user1", day(2024, 1, 10), nil)
	newer := f.session(t, "user1", day(2024, 1, 20), nil)
	other := f.session(t, "user2", day(2024, 1, 25), nil)
	f.log(t, older.ID, "ex1", 1, 5, 100, nil)
	f.log(t, newer.ID, "ex1", 1, 5, 90, nil)
	f.log(t, newer.ID, "ex1", 2, 5, 80, nil)
	f.log(t, other.ID, "ex1", 1, 5, 500, nil)

	logs, err := f.repo.ExerciseHistoryWithPR(f.ctx, ExerciseHistoryQuery{
		UserID: "user1", ExerciseID: "ex1", Limit: 10, Since: recentWindowStart(),
	})
	if err != nil || len(logs) != 3 {
		t.Fatalf("got %d logs, %v; want the user's 3 sets", len(logs), err)
	}
	// Newest session first, sets in order; only the 100 set is a PR.
	wantWeights := []float64{90, 80, 100}
	for i, want := range wantWeights {
		if logs[i].Weight != want || logs[i].IsPR != (want == 100) {
			t.Errorf("logs[%d] = weight %v IsPR %v", i, logs[i].Weight, logs[i].IsPR)
		}
	}

	limited, err := f.repo.ExerciseHistoryWithPR(f.ctx, ExerciseHistoryQuery{
		UserID: "user1", ExerciseID: "ex1", Limit: 2, Since: recentWindowStart(),
	})
	if err != nil || len(limited) != 2 {
		t.Fatalf("limited: got %d logs, %v; want 2", len(limited), err)
	}
}
