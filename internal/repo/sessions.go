package repo

import (
	"context"
	"database/sql"
	"errors"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/session"
)

// Sessions stores login sessions. Tokens are opaque and DB-backed, so
// deleting a row revokes the session at once.
type Sessions struct {
	db *sql.DB
}

func NewSessions(pool *sql.DB) *Sessions {
	return &Sessions{db: pool}
}

// ValidateKind separates unknown tokens (probing) from expired ones for the
// audit log.
type ValidateKind int

const (
	// ValidateUnknown: no such token, or its user is gone.
	ValidateUnknown ValidateKind = iota
	ValidateValid
	// ValidateExpiredIdle: the idle timeout passed; the row is deleted.
	ValidateExpiredIdle
	// ValidateExpiredAbsolute: the absolute timeout passed; the row is deleted.
	ValidateExpiredAbsolute
)

// ValidateResult carries the user's identity so request handling needs no
// second lookup. The identity fields are set only for ValidateValid.
type ValidateResult struct {
	Kind     ValidateKind
	UserID   string
	Username string
	Role     models.UserRole
	// NewExpiresAt is non-nil iff this call slid expires_at forward; the
	// cookie must then be refreshed. Nil still means a valid session
	// (throttled, or pinned at the absolute cap).
	NewExpiresAt *time.Time
}

type SessionListRow struct {
	Token         string
	CreatedAt     time.Time
	LastTouchedAt time.Time
}

// sessionRow is a session joined with its user.
type sessionRow struct {
	userID   string
	username string
	role     models.UserRole
	times    session.SessionTimes
}

// Create returns the new session's token.
func (r *Sessions) Create(ctx context.Context, userID string) (string, error) {
	token := db.NewID()
	now := time.Now().UTC()
	expiresAt := now.Add(session.IdleTTL)
	_, err := r.db.ExecContext(ctx,
		"INSERT INTO sessions (token, user_id, created_at, expires_at, last_touched_at) VALUES (?, ?, ?, ?, ?)",
		token, userID, db.FormatTime(now), db.FormatTime(expiresAt), db.FormatTime(now))
	return token, err
}

// ValidateAndTouch checks the token and, once the throttle window has
// elapsed, slides expires_at and last_touched_at forward. Expired rows are
// deleted on the spot.
func (r *Sessions) ValidateAndTouch(ctx context.Context, token string) (ValidateResult, error) {
	row, err := r.findWithUser(ctx, token)
	if errors.Is(err, ErrNotFound) {
		return ValidateResult{Kind: ValidateUnknown}, nil
	}
	if err != nil {
		return ValidateResult{}, err
	}

	now := time.Now().UTC()
	if !row.times.ExpiresAt.After(now) {
		return r.expire(ctx, token, ValidateExpiredIdle)
	}
	if !now.Before(session.AbsoluteCap(row.times.CreatedAt)) {
		return r.expire(ctx, token, ValidateExpiredAbsolute)
	}

	result := ValidateResult{Kind: ValidateValid, UserID: row.userID, Username: row.username, Role: row.role}
	action := session.ComputeTouchAction(row.times, now)
	switch action.Kind {
	case session.TouchOnly:
		_, err = r.db.ExecContext(ctx, "UPDATE sessions SET last_touched_at = ? WHERE token = ?",
			db.FormatTime(now), token)
	case session.TouchSlide:
		_, err = r.db.ExecContext(ctx, "UPDATE sessions SET last_touched_at = ?, expires_at = ? WHERE token = ?",
			db.FormatTime(now), db.FormatTime(action.ExpiresAt), token)
		result.NewExpiresAt = &action.ExpiresAt
	}
	if err != nil {
		return ValidateResult{}, err
	}
	return result, nil
}

func (r *Sessions) expire(ctx context.Context, token string, kind ValidateKind) (ValidateResult, error) {
	if err := r.Delete(ctx, token); err != nil {
		return ValidateResult{}, err
	}
	return ValidateResult{Kind: kind}, nil
}

func (r *Sessions) findWithUser(ctx context.Context, token string) (sessionRow, error) {
	var (
		row                                 sessionRow
		role, createdAt, expiresAt, touched string
	)
	err := r.db.QueryRowContext(ctx,
		"SELECT s.user_id, s.created_at, s.expires_at, s.last_touched_at, u.username, u.role "+
			"FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token = ?", token,
	).Scan(&row.userID, &createdAt, &expiresAt, &touched, &row.username, &role)
	if errors.Is(err, sql.ErrNoRows) {
		return sessionRow{}, ErrNotFound
	}
	if err != nil {
		return sessionRow{}, err
	}
	row.role = models.ParseUserRole(role)
	row.times, err = parseSessionTimes(createdAt, expiresAt, touched)
	return row, err
}

func parseSessionTimes(createdAt, expiresAt, touched string) (session.SessionTimes, error) {
	var (
		times session.SessionTimes
		err   error
	)
	if times.CreatedAt, err = parseTime("sessions.created_at", createdAt); err != nil {
		return times, err
	}
	if times.ExpiresAt, err = parseTime("sessions.expires_at", expiresAt); err != nil {
		return times, err
	}
	times.LastTouchedAt, err = parseTime("sessions.last_touched_at", touched)
	return times, err
}

func (r *Sessions) Delete(ctx context.Context, token string) error {
	_, err := r.db.ExecContext(ctx, "DELETE FROM sessions WHERE token = ?", token)
	return err
}

// DeleteAllForUserExcept returns the number of rows deleted, for the audit
// log.
func (r *Sessions) DeleteAllForUserExcept(ctx context.Context, userID, keepToken string) (int64, error) {
	return execCount(ctx, r.db, "DELETE FROM sessions WHERE user_id = ? AND token != ?", userID, keepToken)
}

// DeleteAllForUser deletes every session of a user (admin delete, promotion)
// and returns the count. After a user delete the FK cascade has usually done
// this already.
func (r *Sessions) DeleteAllForUser(ctx context.Context, userID string) (int64, error) {
	return execCount(ctx, r.db, "DELETE FROM sessions WHERE user_id = ?", userID)
}

// ListForUser lists the unexpired sessions of a user, newest-touched first.
// The created_at filter hides legacy rows whose expires_at predates the
// absolute-cap clamp, until CleanupExpired retires them.
func (r *Sessions) ListForUser(ctx context.Context, userID string) ([]SessionListRow, error) {
	now := time.Now().UTC()
	rows, err := r.db.QueryContext(ctx,
		"SELECT token, created_at, last_touched_at FROM sessions "+
			"WHERE user_id = ? AND expires_at > ? AND created_at > ? "+
			"ORDER BY last_touched_at DESC",
		userID, db.FormatTime(now), db.FormatTime(now.Add(-session.AbsoluteTTL)))
	if err != nil {
		return nil, err
	}
	return collect(rows, scanSessionListRow)
}

func scanSessionListRow(row scanner) (SessionListRow, error) {
	var (
		item               SessionListRow
		createdAt, touched string
	)
	if err := row.Scan(&item.Token, &createdAt, &touched); err != nil {
		return SessionListRow{}, err
	}
	var err error
	if item.CreatedAt, err = parseTime("sessions.created_at", createdAt); err != nil {
		return SessionListRow{}, err
	}
	item.LastTouchedAt, err = parseTime("sessions.last_touched_at", touched)
	return item, err
}

// CountForUser is read before a user delete: the FK cascade leaves nothing to
// count afterwards.
func (r *Sessions) CountForUser(ctx context.Context, userID string) (int, error) {
	var count int
	err := r.db.QueryRowContext(ctx, "SELECT COUNT(*) FROM sessions WHERE user_id = ?", userID).Scan(&count)
	return count, err
}

// CleanupExpired deletes every expired session and returns the count. The
// created_at arm retires legacy rows whose expires_at was slid past the
// absolute cap before it was clamped.
func (r *Sessions) CleanupExpired(ctx context.Context) (int64, error) {
	now := time.Now().UTC()
	return execCount(ctx, r.db, "DELETE FROM sessions WHERE expires_at <= ? OR created_at <= ?",
		db.FormatTime(now), db.FormatTime(now.Add(-session.AbsoluteTTL)))
}
