package repo

import (
	"context"
	"database/sql"
	"errors"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
)

// Workouts stores workout sessions, their logs, and the records derived from
// them. Methods are split over workouts.go, logs.go, records.go and share.go.
// Every date argument is a calendar date, formatted without timezone
// conversion.
type Workouts struct {
	db *sql.DB
}

func NewWorkouts(pool *sql.DB) *Workouts {
	return &Workouts{db: pool}
}

// SessionEdit updates a workout session owned by UserID. A nil Date keeps the
// stored date; Notes is always written, and nil clears it.
type SessionEdit struct {
	ID     string
	UserID string
	Date   *time.Time
	Notes  *string
}

const sessionColumns = "id, user_id, date, notes, share_token, share_expires_at, created_at"

func scanSession(row scanner) (models.WorkoutSession, error) {
	var (
		session                 models.WorkoutSession
		date, createdAt         string
		notes, token, expiresAt sql.NullString
	)
	err := row.Scan(&session.ID, &session.UserID, &date, &notes, &token, &expiresAt, &createdAt)
	if err != nil {
		return models.WorkoutSession{}, err
	}
	session.Notes = stringPtr(notes)
	session.ShareToken = stringPtr(token)
	if session.Date, err = parseDate("workout_sessions.date", date); err != nil {
		return models.WorkoutSession{}, err
	}
	if session.ShareExpiresAt, err = parseNullTime("workout_sessions.share_expires_at", expiresAt); err != nil {
		return models.WorkoutSession{}, err
	}
	session.CreatedAt, err = parseTime("workout_sessions.created_at", createdAt)
	return session, err
}

func (r *Workouts) CreateSession(ctx context.Context, userID string, date time.Time, notes *string) (models.WorkoutSession, error) {
	session := models.WorkoutSession{
		ID:        db.NewID(),
		UserID:    userID,
		Date:      date,
		Notes:     notes,
		CreatedAt: time.Now().UTC(),
	}
	_, err := r.db.ExecContext(ctx,
		"INSERT INTO workout_sessions (id, user_id, date, notes, created_at) VALUES (?, ?, ?, ?, ?)",
		session.ID, session.UserID, db.FormatDate(session.Date), nullableString(notes), db.FormatTime(session.CreatedAt))
	return session, err
}

// FindOwnedSession returns ErrNotFound for missing and foreign rows alike, so
// existence is not leaked.
func (r *Workouts) FindOwnedSession(ctx context.Context, id, userID string) (models.WorkoutSession, error) {
	session, err := r.FindSessionByID(ctx, id)
	if err != nil {
		return models.WorkoutSession{}, err
	}
	if session.UserID != userID {
		return models.WorkoutSession{}, ErrNotFound
	}
	return session, nil
}

// FindSessionByID returns ErrNotFound when there is no such session.
func (r *Workouts) FindSessionByID(ctx context.Context, id string) (models.WorkoutSession, error) {
	row := r.db.QueryRowContext(ctx, "SELECT "+sessionColumns+" FROM workout_sessions WHERE id = ?", id)
	session, err := scanSession(row)
	if errors.Is(err, sql.ErrNoRows) {
		return models.WorkoutSession{}, ErrNotFound
	}
	return session, err
}

func (r *Workouts) FindSessionsByUserPaginated(ctx context.Context, userID string, limit, offset int) ([]models.WorkoutSession, error) {
	rows, err := r.db.QueryContext(ctx,
		"SELECT "+sessionColumns+" FROM workout_sessions WHERE user_id = ? ORDER BY date DESC LIMIT ? OFFSET ?",
		userID, limit, offset)
	if err != nil {
		return nil, err
	}
	return collect(rows, scanSession)
}

func (r *Workouts) CountSessionsByUser(ctx context.Context, userID string) (int, error) {
	var count int
	err := r.db.QueryRowContext(ctx, "SELECT COUNT(*) FROM workout_sessions WHERE user_id = ?", userID).Scan(&count)
	return count, err
}

// UpdateSession reports false when the session is missing or not owned by
// edit.UserID.
func (r *Workouts) UpdateSession(ctx context.Context, edit SessionEdit) (bool, error) {
	if edit.Date == nil {
		return execChanged(ctx, r.db,
			"UPDATE workout_sessions SET notes = ? WHERE id = ? AND user_id = ?",
			nullableString(edit.Notes), edit.ID, edit.UserID)
	}
	return execChanged(ctx, r.db,
		"UPDATE workout_sessions SET date = ?, notes = ? WHERE id = ? AND user_id = ?",
		db.FormatDate(*edit.Date), nullableString(edit.Notes), edit.ID, edit.UserID)
}

func (r *Workouts) DeleteSession(ctx context.Context, id, userID string) (bool, error) {
	return execChanged(ctx, r.db, "DELETE FROM workout_sessions WHERE id = ? AND user_id = ?", id, userID)
}

// CountSessionsSince counts sessions dated on or after the given calendar
// date, which the caller derives from the user's timezone (week and month
// starts alike).
func (r *Workouts) CountSessionsSince(ctx context.Context, userID string, since time.Time) (int, error) {
	var count int
	err := r.db.QueryRowContext(ctx,
		"SELECT COUNT(*) FROM workout_sessions WHERE user_id = ? AND date >= ?",
		userID, db.FormatDate(since)).Scan(&count)
	return count, err
}

// TotalVolumeSince sums weight x reps over sessions dated on or after since.
// Weights have two decimals, which binary floats cannot hold exactly, so the
// sum is rounded to keep noise such as 168.29999999999998 off the pages.
func (r *Workouts) TotalVolumeSince(ctx context.Context, userID string, since time.Time) (float64, error) {
	var volume sql.NullFloat64
	err := r.db.QueryRowContext(ctx,
		"SELECT ROUND(SUM(wl.weight * wl.reps), 2) FROM workout_logs wl "+
			"JOIN workout_sessions ws ON wl.session_id = ws.id "+
			"WHERE ws.user_id = ? AND ws.date >= ?",
		userID, db.FormatDate(since)).Scan(&volume)
	return volume.Float64, err
}
