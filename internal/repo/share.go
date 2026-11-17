package repo

import (
	"context"
	"database/sql"
	"errors"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
)

// SetShareToken stores a fresh share token and returns it. A nil ttl never
// expires. ErrNotFound means the session is missing or not owned by userID.
func (r *Workouts) SetShareToken(ctx context.Context, id, userID string, ttl *time.Duration) (string, error) {
	token := db.NewID()
	var expiresAt any
	if ttl != nil {
		expiresAt = db.FormatTime(time.Now().UTC().Add(*ttl))
	}
	changed, err := execChanged(ctx, r.db,
		"UPDATE workout_sessions SET share_token = ?, share_expires_at = ? WHERE id = ? AND user_id = ?",
		token, expiresAt, id, userID)
	if err != nil {
		return "", err
	}
	if !changed {
		return "", ErrNotFound
	}
	return token, nil
}

// RevokeShareToken clears the token and its expiry; false means the session
// is missing or not owned by userID.
func (r *Workouts) RevokeShareToken(ctx context.Context, id, userID string) (bool, error) {
	return execChanged(ctx, r.db,
		"UPDATE workout_sessions SET share_token = NULL, share_expires_at = NULL WHERE id = ? AND user_id = ?",
		id, userID)
}

// FindSessionByShareToken filters expiry in SQL so expired and nonexistent
// tokens are indistinguishable: both are ErrNotFound.
func (r *Workouts) FindSessionByShareToken(ctx context.Context, token string) (models.WorkoutSession, error) {
	row := r.db.QueryRowContext(ctx,
		"SELECT "+sessionColumns+" FROM workout_sessions "+
			"WHERE share_token = ? AND (share_expires_at IS NULL OR share_expires_at > ?)",
		token, db.FormatTime(time.Now().UTC()))
	session, err := scanSession(row)
	if errors.Is(err, sql.ErrNoRows) {
		return models.WorkoutSession{}, ErrNotFound
	}
	return session, err
}

// CleanupExpiredShareTokens clears expired tokens and returns the count.
// Expiry is already enforced by FindSessionByShareToken; this keeps the
// owner's page from showing a dead link as shared.
func (r *Workouts) CleanupExpiredShareTokens(ctx context.Context) (int64, error) {
	return execCount(ctx, r.db,
		"UPDATE workout_sessions SET share_token = NULL, share_expires_at = NULL "+
			"WHERE share_expires_at IS NOT NULL AND share_expires_at <= ?",
		db.FormatTime(time.Now().UTC()))
}
