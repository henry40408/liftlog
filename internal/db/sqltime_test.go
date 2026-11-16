package db

import (
	"testing"
	"time"
)

func TestFormatTimeMatchesChronoFractionRules(t *testing.T) {
	base := time.Date(2024, 3, 5, 7, 8, 9, 0, time.UTC)
	cases := []struct {
		nanos int
		want  string
	}{
		{0, "2024-03-05 07:08:09+00:00"},
		{120_000_000, "2024-03-05 07:08:09.120+00:00"},
		{1_000_000, "2024-03-05 07:08:09.001+00:00"},
		{123_456_000, "2024-03-05 07:08:09.123456+00:00"},
		{123_456_789, "2024-03-05 07:08:09.123456789+00:00"},
		{1, "2024-03-05 07:08:09.000000001+00:00"},
	}
	for _, c := range cases {
		if got := FormatTime(base.Add(time.Duration(c.nanos))); got != c.want {
			t.Errorf("nanos %d: got %q, want %q", c.nanos, got, c.want)
		}
	}
}

func TestFormatTimeConvertsToUTC(t *testing.T) {
	taipei := time.FixedZone("UTC+8", 8*3600)
	got := FormatTime(time.Date(2024, 3, 5, 15, 0, 0, 0, taipei))
	if want := "2024-03-05 07:00:00+00:00"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestParseTimeAcceptsEveryStoredSpelling(t *testing.T) {
	want := time.Date(2024, 3, 5, 7, 8, 9, 0, time.UTC)
	for _, s := range []string{
		"2024-03-05 07:08:09+00:00",
		"2024-03-05T07:08:09Z",
		"2024-03-05T07:08:09+00:00",
		"2024-03-05 07:08:09",
		"2024-03-05T07:08:09",
		"2024-03-05 15:08:09+08:00",
	} {
		got, err := ParseTime(s)
		if err != nil || !got.Equal(want) || got.Location() != time.UTC {
			t.Errorf("%q: got %v, %v", s, got, err)
		}
	}
}

func TestParseTimeKeepsFractionalSeconds(t *testing.T) {
	for _, s := range []string{
		"2024-03-05 07:08:09.123456789+00:00",
		"2024-03-05T07:08:09.123456789Z",
		"2024-03-05 07:08:09.123456789",
	} {
		got, err := ParseTime(s)
		if err != nil || got.Nanosecond() != 123_456_789 {
			t.Errorf("%q: got %v, %v", s, got, err)
		}
	}
}

func TestParseTimeRejectsGarbage(t *testing.T) {
	for _, s := range []string{"", "yesterday", "2024-03-05"} {
		if _, err := ParseTime(s); err == nil {
			t.Errorf("%q must be rejected", s)
		}
	}
}

func TestTimeRoundTripsThroughStorage(t *testing.T) {
	for _, nanos := range []int{0, 500_000_000, 123_000, 987_654_321} {
		in := time.Date(2026, 11, 17, 23, 59, 58, nanos, time.UTC)
		out, err := ParseTime(FormatTime(in))
		if err != nil || !out.Equal(in) {
			t.Errorf("nanos %d: got %v, %v", nanos, out, err)
		}
	}
}

func TestDateRoundTrip(t *testing.T) {
	got, err := ParseDate("2024-01-15")
	if err != nil {
		t.Fatal(err)
	}
	if want := time.Date(2024, 1, 15, 0, 0, 0, 0, time.UTC); !got.Equal(want) {
		t.Errorf("got %v, want %v", got, want)
	}
	if s := FormatDate(got); s != "2024-01-15" {
		t.Errorf("got %q", s)
	}
}

func TestFormatDateDoesNotShiftTheCalendarDay(t *testing.T) {
	late := time.Date(2024, 1, 15, 23, 30, 0, 0, time.FixedZone("UTC-8", -8*3600))
	if got := FormatDate(late); got != "2024-01-15" {
		t.Errorf("got %q", got)
	}
}

func TestParseDateRejectsGarbage(t *testing.T) {
	for _, s := range []string{"", "15/01/2024", "2024-13-01"} {
		if _, err := ParseDate(s); err == nil {
			t.Errorf("%q must be rejected", s)
		}
	}
}
