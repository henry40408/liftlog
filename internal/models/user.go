// Package models holds the plain data types shared by repositories, handlers
// and views. Scanning rows into them is the repositories' job.
package models

import (
	"log/slog"
	"time"
)

type UserRole string

const (
	RoleAdmin UserRole = "admin"
	RoleUser  UserRole = "user"
)

// ParseUserRole maps a stored role to a UserRole. An unknown value is logged
// and treated as an ordinary user, never as an admin.
func ParseUserRole(s string) UserRole {
	switch s {
	case string(RoleAdmin):
		return RoleAdmin
	case string(RoleUser):
		return RoleUser
	}
	slog.Warn("unknown user role in DB; defaulting to user", "role", s)
	return RoleUser
}

func (r UserRole) IsAdmin() bool { return r == RoleAdmin }

type User struct {
	ID           string
	Username     string
	PasswordHash string
	Role         UserRole
	CreatedAt    time.Time
	// Timezone is an IANA name; nil until the user picks one (treated as UTC).
	Timezone *string
}

// UserListItem is an admin users-list row; it omits the password hash on
// purpose.
type UserListItem struct {
	ID        string
	Username  string
	Role      UserRole
	CreatedAt time.Time
}
