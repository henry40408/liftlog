package db

import (
	"fmt"
	"time"
)

// Timestamps are stored as TEXT in the format the former Rust server wrote
// (rusqlite + chrono, `%F %T%.f%:z`), so existing databases stay readable and
// rows written here stay readable by it. Always bind the strings below, never
// a time.Time: the driver would pick its own format.

const (
	dateLayout = "2006-01-02"
	timeLayout = "2006-01-02 15:04:05"
	zoneSuffix = "+00:00"
)

// readTimeLayouts are every spelling found in a database: the one written by
// FormatTime, RFC 3339 (what the driver renders for DATETIME columns), and
// SQLite's CURRENT_TIMESTAMP, which has no zone and means UTC. Fractional
// seconds are accepted by every layout even though none spells them out.
var readTimeLayouts = []string{
	"2006-01-02 15:04:05Z07:00",
	time.RFC3339,
	timeLayout,
	"2006-01-02T15:04:05",
}

// FormatTime renders t in UTC with a fraction of 0, 3, 6 or 9 digits, whichever
// is the shortest that is exact (chrono's `%.f`).
func FormatTime(t time.Time) string {
	t = t.UTC()
	return t.Format(timeLayout) + fraction(t.Nanosecond()) + zoneSuffix
}

func fraction(nanos int) string {
	switch {
	case nanos == 0:
		return ""
	case nanos%1_000_000 == 0:
		return fmt.Sprintf(".%03d", nanos/1_000_000)
	case nanos%1_000 == 0:
		return fmt.Sprintf(".%06d", nanos/1_000)
	}
	return fmt.Sprintf(".%09d", nanos)
}

// ParseTime reads a stored timestamp as an instant in UTC.
func ParseTime(s string) (time.Time, error) {
	for _, layout := range readTimeLayouts {
		if t, err := time.Parse(layout, s); err == nil {
			return t.UTC(), nil
		}
	}
	return time.Time{}, fmt.Errorf("unrecognised timestamp %q", s)
}

// FormatDate renders a calendar date. The date is read from t as is, with no
// timezone conversion.
func FormatDate(t time.Time) string {
	return t.Format(dateLayout)
}

// ParseDate reads a stored calendar date as midnight UTC. It also accepts the
// midnight timestamp the driver renders for DATE columns; the calendar day is
// taken as written, never converted.
func ParseDate(s string) (time.Time, error) {
	for _, layout := range []string{dateLayout, time.RFC3339} {
		if t, err := time.Parse(layout, s); err == nil {
			return time.Date(t.Year(), t.Month(), t.Day(), 0, 0, 0, 0, time.UTC), nil
		}
	}
	return time.Time{}, fmt.Errorf("unrecognised date %q", s)
}
