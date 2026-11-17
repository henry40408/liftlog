package server

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/netip"
	"strings"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/session"
)

// seedDatedWorkouts creates count workouts whose notes sort by age: note-01-x
// is the oldest and the list shows the newest first.
func (ta *testApp) seedDatedWorkouts(userID string, count int) {
	ta.t.Helper()
	for i := 1; i <= count; i++ {
		notes := fmt.Sprintf("note-%02d-x", i)
		if _, err := ta.app.workouts.CreateSession(ta.t.Context(), userID, wkDay.AddDate(0, 0, i), &notes); err != nil {
			ta.t.Fatal(err)
		}
	}
}

func TestWorkoutsListPagesSliceTheNewestFirstList(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	ta.seedDatedWorkouts(user.user.ID, 21)

	cases := []struct {
		page    string
		present []string
		absent  []string
	}{
		{"1", []string{"note-21-x", "note-12-x"}, []string{"note-11-x"}},
		{"2", []string{"note-11-x", "note-02-x"}, []string{"note-12-x", "note-01-x"}},
		{"3", []string{"note-01-x"}, []string{"note-02-x"}},
	}
	for _, c := range cases {
		page := ta.get("/workouts?page="+c.page, user.cookie)
		wantStatus(t, page, http.StatusOK)
		wantContains(t, page, "Page "+c.page+" / 3</span>")
		for _, note := range c.present {
			wantContains(t, page, note)
		}
		for _, note := range c.absent {
			if strings.Contains(page.text(), note) {
				t.Errorf("page %s lists %s", c.page, note)
			}
		}
	}
}

func TestWorkoutsListPageCountRoundsUpOnlyOnARemainder(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	ta.seedDatedWorkouts(user.user.ID, 2*workoutsPerPage)

	wantContains(t, ta.get("/workouts", user.cookie), "Page 1 / 2</span>")
}

func TestExerciseStatsShowsTheLoggedPR(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	exercise := ta.seedStatsExercise(user.user.ID, "Bench Press", "chest")
	ta.seedStatsSet(statsSet{userID: user.user.ID, exerciseID: exercise.ID, date: statsDate(2024, 1, 15), reps: 5, weight: 137})

	page := ta.get("/stats/exercise/"+exercise.ID, user.cookie)

	wantContains(t, page, `font-size: var(--font-4xl);">137</span>`)
	if strings.Contains(page.text(), "No PR for this exercise yet.") {
		t.Fatal("the PR card is missing")
	}
}

func TestNewUsesTheGivenLogSalt(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})

	got := ta.app.fingerprint("token")

	if want := session.TokenFingerprint("token", testLogSalt); got != want {
		t.Fatalf("fingerprint = %q, want %q", got, want)
	}
}

func TestProductionSensitiveAndSetupBudgetsHaveAWindow(t *testing.T) {
	throttles := NewThrottles()
	peer := netip.MustParseAddr("192.0.2.1")
	for range 5 {
		if !throttles.SensitiveAction.TryAcquire("user") || !throttles.Setup.TryAcquire(peer) {
			t.Fatal("a budget refused an attempt inside its limit")
		}
	}
	if throttles.SensitiveAction.TryAcquire("user") {
		t.Error("the sensitive-action budget has no window")
	}
	if throttles.Setup.TryAcquire(peer) {
		t.Error("the setup budget has no window")
	}
}

func TestSleepContextWithoutDelayNeverWaitsOnTheClient(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	req := httptest.NewRequest(http.MethodPost, "/auth/login", nil).WithContext(ctx)

	// A zero delay would build a timer that may not have fired yet; many rounds
	// make the select race against the cancelled context show up.
	for range 500 {
		if !sleepContext(req, 0) {
			t.Fatal("a zero delay reported the client as gone")
		}
	}
	if sleepContext(req, time.Hour) {
		t.Fatal("a cancelled client was waited for")
	}
}
