package repo

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
)

// Users stores accounts. It never hashes or verifies passwords: callers pass
// hashes in and verify the hash returned by FindByUsername.
type Users struct {
	db *sql.DB
}

func NewUsers(pool *sql.DB) *Users {
	return &Users{db: pool}
}

// NewUser is the input of Create; PasswordHash is already hashed.
type NewUser struct {
	Username     string
	PasswordHash string
	Role         models.UserRole
}

const userColumns = "id, username, password_hash, role, created_at, timezone"

func scanUser(row scanner) (models.User, error) {
	var (
		user      models.User
		role      string
		createdAt string
		timezone  sql.NullString
	)
	if err := row.Scan(&user.ID, &user.Username, &user.PasswordHash, &role, &createdAt, &timezone); err != nil {
		return models.User{}, err
	}
	user.Role = models.ParseUserRole(role)
	user.Timezone = stringPtr(timezone)
	var err error
	user.CreatedAt, err = parseTime("users.created_at", createdAt)
	return user, err
}

func (r *Users) Count(ctx context.Context) (int, error) {
	var count int
	err := r.db.QueryRowContext(ctx, "SELECT COUNT(*) FROM users").Scan(&count)
	return count, err
}

// FindByID returns ErrNotFound when there is no such user.
func (r *Users) FindByID(ctx context.Context, id string) (models.User, error) {
	return r.findOne(ctx, "id", id)
}

// FindByUsername is the login lookup; the caller verifies PasswordHash. The
// match is case-sensitive by design.
func (r *Users) FindByUsername(ctx context.Context, username string) (models.User, error) {
	return r.findOne(ctx, "username", username)
}

func (r *Users) findOne(ctx context.Context, column, value string) (models.User, error) {
	row := r.db.QueryRowContext(ctx, "SELECT "+userColumns+" FROM users WHERE "+column+" = ?", value)
	user, err := scanUser(row)
	if errors.Is(err, sql.ErrNoRows) {
		return models.User{}, ErrNotFound
	}
	return user, err
}

// FindAll lists users newest first. It selects explicit columns so the
// password hash stays in the database.
func (r *Users) FindAll(ctx context.Context) ([]models.UserListItem, error) {
	rows, err := r.db.QueryContext(ctx,
		"SELECT id, username, role, created_at FROM users ORDER BY created_at DESC")
	if err != nil {
		return nil, err
	}
	return collect(rows, scanUserListItem)
}

func scanUserListItem(row scanner) (models.UserListItem, error) {
	var (
		item      models.UserListItem
		role      string
		createdAt string
	)
	if err := row.Scan(&item.ID, &item.Username, &role, &createdAt); err != nil {
		return models.UserListItem{}, err
	}
	item.Role = models.ParseUserRole(role)
	var err error
	item.CreatedAt, err = parseTime("users.created_at", createdAt)
	return item, err
}

func (r *Users) Create(ctx context.Context, input NewUser) (models.User, error) {
	user := newUserRow(input)
	_, err := r.db.ExecContext(ctx,
		"INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, ?, ?)",
		user.ID, user.Username, user.PasswordHash, string(user.Role), db.FormatTime(user.CreatedAt))
	if err != nil {
		return models.User{}, fmt.Errorf("insert user: %w", err)
	}
	return user, nil
}

// CreateFirstAdmin creates the admin only if the users table is empty. The
// bool is false when another request got there first. The check and the
// INSERT are one statement, so concurrent setups cannot both succeed.
func (r *Users) CreateFirstAdmin(ctx context.Context, username, passwordHash string) (models.User, bool, error) {
	user := newUserRow(NewUser{Username: username, PasswordHash: passwordHash, Role: models.RoleAdmin})
	result, err := r.db.ExecContext(ctx,
		"INSERT INTO users (id, username, password_hash, role, created_at) "+
			"SELECT ?, ?, ?, ?, ? WHERE NOT EXISTS (SELECT 1 FROM users)",
		user.ID, user.Username, user.PasswordHash, string(user.Role), db.FormatTime(user.CreatedAt))
	if err != nil {
		return models.User{}, false, fmt.Errorf("insert first admin: %w", err)
	}
	inserted, err := rowsChanged(result)
	if err != nil {
		return models.User{}, false, err
	}
	if !inserted {
		return models.User{}, false, nil
	}
	return user, true, nil
}

func newUserRow(input NewUser) models.User {
	return models.User{
		ID:           db.NewID(),
		Username:     input.Username,
		PasswordHash: input.PasswordHash,
		Role:         input.Role,
		CreatedAt:    time.Now().UTC(),
	}
}

// ChangePassword stores an already hashed password; false means no such user.
func (r *Users) ChangePassword(ctx context.Context, userID, passwordHash string) (bool, error) {
	return execChanged(ctx, r.db, "UPDATE users SET password_hash = ? WHERE id = ?", passwordHash, userID)
}

func (r *Users) Delete(ctx context.Context, id string) (bool, error) {
	return execChanged(ctx, r.db, "DELETE FROM users WHERE id = ?", id)
}

func (r *Users) UpdateRole(ctx context.Context, id string, role models.UserRole) (bool, error) {
	return execChanged(ctx, r.db, "UPDATE users SET role = ? WHERE id = ?", string(role), id)
}

// FindTimezone returns nil when the user is missing or has not chosen one.
func (r *Users) FindTimezone(ctx context.Context, userID string) (*string, error) {
	var timezone sql.NullString
	err := r.db.QueryRowContext(ctx, "SELECT timezone FROM users WHERE id = ?", userID).Scan(&timezone)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	return stringPtr(timezone), nil
}

// SetTimezone does not validate the name; the caller must.
func (r *Users) SetTimezone(ctx context.Context, userID, timezone string) (bool, error) {
	return execChanged(ctx, r.db, "UPDATE users SET timezone = ? WHERE id = ?", timezone, userID)
}
