package repo

import (
	"context"
	"errors"
	"testing"
)

func newExerciseFixture(t *testing.T, userIDs ...string) (*Exercises, context.Context) {
	t.Helper()
	pool := setupTestDB(t)
	for _, id := range userIDs {
		createTestUser(t, pool, id)
	}
	return NewExercises(pool), context.Background()
}

func mustCreateExercise(t *testing.T, repo *Exercises, name, category, userID string) string {
	t.Helper()
	exercise, err := repo.Create(context.Background(), NewExercise{Name: name, Category: category, UserID: userID})
	if err != nil {
		t.Fatal(err)
	}
	return exercise.ID
}

func TestExercisesCreate(t *testing.T) {
	repo, ctx := newExerciseFixture(t, "user1")
	exercise, err := repo.Create(ctx, NewExercise{Name: "Bench Press", Category: "chest", UserID: "user1"})
	if err != nil {
		t.Fatal(err)
	}
	if exercise.Name != "Bench Press" || exercise.Category != "chest" || exercise.UserID != "user1" || exercise.ID == "" {
		t.Errorf("unexpected exercise %+v", exercise)
	}
}

func TestExercisesFindByID(t *testing.T) {
	repo, ctx := newExerciseFixture(t, "user1")
	id := mustCreateExercise(t, repo, "Bench Press", "chest", "user1")

	found, err := repo.FindByID(ctx, id)
	if err != nil || found.ID != id || found.Name != "Bench Press" {
		t.Fatalf("FindByID = %+v, %v", found, err)
	}
	if _, err := repo.FindByID(ctx, "nonexistent"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("missing: err = %v, want ErrNotFound", err)
	}
}

func TestExercisesWithoutAnOwnerAreNotFound(t *testing.T) {
	pool := setupTestDB(t)
	// Only a database created before 0.4 allows the NULL.
	mustExec(t, pool, `DROP TABLE exercises;
		CREATE TABLE exercises (id TEXT PRIMARY KEY, name TEXT, category TEXT, user_id TEXT);
		INSERT INTO exercises VALUES ('ex-squat', 'Squat', 'legs', NULL)`)

	if _, err := NewExercises(pool).FindByID(context.Background(), "ex-squat"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("err = %v, want ErrNotFound", err)
	}
}

func TestExercisesFindOwnedHidesOtherUsersExercises(t *testing.T) {
	repo, ctx := newExerciseFixture(t, "user1", "user2")
	id := mustCreateExercise(t, repo, "Bench Press", "chest", "user1")

	if _, err := repo.FindOwned(ctx, id, "user1"); err != nil {
		t.Fatalf("owner: %v", err)
	}
	if _, err := repo.FindOwned(ctx, id, "user2"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("other user: err = %v, want ErrNotFound", err)
	}
	if _, err := repo.FindOwned(ctx, "nonexistent", "user1"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("missing: err = %v, want ErrNotFound", err)
	}
}

func TestExercisesFindAvailableForUser(t *testing.T) {
	repo, ctx := newExerciseFixture(t, "user1", "user2")
	mustCreateExercise(t, repo, "Squat", "legs", "user1")
	mustCreateExercise(t, repo, "Bench Press", "chest", "user1")
	mustCreateExercise(t, repo, "Deadlift", "back", "user2")

	user1, err := repo.FindAvailableForUser(ctx, "user1")
	if err != nil {
		t.Fatal(err)
	}
	user2, _ := repo.FindAvailableForUser(ctx, "user2")
	if len(user1) != 2 || len(user2) != 1 {
		t.Fatalf("got %d and %d exercises, want 2 and 1", len(user1), len(user2))
	}
	if user1[0].Category != "chest" || user1[1].Category != "legs" {
		t.Errorf("order = %s, %s; want category order", user1[0].Category, user1[1].Category)
	}
}

func TestExercisesUpdate(t *testing.T) {
	repo, ctx := newExerciseFixture(t, "user1")
	id := mustCreateExercise(t, repo, "Bench Press", "chest", "user1")

	updated, err := repo.Update(ctx, ExerciseEdit{ID: id, UserID: "user1", Name: "Incline Bench", Category: "chest"})
	if err != nil || !updated {
		t.Fatalf("Update = %v, %v", updated, err)
	}
	found, _ := repo.FindByID(ctx, id)
	if found.Name != "Incline Bench" {
		t.Errorf("name = %q", found.Name)
	}
}

func TestExercisesUpdateWrongUser(t *testing.T) {
	repo, ctx := newExerciseFixture(t, "user1", "user2")
	id := mustCreateExercise(t, repo, "Bench Press", "chest", "user1")

	updated, err := repo.Update(ctx, ExerciseEdit{ID: id, UserID: "user2", Name: "Hacked", Category: "chest"})
	if err != nil || updated {
		t.Fatalf("Update = %v, %v; want false, nil", updated, err)
	}
	found, _ := repo.FindByID(ctx, id)
	if found.Name != "Bench Press" {
		t.Errorf("name changed to %q", found.Name)
	}
}

func TestExercisesDelete(t *testing.T) {
	repo, ctx := newExerciseFixture(t, "user1")
	id := mustCreateExercise(t, repo, "Bench Press", "chest", "user1")

	deleted, err := repo.Delete(ctx, id, "user1")
	if err != nil || !deleted {
		t.Fatalf("Delete = %v, %v", deleted, err)
	}
	if _, err := repo.FindByID(ctx, id); !errors.Is(err, ErrNotFound) {
		t.Errorf("exercise still found: %v", err)
	}
}

func TestExercisesDeleteWrongUser(t *testing.T) {
	repo, ctx := newExerciseFixture(t, "user1", "user2")
	id := mustCreateExercise(t, repo, "Bench Press", "chest", "user1")

	deleted, err := repo.Delete(ctx, id, "user2")
	if err != nil || deleted {
		t.Fatalf("Delete = %v, %v; want false, nil", deleted, err)
	}
	if _, err := repo.FindByID(ctx, id); err != nil {
		t.Errorf("exercise gone: %v", err)
	}
}

func TestExercisesDeleteReferencedByLogsFailsWithErrExerciseInUse(t *testing.T) {
	pool := setupTestDB(t)
	createTestUser(t, pool, "user1")
	createTestExercise(t, pool, "ex1", "user1")
	workouts := NewWorkouts(pool)
	ctx := context.Background()
	session, err := workouts.CreateSession(ctx, "user1", day(2024, 1, 15), nil)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := workouts.CreateLog(ctx, NewLog{SessionID: session.ID, ExerciseID: "ex1", SetNumber: 1, Reps: 5, Weight: 100}); err != nil {
		t.Fatal(err)
	}

	deleted, err := NewExercises(pool).Delete(ctx, "ex1", "user1")
	if !errors.Is(err, ErrExerciseInUse) || deleted {
		t.Fatalf("Delete = %v, %v; want false, ErrExerciseInUse", deleted, err)
	}
}
