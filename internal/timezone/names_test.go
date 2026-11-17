package timezone

import (
	"os"
	"slices"
	"strings"
	"testing"
	"time"
)

func TestNamesMatchZonesFileInOrder(t *testing.T) {
	file, err := os.ReadFile("zones.txt")
	if err != nil {
		t.Fatal(err)
	}
	var want []string
	for line := range strings.SplitSeq(string(file), "\n") {
		if line = strings.TrimSpace(line); line != "" {
			want = append(want, line)
		}
	}

	got := Names()
	if len(got) == 0 {
		t.Fatal("Names is empty")
	}
	if !slices.Equal(got, want) {
		t.Fatalf("Names has %d entries, zones.txt has %d, or the order differs", len(got), len(want))
	}
}

func TestNamesContainKnownZonesAndEveryNameParses(t *testing.T) {
	names := Names()
	for _, known := range []string{"Asia/Taipei", "America/New_York", "UTC"} {
		if !slices.Contains(names, known) {
			t.Errorf("Names lacks %q", known)
		}
	}
	for _, name := range names {
		if _, ok := Parse(name); !ok {
			t.Errorf("Names lists %q, which Parse rejects", name)
		}
	}
}

// TodayIn reads the real clock, so the result is bracketed by DateAt on both
// sides of the call; that stays correct if midnight passes mid-test.
func TestTodayInIsTheCurrentDateInTheZoneAsMidnightUTC(t *testing.T) {
	for _, name := range []string{"UTC", "Asia/Taipei", "Pacific/Kiritimati", "Pacific/Pago_Pago"} {
		before := DateAt(time.Now(), zone(name))
		got := TodayIn(zone(name))
		after := DateAt(time.Now(), zone(name))
		if !got.Equal(before) && !got.Equal(after) {
			t.Errorf("TodayIn(%q) = %v, want %v or %v", name, got, before, after)
		}
		if got.Location() != time.UTC || got.Hour() != 0 || got.Minute() != 0 {
			t.Errorf("TodayIn(%q) = %v, want midnight UTC", name, got)
		}
	}
}

func TestTodayInWithoutAZoneUsesUTC(t *testing.T) {
	before := DateAt(time.Now(), nil)
	got := TodayIn(nil)
	after := DateAt(time.Now(), nil)
	if !got.Equal(before) && !got.Equal(after) {
		t.Errorf("TodayIn(nil) = %v, want %v or %v", got, before, after)
	}
}
