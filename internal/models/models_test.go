package models

import (
	"math"
	"testing"
	"time"
)

func TestUserRoleParse(t *testing.T) {
	cases := map[string]UserRole{
		"admin":   RoleAdmin,
		"user":    RoleUser,
		"unknown": RoleUser,
		"":        RoleUser,
	}
	for in, want := range cases {
		if got := ParseUserRole(in); got != want {
			t.Errorf("ParseUserRole(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestUserRoleStringValues(t *testing.T) {
	if RoleAdmin != "admin" || RoleUser != "user" {
		t.Fatalf("role strings changed: %q %q", RoleAdmin, RoleUser)
	}
}

func TestUserRoleIsAdmin(t *testing.T) {
	if !RoleAdmin.IsAdmin() {
		t.Error("admin must be admin")
	}
	if RoleUser.IsAdmin() {
		t.Error("user must not be admin")
	}
}

func TestChartPointComputesEpleyE1RM(t *testing.T) {
	metric := ExerciseSessionMetric{
		Date:      time.Date(2024, 1, 15, 0, 0, 0, 0, time.UTC),
		TopWeight: 100,
		TopReps:   6,
		Volume:    600,
	}
	point := NewChartPoint(metric)
	// Epley: 100 * (1 + 6/30) = 120
	if math.Abs(point.E1RM-120) > 1e-9 {
		t.Errorf("E1RM = %v, want 120", point.E1RM)
	}
	if point.TopWeight != 100 || point.TopReps != 6 || point.Volume != 600 || !point.Date.Equal(metric.Date) {
		t.Errorf("fields not copied: %+v", point)
	}
}

func TestChartPointE1RMWithZeroRepsEqualsTopWeight(t *testing.T) {
	point := NewChartPoint(ExerciseSessionMetric{TopWeight: 140})
	if math.Abs(point.E1RM-140) > 1e-9 {
		t.Errorf("E1RM = %v, want 140", point.E1RM)
	}
}

func TestRecentIsAllTime(t *testing.T) {
	recent := func(v float64) *float64 { return &v }
	cases := []struct {
		name string
		pr   PersonalRecordSummary
		want bool
	}{
		{"no recent value", PersonalRecordSummary{AllTimeValue: 100}, false},
		{"recent below all-time", PersonalRecordSummary{AllTimeValue: 100, RecentValue: recent(90)}, false},
		{"recent equals all-time", PersonalRecordSummary{AllTimeValue: 100, RecentValue: recent(100)}, true},
		{"recent above all-time", PersonalRecordSummary{AllTimeValue: 100, RecentValue: recent(110)}, true},
	}
	for _, c := range cases {
		if got := c.pr.RecentIsAllTime(); got != c.want {
			t.Errorf("%s: got %v, want %v", c.name, got, c.want)
		}
	}
}

func TestRecentPRWindowStartIsThirtyDaysBack(t *testing.T) {
	now := time.Date(2026, 3, 31, 12, 0, 0, 0, time.UTC)
	want := time.Date(2026, 3, 1, 12, 0, 0, 0, time.UTC)
	if got := RecentPRWindowStart(now); !got.Equal(want) {
		t.Errorf("got %v, want %v", got, want)
	}
}
