package views

import (
	"strings"
	"testing"
	"time"
)

func TestRecentPRDateIsADashUntilThereIsARecentPR(t *testing.T) {
	if got := string(recentPRDate(nil)); got != string(emptyDash()) {
		t.Errorf("no recent PR rendered %q, want the dash", got)
	}

	at := time.Date(2026, 3, 4, 23, 30, 0, 0, time.UTC)
	got := string(recentPRDate(&at))
	if !strings.Contains(got, `data-fmt="date"`) || !strings.Contains(got, "2026-03-04") {
		t.Errorf("recent PR rendered %q, want a date element for 2026-03-04", got)
	}
}

func TestSharedNotesRendersEscapedNotesOrNothing(t *testing.T) {
	if got := sharedNotes(nil); got != "" {
		t.Errorf("no notes rendered %q, want nothing", got)
	}

	notes := "<b>heavy</b> day"
	got := string(sharedNotes(&notes))
	if !strings.Contains(got, "&lt;b&gt;heavy&lt;/b&gt; day") || strings.Contains(got, "<b>") {
		t.Errorf("notes rendered %q, want them HTML-escaped inside <em>", got)
	}
}
