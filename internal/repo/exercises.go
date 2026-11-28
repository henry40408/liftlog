package repo

import (
	"context"
	"database/sql"
	"errors"

	"modernc.org/sqlite"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
)

// sqliteConstraintTrigger is what SQLite reports for an immediate foreign key
// action such as ON DELETE RESTRICT (SQLITE_CONSTRAINT_FOREIGNKEY is only for
// deferred ones).
const sqliteConstraintTrigger = 1811

type Exercises struct {
	db *sql.DB
}

func NewExercises(pool *sql.DB) *Exercises {
	return &Exercises{db: pool}
}

type NewExercise struct {
	Name     string
	Category string
	UserID   string
}

// ExerciseEdit updates an exercise owned by UserID.
type ExerciseEdit struct {
	ID       string
	UserID   string
	Name     string
	Category string
}

const exerciseColumns = "id, name, category, user_id"

func scanExercise(row scanner) (models.Exercise, error) {
	var exercise models.Exercise
	err := row.Scan(&exercise.ID, &exercise.Name, &exercise.Category, &exercise.UserID)
	return exercise, err
}

// FindByID returns ErrNotFound when there is no such exercise. Databases created
// before 0.4 keep default exercises without an owner, which count as missing.
func (r *Exercises) FindByID(ctx context.Context, id string) (models.Exercise, error) {
	row := r.db.QueryRowContext(ctx,
		"SELECT "+exerciseColumns+" FROM exercises WHERE id = ? AND user_id IS NOT NULL", id)
	exercise, err := scanExercise(row)
	if errors.Is(err, sql.ErrNoRows) {
		return models.Exercise{}, ErrNotFound
	}
	return exercise, err
}

// FindOwned treats another user's exercise as ErrNotFound, like a missing
// one, so its existence is not disclosed.
func (r *Exercises) FindOwned(ctx context.Context, id, userID string) (models.Exercise, error) {
	exercise, err := r.FindByID(ctx, id)
	if err != nil {
		return models.Exercise{}, err
	}
	if exercise.UserID != userID {
		return models.Exercise{}, ErrNotFound
	}
	return exercise, nil
}

func (r *Exercises) FindAvailableForUser(ctx context.Context, userID string) ([]models.Exercise, error) {
	rows, err := r.db.QueryContext(ctx,
		"SELECT "+exerciseColumns+" FROM exercises WHERE user_id = ? ORDER BY category, name", userID)
	if err != nil {
		return nil, err
	}
	return collect(rows, scanExercise)
}

func (r *Exercises) Create(ctx context.Context, input NewExercise) (models.Exercise, error) {
	exercise := models.Exercise{
		ID:       db.NewID(),
		Name:     input.Name,
		Category: input.Category,
		UserID:   input.UserID,
	}
	_, err := r.db.ExecContext(ctx,
		"INSERT INTO exercises (id, name, category, user_id) VALUES (?, ?, ?, ?)",
		exercise.ID, exercise.Name, exercise.Category, exercise.UserID)
	return exercise, err
}

// Update reports false when the exercise is missing or not owned by UserID.
func (r *Exercises) Update(ctx context.Context, edit ExerciseEdit) (bool, error) {
	return execChanged(ctx, r.db,
		"UPDATE exercises SET name = ?, category = ? WHERE id = ? AND user_id = ?",
		edit.Name, edit.Category, edit.ID, edit.UserID)
}

// Delete reports false when the exercise is missing or not owned by userID,
// and ErrExerciseInUse while workout logs still reference it.
func (r *Exercises) Delete(ctx context.Context, id, userID string) (bool, error) {
	deleted, err := execChanged(ctx, r.db, "DELETE FROM exercises WHERE id = ? AND user_id = ?", id, userID)
	if isConstraintTrigger(err) {
		return false, ErrExerciseInUse
	}
	return deleted, err
}

func isConstraintTrigger(err error) bool {
	var sqliteErr *sqlite.Error
	return errors.As(err, &sqliteErr) && sqliteErr.Code() == sqliteConstraintTrigger
}
