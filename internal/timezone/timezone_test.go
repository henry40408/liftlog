package timezone

import (
	"bufio"
	"os"
	"strings"
	"testing"
	"time"
)

func date(month time.Month, day int) time.Time {
	return time.Date(2026, month, day, 0, 0, 0, 0, time.UTC)
}

func zone(name string) *string { return &name }

func TestParseAcceptsIANANamesOnly(t *testing.T) {
	for _, name := range []string{"Asia/Taipei", "America/New_York", "UTC"} {
		if _, ok := Parse(name); !ok {
			t.Errorf("%q rejected", name)
		}
	}
	for _, name := range []string{"Not/AZone", "", "asia/taipei", "Local", "../etc/passwd"} {
		if _, ok := Parse(name); ok {
			t.Errorf("%q accepted", name)
		}
	}
}

// Every name the Rust server accepted must load from the embedded database.
func TestEveryChronoTzNameLoads(t *testing.T) {
	file, err := os.Open("zones.txt")
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()

	count := 0
	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		name := strings.TrimSpace(scanner.Text())
		if name == "" {
			continue
		}
		count++
		if _, ok := Parse(name); !ok {
			t.Errorf("%q does not load", name)
		}
	}
	if count < 500 {
		t.Fatalf("only %d names read", count)
	}
}

func TestDateAtFollowsTheZoneAcrossTheUTCDateLine(t *testing.T) {
	// 20:00 UTC on 11-08 is already 11-09 in Taipei but still 11-08 in LA.
	instant := time.Date(2026, 11, 8, 20, 0, 0, 0, time.UTC)
	if got := DateAt(instant, zone("Asia/Taipei")); !got.Equal(date(11, 9)) {
		t.Errorf("Taipei = %v", got)
	}
	if got := DateAt(instant, zone("America/Los_Angeles")); !got.Equal(date(11, 8)) {
		t.Errorf("Los Angeles = %v", got)
	}
}

func TestDateAtFallsBackToUTCWhenUnsetOrUnknown(t *testing.T) {
	instant := time.Date(2026, 11, 8, 20, 0, 0, 0, time.UTC)
	if got := DateAt(instant, nil); !got.Equal(date(11, 8)) {
		t.Errorf("unset = %v", got)
	}
	if got := DateAt(instant, zone("Not/AZone")); !got.Equal(date(11, 8)) {
		t.Errorf("unknown = %v", got)
	}
}

func TestDateAtReturnsMidnightUTC(t *testing.T) {
	got := DateAt(time.Now(), zone("Asia/Taipei"))
	if got.Location() != time.UTC || got.Hour() != 0 || got.Minute() != 0 {
		t.Fatalf("got %v", got)
	}
}

func TestWindowStartCountsTodayAsTheFirstDay(t *testing.T) {
	today := date(11, 9)
	if got := WindowStart(today, 7); !got.Equal(date(11, 3)) {
		t.Errorf("7 days = %v", got)
	}
	if got := WindowStart(today, 1); !got.Equal(today) {
		t.Errorf("1 day = %v", got)
	}
}
