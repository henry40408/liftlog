package views

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/version"
)

// capturedVersion is the build version baked into the Rust goldens; see
// testdata/README.txt. It is the only value normalised, together with the
// clock-dependent session times, which the tests feed in exactly instead.
const capturedVersion = "v0.30.1-1-g19c79d8"

const (
	aliceID = "b6db7e65-fdaf-5a9b-ad5c-644b892fafd3"
	bobID   = "244f6aef-a7c9-5119-8ae6-7d18f318df5d"
	carolID = "595fe64c-d94c-5100-9761-4545d1563589"

	squatID       = "aaed55bc-3dc4-5ab0-84b9-a2b1a7d2e3ea"
	escapedExerID = "0376c522-0917-5353-b064-fbfc9510d054"
)

var (
	adminNav = NavUser{Username: "alice", IsAdmin: true}
	plainNav = NavUser{Username: "bob"}
)

func readPageGolden(t *testing.T, name string) string {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join("testdata", name+".golden.html"))
	if err != nil {
		t.Fatal(err)
	}
	return strings.ReplaceAll(string(raw), capturedVersion, version.Version())
}

func assertPageGolden(t *testing.T, name, got string) {
	t.Helper()
	if want := readPageGolden(t, name); got != want {
		t.Errorf("%s differs from the Rust golden\ngot:\n%s\nwant:\n%s", name, got, want)
	}
}

func utc(year int, month time.Month, day, hour, minute, second, micro int) time.Time {
	return time.Date(year, month, day, hour, minute, second, micro*1000, time.UTC)
}

func ptr(s string) *string { return &s }

func TestLoginGoldens(t *testing.T) {
	assertPageGolden(t, "login", Login(""))
	assertPageGolden(t, "login-failed", Login("Invalid username or password"))
}

func TestDashboardGoldens(t *testing.T) {
	notes := ptr(`Felt strong <b>today</b> & the bar moved "fast". Don't skip warm-up.`)
	day := func(month time.Month, d int) time.Time { return time.Date(2026, month, d, 0, 0, 0, 0, time.UTC) }
	alice := DashboardView{
		User: adminNav, WorkoutsThisWeek: 3, WorkoutsThisMonth: 10, TotalVolume: 6172.25, TimezoneUnset: true,
		RecentWorkouts: []models.WorkoutSession{
			{ID: "71cfcccf-b0aa-533b-84e4-55839cfbd941", Date: day(11, 16)},
			{ID: "02782218-b94f-5b6c-9a5c-672691802f7e", Date: day(11, 13), Notes: notes},
			{ID: "4592a164-902e-551c-a8e9-a7cf803de5da", Date: day(11, 10)},
			{ID: "36c0871d-a317-5dfa-b3fb-b5972484cbc0", Date: day(11, 7)},
			{ID: "0c9604db-85f8-51ae-bfa0-b6a0d1bf53cd", Date: day(11, 4)},
		},
	}
	bob := DashboardView{
		User: plainNav, WorkoutsThisWeek: 2, WorkoutsThisMonth: 4, TotalVolume: 2262.5,
		RecentWorkouts: []models.WorkoutSession{
			{ID: "c76f5408-51e6-55ce-b637-4c873b32afca", Date: day(11, 15), Notes: ptr("Bob's workout")},
			{ID: "926a7a8f-adfe-531e-8278-5cebd92fd1b2", Date: day(11, 12)},
			{ID: "d063ced1-3a14-5668-95d4-fafe82f94a1f", Date: day(11, 8)},
			{ID: "85eef01d-e4a4-5389-9b26-4e868e92b540", Date: day(11, 4)},
		},
	}
	assertPageGolden(t, "dashboard", Dashboard(alice))
	assertPageGolden(t, "dashboard-timezone-set", Dashboard(bob))
	assertPageGolden(t, "dashboard-empty", Dashboard(DashboardView{User: NavUser{Username: "carol"}}))
}

func TestDashboardShowsWholeVolumeWithoutDecimals(t *testing.T) {
	got := Dashboard(DashboardView{User: plainNav, TotalVolume: 1000})
	if !strings.Contains(got, `<div class="stat-value">1000</div>`) {
		t.Errorf("volume not rendered like Rust's f64 Display: %s", got)
	}
}

func aliceExercises() []models.Exercise {
	return []models.Exercise{
		{ID: "fbe58868-1aa5-5ab6-ad18-9dd398a49b5b", Name: "Bench Press", Category: "chest"},
		{ID: "f2efc5aa-2ac8-54b6-af56-933b6c5be8cf", Name: "Deadlift", Category: "back"},
		{ID: "de28bf00-4cb9-5486-877d-dd7f8f55f010", Name: "Pull-up", Category: "back"},
		{ID: squatID, Name: "Squat", Category: "legs"},
		{ID: "9f62366c-2d2b-5ab4-ab90-a9a93d1b08de", Name: "Overhead Press", Category: "shoulders"},
		{ID: "9264ffa1-b941-569f-88ec-e1f12d75ad74", Name: "Barbell Curl", Category: "arms"},
		{ID: escapedExerID, Name: "Farmer's Walk & <Carry>", Category: "core"},
		{ID: "fac81715-c51c-5bae-a239-e6359a311d8e", Name: "Plank", Category: "core"},
	}
}

func TestExercisesGoldens(t *testing.T) {
	exercises := aliceExercises()
	assertPageGolden(t, "exercises-list", ExercisesList(adminNav, exercises, models.Categories))
	assertPageGolden(t, "exercises-list-empty", ExercisesList(NavUser{Username: "carol"}, nil, models.Categories))
	assertPageGolden(t, "exercises-new", NewExercise(adminNav, models.Categories, ""))
	assertPageGolden(t, "exercises-new-error",
		NewExercise(adminNav, models.Categories, "Exercise name is required"))
	assertPageGolden(t, "exercises-edit", EditExercise(adminNav, exercises[3], models.Categories, ""))
	assertPageGolden(t, "exercises-edit-escaped", EditExercise(adminNav, exercises[6], models.Categories, ""))
}

func TestEditExerciseErrorGolden(t *testing.T) {
	// A failed POST re-renders the stored exercise, not the rejected input.
	assertPageGolden(t, "exercises-edit-error",
		EditExercise(adminNav, aliceExercises()[3], models.Categories, "Exercise name is required"))
}

func seededUsers() []models.UserListItem {
	return []models.UserListItem{
		{ID: carolID, Username: "carol", Role: models.RoleUser, CreatedAt: utc(2026, 3, 1, 9, 0, 0, 0)},
		{ID: bobID, Username: "bob", Role: models.RoleUser, CreatedAt: utc(2026, 2, 10, 9, 0, 0, 0)},
		{ID: aliceID, Username: "alice", Role: models.RoleAdmin, CreatedAt: utc(2026, 1, 5, 9, 0, 0, 0)},
	}
}

func TestUsersGoldens(t *testing.T) {
	assertPageGolden(t, "users", UsersList(UsersListView{User: adminNav, ViewerID: aliceID, Users: seededUsers()}))
	assertPageGolden(t, "users-as-user", UsersList(UsersListView{User: plainNav, ViewerID: bobID, Users: seededUsers()}))
}

func TestUsersListMarksTheViewerAndHidesAdminOnlyPromote(t *testing.T) {
	got := UsersList(UsersListView{User: adminNav, ViewerID: aliceID, Users: seededUsers()})
	if !strings.Contains(got, `<span class="muted">(you)</span>`) {
		t.Error("the viewer's row is not marked (you)")
	}
	if strings.Contains(got, "/users/"+aliceID+"/") {
		t.Error("the viewer got action links for their own row")
	}
}

func TestNewUserGoldens(t *testing.T) {
	assertPageGolden(t, "users-new", NewUser(adminNav, ""))
	assertPageGolden(t, "users-new-weak", NewUser(adminNav,
		"This is a very common password. Add another word or two. Uncommon words are better."))
}

func TestConfirmActionGoldens(t *testing.T) {
	promote := ConfirmActionView{
		User:        adminNav,
		ActionLabel: "Promote to admin",
		Consequence: "bob will gain full administrative access, including the ability to create and delete other users. They will be signed out of every device and must log in again.",
		FormAction:  "/users/" + bobID + "/promote",
	}
	del := ConfirmActionView{
		User:        adminNav,
		ActionLabel: "Delete user",
		Consequence: "bob and all of their workouts, exercises and sessions will be permanently deleted. This cannot be undone.",
		FormAction:  "/users/" + bobID + "/delete",
	}
	assertPageGolden(t, "confirm-promote-user", ConfirmAction(promote))
	assertPageGolden(t, "confirm-delete-user", ConfirmAction(del))

	promote.Error, del.Error = "Password is incorrect", "Password is incorrect"
	assertPageGolden(t, "confirm-promote-user-wrong-password", ConfirmAction(promote))
	assertPageGolden(t, "confirm-delete-user-wrong-password", ConfirmAction(del))
}

func TestConfirmGoldens(t *testing.T) {
	logout := func(user NavUser, consequence string) string {
		return Confirm(ConfirmView{
			User: user, ActionLabel: "Log out other devices", Consequence: consequence,
			FormAction: "/settings/logout-others", CancelURL: "/settings",
		})
	}
	assertPageGolden(t, "confirm-logout-others",
		logout(adminNav, "1 other signed-in device will be logged out. This device stays signed in."))
	assertPageGolden(t, "confirm-logout-others-none",
		logout(plainNav, "No other device is signed in, so nothing will be logged out."))

	assertPageGolden(t, "confirm-exercise-delete-escaped", Confirm(ConfirmView{
		User:        adminNav,
		ActionLabel: "Delete exercise",
		Consequence: "Farmer's Walk & <Carry> will be permanently deleted. If any workout still logs it, the deletion will be refused instead.",
		FormAction:  "/exercises/" + escapedExerID + "/delete",
		CancelURL:   "/exercises",
	}))
}

var timezoneOptionPattern = regexp.MustCompile(`<option value="([^"]*)"></option>`)

// timezoneNamesInGolden reads the datalist back out of the settings golden, so
// the test does not depend on the timezone package.
func timezoneNamesInGolden(t *testing.T) []string {
	t.Helper()
	golden := readPageGolden(t, "settings")
	_, datalist, _ := strings.Cut(golden, `<datalist id="timezone-names">`)
	datalist, _, _ = strings.Cut(datalist, "</datalist>")
	var names []string
	for _, m := range timezoneOptionPattern.FindAllStringSubmatch(datalist, -1) {
		names = append(names, m[1])
	}
	if len(names) < 100 {
		t.Fatalf("only %d timezone names found in the golden", len(names))
	}
	return names
}

func settingsView(t *testing.T) SettingsView {
	t.Helper()
	return SettingsView{
		User:                adminNav,
		TimezoneNames:       timezoneNamesInGolden(t),
		GitVersion:          version.Version(),
		CurrentSessionToken: "current",
		Sessions: []SessionRow{
			{Token: "current", CreatedAt: utc(2026, 11, 15, 8, 0, 0, 0), LastTouchedAt: utc(2026, 11, 16, 16, 40, 2, 314384)},
			{Token: "other", CreatedAt: utc(2026, 11, 10, 8, 0, 0, 0), LastTouchedAt: utc(2026, 11, 10, 8, 0, 0, 0)},
		},
	}
}

func TestSettingsGoldens(t *testing.T) {
	view := settingsView(t)
	assertPageGolden(t, "settings", Settings(view))

	withError := view
	withError.Error = "Unknown timezone. Pick a name such as Asia/Taipei."
	assertPageGolden(t, "settings-timezone-error", Settings(withError))

	mismatch := view
	mismatch.Error = "New passwords do not match"
	assertPageGolden(t, "settings-password-mismatch", Settings(mismatch))

	bob := view
	bob.User = plainNav
	bob.Timezone = "Asia/Taipei"
	bob.Sessions = []SessionRow{{
		Token: "current", CreatedAt: utc(2026, 11, 15, 8, 0, 0, 0), LastTouchedAt: utc(2026, 11, 16, 16, 40, 2, 335204),
	}}
	assertPageGolden(t, "settings-timezone-set", Settings(bob))
}

func TestSettingsShowsTheSuccessMessage(t *testing.T) {
	view := settingsView(t)
	view.Success = "Timezone saved"
	want := `<div class="alert alert-success" style="margin-bottom: var(--sp-5);">Timezone saved</div>`
	if got := Settings(view); !strings.Contains(got, want) {
		t.Errorf("success alert missing in %s", got)
	}
}

func TestSetupRendersTokenField(t *testing.T) {
	got := Setup("Invalid setup token")
	for _, want := range []string{
		`<title>Setup - LiftLog</title>`,
		`<div class="auth-subtitle">Create the first user account to get started</div>`,
		`<div class="error">Invalid setup token</div>`,
		`<form method="post" action="/auth/setup">`,
		`name="setup_token" required autofocus autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="XXXXX-XXXXX"`,
		"Find it on the server's stderr (`docker logs` shows it), on the line starting with &quot;Setup token&quot;.",
		`<input type="password" id="password" name="password" required minlength="12" maxlength="128">`,
		`<button type="submit">Create Account</button>`,
	} {
		if !strings.Contains(got, want) {
			t.Errorf("setup page lacks %q", want)
		}
	}
}

func TestRFC3339MatchesChrono(t *testing.T) {
	for _, tc := range []struct {
		in   time.Time
		want string
	}{
		{utc(2026, 3, 1, 9, 0, 0, 0), "2026-03-01T09:00:00+00:00"},
		{utc(2026, 11, 16, 16, 40, 2, 314384), "2026-11-16T16:40:02.314384+00:00"},
		{time.Date(2026, 1, 2, 3, 4, 5, 120_000_000, time.UTC), "2026-01-02T03:04:05.120+00:00"},
	} {
		if got := rfc3339(tc.in); got != tc.want {
			t.Errorf("rfc3339(%v) = %q, want %q", tc.in, got, tc.want)
		}
	}
}
