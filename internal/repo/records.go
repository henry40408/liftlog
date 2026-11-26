package repo

import (
	"context"
	"database/sql"
	"errors"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
)

// ExerciseHistoryQuery selects the newest Limit sets of one exercise; Since
// starts the recent-PR window.
type ExerciseHistoryQuery struct {
	UserID     string
	ExerciseID string
	Limit      int
	Since      time.Time
}

// PRSummariesByUser returns one PR per exercise (max weight), plus the max
// since the given time for the "PR (1M)" column.
//
// created_at goes through datetime(): app-written rows carry +00:00 and
// CURRENT_TIMESTAMP rows do not, so raw strings misorder.
func (r *Workouts) PRSummariesByUser(ctx context.Context, userID string, since time.Time) ([]models.PersonalRecordSummary, error) {
	rows, err := r.db.QueryContext(ctx,
		`SELECT wl.exercise_id, e.name as exercise_name,
                MAX(wl.weight) as all_time_value,
                (SELECT wl3.created_at FROM workout_logs wl3
                 JOIN workout_sessions ws3 ON wl3.session_id = ws3.id
                 WHERE ws3.user_id = ?1 AND wl3.exercise_id = wl.exercise_id
                 ORDER BY wl3.weight DESC, wl3.created_at DESC LIMIT 1) as all_time_achieved_at,
                MAX(CASE WHEN datetime(wl.created_at) >= datetime(?2)
                         THEN wl.weight END) as recent_value,
                (SELECT wl4.created_at FROM workout_logs wl4
                 JOIN workout_sessions ws4 ON wl4.session_id = ws4.id
                 WHERE ws4.user_id = ?1 AND wl4.exercise_id = wl.exercise_id
                   AND datetime(wl4.created_at) >= datetime(?2)
                 ORDER BY wl4.weight DESC, wl4.created_at DESC LIMIT 1) as recent_achieved_at
         FROM workout_logs wl
         JOIN workout_sessions ws ON wl.session_id = ws.id
         JOIN exercises e ON wl.exercise_id = e.id
         WHERE ws.user_id = ?1
         GROUP BY wl.exercise_id
         ORDER BY all_time_achieved_at DESC`,
		userID, db.FormatTime(since))
	if err != nil {
		return nil, err
	}
	return collect(rows, scanPRSummary)
}

func scanPRSummary(row scanner) (models.PersonalRecordSummary, error) {
	var (
		pr          models.PersonalRecordSummary
		allTimeAt   string
		recentValue sql.NullFloat64
		recentAt    sql.NullString
	)
	err := row.Scan(&pr.ExerciseID, &pr.ExerciseName, &pr.AllTimeValue, &allTimeAt, &recentValue, &recentAt)
	if err != nil {
		return models.PersonalRecordSummary{}, err
	}
	if pr.AllTimeAchievedAt, err = parseTime("all_time_achieved_at", allTimeAt); err != nil {
		return models.PersonalRecordSummary{}, err
	}
	if recentValue.Valid {
		pr.RecentValue = &recentValue.Float64
	}
	pr.RecentAchievedAt, err = parseNullTime("recent_achieved_at", recentAt)
	return pr, err
}

// LastWeightPerExerciseByUser returns the weight and RPE of the latest set of
// each exercise, most recently trained first.
func (r *Workouts) LastWeightPerExerciseByUser(ctx context.Context, userID string) ([]models.LastExerciseWeight, error) {
	// SQLite bare-column rule: with a single MAX() aggregate, weight and rpe
	// come from the MAX(created_at) row, i.e. the latest set. A second
	// aggregate in this SELECT would break that.
	rows, err := r.db.QueryContext(ctx,
		`SELECT wl.exercise_id,
                wl.weight as weight,
                wl.rpe as rpe,
                MAX(wl.created_at) as logged_at
         FROM workout_logs wl
         JOIN workout_sessions ws ON wl.session_id = ws.id
         WHERE ws.user_id = ?
         GROUP BY wl.exercise_id
         ORDER BY logged_at DESC`, userID)
	if err != nil {
		return nil, err
	}
	return collect(rows, scanLastWeight)
}

func scanLastWeight(row scanner) (models.LastExerciseWeight, error) {
	var (
		entry    models.LastExerciseWeight
		rpe      sql.NullInt64
		loggedAt string
	)
	if err := row.Scan(&entry.ExerciseID, &entry.Weight, &rpe, &loggedAt); err != nil {
		return models.LastExerciseWeight{}, err
	}
	entry.RPE = intPtr(rpe)
	var err error
	entry.LoggedAt, err = parseTime("logged_at", loggedAt)
	return entry, err
}

// MaxWeightForExercise returns ErrNotFound when the user never logged the
// exercise.
func (r *Workouts) MaxWeightForExercise(ctx context.Context, userID, exerciseID string) (models.DynamicPR, error) {
	var (
		pr         models.DynamicPR
		achievedAt string
	)
	err := r.db.QueryRowContext(ctx,
		`SELECT wl.exercise_id, e.name as exercise_name,
                MAX(wl.weight) as value, wl.created_at as achieved_at
         FROM workout_logs wl
         JOIN workout_sessions ws ON wl.session_id = ws.id
         JOIN exercises e ON wl.exercise_id = e.id
         WHERE ws.user_id = ? AND wl.exercise_id = ?
         GROUP BY wl.exercise_id`, userID, exerciseID,
	).Scan(&pr.ExerciseID, &pr.ExerciseName, &pr.Value, &achievedAt)
	if errors.Is(err, sql.ErrNoRows) {
		return models.DynamicPR{}, ErrNotFound
	}
	if err != nil {
		return models.DynamicPR{}, err
	}
	pr.AchievedAt, err = parseTime("achieved_at", achievedAt)
	return pr, err
}

// SessionMetricsForExercise returns per-session aggregates for one exercise,
// oldest first: top set weight and reps (higher reps on ties) and volume. The
// volume is rounded as in TotalVolumeSince, so equal sessions compare equal and
// the chart does not mark float noise as a PR.
func (r *Workouts) SessionMetricsForExercise(ctx context.Context, userID, exerciseID string) ([]models.ExerciseSessionMetric, error) {
	rows, err := r.db.QueryContext(ctx,
		`SELECT
             ws.date AS date,
             MAX(wl.weight) AS top_weight,
             (SELECT wl2.reps
                FROM workout_logs wl2
               WHERE wl2.session_id  = ws.id
                 AND wl2.exercise_id = wl.exercise_id
               ORDER BY wl2.weight DESC, wl2.reps DESC
               LIMIT 1) AS top_reps,
             ROUND(SUM(wl.weight * wl.reps), 2) AS volume
         FROM workout_logs wl
         JOIN workout_sessions ws ON wl.session_id = ws.id
         WHERE ws.user_id = ? AND wl.exercise_id = ?
         GROUP BY ws.id
         ORDER BY ws.date ASC, ws.created_at ASC`, userID, exerciseID)
	if err != nil {
		return nil, err
	}
	return collect(rows, scanSessionMetric)
}

func scanSessionMetric(row scanner) (models.ExerciseSessionMetric, error) {
	var (
		metric models.ExerciseSessionMetric
		date   string
	)
	if err := row.Scan(&date, &metric.TopWeight, &metric.TopReps, &metric.Volume); err != nil {
		return models.ExerciseSessionMetric{}, err
	}
	var err error
	metric.Date, err = parseDate("workout_sessions.date", date)
	return metric, err
}

// ExerciseHistoryWithPR lists the newest sets of one exercise with IsPR and
// IsRecentPR computed on the fly (see FindLogsBySessionWithPR).
func (r *Workouts) ExerciseHistoryWithPR(ctx context.Context, query ExerciseHistoryQuery) ([]models.WorkoutLogWithExercise, error) {
	rows, err := r.db.QueryContext(ctx,
		`SELECT wl.id, wl.session_id, wl.exercise_id, e.name as exercise_name,
                wl.set_number, wl.reps, wl.weight, wl.rpe,
                CASE WHEN wl.weight = (
                    SELECT MAX(wl2.weight) FROM workout_logs wl2
                    JOIN workout_sessions ws2 ON wl2.session_id = ws2.id
                    WHERE ws2.user_id = ?1 AND wl2.exercise_id = wl.exercise_id
                ) THEN 1 ELSE 0 END as is_pr,
                CASE WHEN datetime(wl.created_at) >= datetime(?4) AND wl.weight = (
                    SELECT MAX(wl3.weight) FROM workout_logs wl3
                    JOIN workout_sessions ws3 ON wl3.session_id = ws3.id
                    WHERE ws3.user_id = ?1 AND wl3.exercise_id = wl.exercise_id
                      AND datetime(wl3.created_at) >= datetime(?4)
                ) THEN 1 ELSE 0 END as is_recent_pr
         FROM workout_logs wl
         JOIN workout_sessions ws ON wl.session_id = ws.id
         JOIN exercises e ON wl.exercise_id = e.id
         WHERE ws.user_id = ?1 AND wl.exercise_id = ?2
         ORDER BY ws.date DESC, wl.set_number
         LIMIT ?3`,
		query.UserID, query.ExerciseID, query.Limit, db.FormatTime(query.Since))
	if err != nil {
		return nil, err
	}
	return collect(rows, scanLogWithExercise)
}
