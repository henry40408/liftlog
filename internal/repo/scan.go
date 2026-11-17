package repo

import (
	"context"
	"database/sql"
	"fmt"
	"time"

	"github.com/henry40408/liftlog/internal/db"
)

// scanner is the part of *sql.Row and *sql.Rows that the scan functions use.
type scanner interface {
	Scan(dest ...any) error
}

// Timestamps and dates are scanned as strings and parsed by package db: the
// driver renders DATE/DATETIME columns as time.Time but leaves expression
// results as the stored text.

func parseTime(column, s string) (time.Time, error) {
	t, err := db.ParseTime(s)
	if err != nil {
		return time.Time{}, fmt.Errorf("column %s: %w", column, err)
	}
	return t, nil
}

func parseDate(column, s string) (time.Time, error) {
	t, err := db.ParseDate(s)
	if err != nil {
		return time.Time{}, fmt.Errorf("column %s: %w", column, err)
	}
	return t, nil
}

func parseNullTime(column string, s sql.NullString) (*time.Time, error) {
	if !s.Valid {
		return nil, nil
	}
	t, err := parseTime(column, s.String)
	if err != nil {
		return nil, err
	}
	return &t, nil
}

func stringPtr(s sql.NullString) *string {
	if !s.Valid {
		return nil
	}
	return &s.String
}

func intPtr(n sql.NullInt64) *int {
	if !n.Valid {
		return nil
	}
	v := int(n.Int64)
	return &v
}

// nullableInt binds a nil pointer as SQL NULL.
func nullableInt(n *int) any {
	if n == nil {
		return nil
	}
	return *n
}

func nullableString(s *string) any {
	if s == nil {
		return nil
	}
	return *s
}

func collect[T any](rows *sql.Rows, scan func(scanner) (T, error)) ([]T, error) {
	defer rows.Close()
	items := []T{}
	for rows.Next() {
		item, err := scan(rows)
		if err != nil {
			return nil, err
		}
		items = append(items, item)
	}
	return items, rows.Err()
}

func execCount(ctx context.Context, pool *sql.DB, query string, args ...any) (int64, error) {
	result, err := pool.ExecContext(ctx, query, args...)
	if err != nil {
		return 0, err
	}
	return result.RowsAffected()
}

func rowsChanged(result sql.Result) (bool, error) {
	n, err := result.RowsAffected()
	return n > 0, err
}

// execChanged runs a write and reports whether it touched any row.
func execChanged(ctx context.Context, pool *sql.DB, query string, args ...any) (bool, error) {
	result, err := pool.ExecContext(ctx, query, args...)
	if err != nil {
		return false, err
	}
	return rowsChanged(result)
}
