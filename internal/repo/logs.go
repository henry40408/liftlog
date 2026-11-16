package repo

import (
	"context"
	"database/sql"
	"errors"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
)

type NewLog struct {
	SessionID  string
	ExerciseID string
	SetNumber  int
	Reps       int
	Weight     float64
	RPE        *int
}

// LogEdit updates a log of the session SessionID.
type LogEdit struct {
	ID        string
	SessionID string
	Reps      int
	Weight    float64
	RPE       *int
}

func (r *Workouts) CreateLog(ctx context.Context, input NewLog) (models.WorkoutLog, error) {
	log := models.WorkoutLog{
		ID:         db.NewID(),
		SessionID:  input.SessionID,
		ExerciseID: input.ExerciseID,
		SetNumber:  input.SetNumber,
		Reps:       input.Reps,
		Weight:     input.Weight,
		RPE:        input.RPE,
		CreatedAt:  time.Now().UTC(),
	}
	_, err := r.db.ExecContext(ctx,
		"INSERT INTO workout_logs (id, session_id, exercise_id, set_number, reps, weight, rpe, created_at) "+
			"VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
		log.ID, log.SessionID, log.ExerciseID, log.SetNumber, log.Reps, log.Weight,
		nullableInt(log.RPE), db.FormatTime(log.CreatedAt))
	return log, err
}

// FindLogByID returns ErrNotFound when there is no such log.
func (r *Workouts) FindLogByID(ctx context.Context, id string) (models.WorkoutLog, error) {
	var (
		log       models.WorkoutLog
		rpe       sql.NullInt64
		createdAt string
	)
	err := r.db.QueryRowContext(ctx,
		"SELECT id, session_id, exercise_id, set_number, reps, weight, rpe, created_at FROM workout_logs WHERE id = ?", id,
	).Scan(&log.ID, &log.SessionID, &log.ExerciseID, &log.SetNumber, &log.Reps, &log.Weight, &rpe, &createdAt)
	if errors.Is(err, sql.ErrNoRows) {
		return models.WorkoutLog{}, ErrNotFound
	}
	if err != nil {
		return models.WorkoutLog{}, err
	}
	log.RPE = intPtr(rpe)
	log.CreatedAt, err = parseTime("workout_logs.created_at", createdAt)
	return log, err
}

// DeleteLog reports false when the log is not part of sessionID.
func (r *Workouts) DeleteLog(ctx context.Context, id, sessionID string) (bool, error) {
	return execChanged(ctx, r.db, "DELETE FROM workout_logs WHERE id = ? AND session_id = ?", id, sessionID)
}

// UpdateLog reports false when the log is not part of edit.SessionID.
func (r *Workouts) UpdateLog(ctx context.Context, edit LogEdit) (bool, error) {
	return execChanged(ctx, r.db,
		"UPDATE workout_logs SET reps = ?, weight = ?, rpe = ? WHERE id = ? AND session_id = ?",
		edit.Reps, edit.Weight, nullableInt(edit.RPE), edit.ID, edit.SessionID)
}

// NextSetNumber is one past the highest set of the exercise in the session.
func (r *Workouts) NextSetNumber(ctx context.Context, sessionID, exerciseID string) (int, error) {
	var highest sql.NullInt64
	err := r.db.QueryRowContext(ctx,
		"SELECT MAX(set_number) FROM workout_logs WHERE session_id = ? AND exercise_id = ?",
		sessionID, exerciseID).Scan(&highest)
	if err != nil {
		return 0, err
	}
	return int(highest.Int64) + 1, nil
}

// FindLogsBySessionWithPR lists a session's logs, newest first, with IsPR and
// IsRecentPR computed on the fly; the recent window starts at since.
func (r *Workouts) FindLogsBySessionWithPR(ctx context.Context, sessionID, userID string, since time.Time) ([]models.WorkoutLogWithExercise, error) {
	rows, err := r.db.QueryContext(ctx,
		`SELECT wl.id, wl.session_id, wl.exercise_id, e.name as exercise_name,
                wl.set_number, wl.reps, wl.weight, wl.rpe,
                CASE WHEN wl.weight = (
                    SELECT MAX(wl2.weight) FROM workout_logs wl2
                    JOIN workout_sessions ws2 ON wl2.session_id = ws2.id
                    WHERE ws2.user_id = ?1 AND wl2.exercise_id = wl.exercise_id
                ) THEN 1 ELSE 0 END as is_pr,
                CASE WHEN datetime(wl.created_at) >= datetime(?3) AND wl.weight = (
                    SELECT MAX(wl3.weight) FROM workout_logs wl3
                    JOIN workout_sessions ws3 ON wl3.session_id = ws3.id
                    WHERE ws3.user_id = ?1 AND wl3.exercise_id = wl.exercise_id
                      AND datetime(wl3.created_at) >= datetime(?3)
                ) THEN 1 ELSE 0 END as is_recent_pr
         FROM workout_logs wl
         JOIN exercises e ON wl.exercise_id = e.id
         WHERE wl.session_id = ?2
         ORDER BY wl.created_at DESC, wl.set_number`,
		userID, sessionID, db.FormatTime(since))
	if err != nil {
		return nil, err
	}
	return collect(rows, scanLogWithExercise)
}

// FindLogsBySessionForShare is FindLogsBySessionWithPR without the PR
// calculation: the public share page shows no badges.
func (r *Workouts) FindLogsBySessionForShare(ctx context.Context, sessionID string) ([]models.WorkoutLogWithExercise, error) {
	rows, err := r.db.QueryContext(ctx,
		`SELECT wl.id, wl.session_id, wl.exercise_id, e.name as exercise_name,
                wl.set_number, wl.reps, wl.weight, wl.rpe,
                0 as is_pr, 0 as is_recent_pr
         FROM workout_logs wl
         JOIN exercises e ON wl.exercise_id = e.id
         WHERE wl.session_id = ?
         ORDER BY wl.created_at DESC, wl.set_number`,
		sessionID)
	if err != nil {
		return nil, err
	}
	return collect(rows, scanLogWithExercise)
}

func scanLogWithExercise(row scanner) (models.WorkoutLogWithExercise, error) {
	var (
		log              models.WorkoutLogWithExercise
		rpe              sql.NullInt64
		isPR, isRecentPR int
	)
	err := row.Scan(&log.ID, &log.SessionID, &log.ExerciseID, &log.ExerciseName,
		&log.SetNumber, &log.Reps, &log.Weight, &rpe, &isPR, &isRecentPR)
	if err != nil {
		return models.WorkoutLogWithExercise{}, err
	}
	log.RPE = intPtr(rpe)
	log.IsPR = isPR != 0
	log.IsRecentPR = isRecentPR != 0
	return log, err
}
