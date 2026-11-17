// Package timezone handles per-user timezones. workout_sessions.date is a
// calendar date in the user's own timezone, so "today" and rolling windows must
// be computed there, not in UTC or the server's local zone.
package timezone

import (
	_ "embed"
	"strings"
	"sync"
	"time"
	_ "time/tzdata" // the scratch image has no zoneinfo
)

// zones.txt is the chrono-tz name list the Rust server accepted, one per line.
// Matching it exactly (case-sensitive) keeps the set of valid names unchanged.
//
//go:embed zones.txt
var zonesFile string

// locations maps every accepted name to its loaded zone, built on first use.
// A name the embedded database cannot load is left out, so Parse rejects it.
var locations = sync.OnceValue(func() map[string]*time.Location {
	zones := make(map[string]*time.Location)
	for name := range strings.SplitSeq(zonesFile, "\n") {
		name = strings.TrimSpace(name)
		if name == "" {
			continue
		}
		if location, err := time.LoadLocation(name); err == nil {
			zones[name] = location
		}
	}
	return zones
})

// Parse loads an IANA name such as Asia/Taipei; ok is false for any other
// string.
func Parse(name string) (location *time.Location, ok bool) {
	location, ok = locations()[name]
	return location, ok
}

// TodayIn is the calendar date "now" in the given zone, as midnight UTC.
func TodayIn(zone *string) time.Time {
	return DateAt(time.Now(), zone)
}

// DateAt is the calendar date of instant in the given zone, as midnight UTC.
// A nil zone (not chosen yet) or an unknown name (for example one removed from
// the IANA database) falls back to UTC.
func DateAt(instant time.Time, zone *string) time.Time {
	location := time.UTC
	if zone != nil {
		if parsed, ok := Parse(*zone); ok {
			location = parsed
		}
	}
	year, month, day := instant.In(location).Date()
	return time.Date(year, month, day, 0, 0, 0, 0, time.UTC)
}

// WindowStart is the first date of a window that covers days calendar days
// ending today.
func WindowStart(today time.Time, days int) time.Time {
	return today.AddDate(0, 0, -(days - 1))
}
