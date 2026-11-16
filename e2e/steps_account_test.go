package e2e

import (
	"fmt"
	"regexp"
	"slices"
	"strconv"
	"strings"

	"github.com/cucumber/godog"

	"github.com/henry40408/liftlog/e2e/internal/harness"
)

func registerSettingsSteps(sc *godog.ScenarioContext, w *world) {
	sc.Step(`^I change my password from `+quoted+` to `+quoted+`$`, w.changePassword)
	sc.Step(`^I submit the password form with current `+quoted+`, new `+quoted+`, confirm `+quoted+`$`, w.submitPasswordForm)
	sc.Step(`^I see a password-change success message$`, w.seePasswordSuccess)
	sc.Step(`^I see a settings error `+quoted+`$`, w.seeSettingsError)
	sc.Step(`^I have a second session as `+quoted+`$`, w.secondSession)
	sc.Step(`^I log out all other devices$`, w.logOutOthers)
	sc.Step(`^the active sessions table has (\d+) rows?$`, w.sessionsRowCount)
	sc.Step(`^the active sessions table marks my current device$`, w.sessionsMarkCurrent)
	sc.Step(`^the active sessions table labels every cell$`, w.sessionsLabelCells)
	sc.Step(`^the settings page in timezone `+quoted+` shows session times ending with `+quoted+` for `+quoted+` with password `+quoted+`$`, w.sessionsInTimezone)
}

func (w *world) changePassword(current, next string) error {
	return w.submitPasswordForm(current, next, next)
}

func (w *world) submitPasswordForm(current, next, confirm string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return b.Settings().ChangePassword(harness.PasswordChange{Current: current, New: next, Confirm: confirm})
}

func (w *world) seePasswordSuccess() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return eventuallyContains("the success banner", "Password changed successfully", b.Settings().Success)
}

func (w *world) seeSettingsError(message string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return eventuallyContains("the settings error", message, b.Settings().Error)
}

func (w *world) secondSession(username string) error {
	return w.server.OpenSecondSession(username, harness.Password)
}

func (w *world) logOutOthers() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	settings := b.Settings()
	if err := settings.LogOutOtherDevices(); err != nil {
		return err
	}
	// The POST re-renders /settings, so wait for the banner.
	return eventuallyContains("the log-out-others banner", "Logged out of all other devices.", settings.Success)
}

func (w *world) sessionsRowCount(count int) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	settings := b.Settings()
	if err := settings.Goto(); err != nil {
		return err
	}
	return harness.EventuallyEq("the active sessions row count", count, settings.SessionCount)
}

func (w *world) sessionsMarkCurrent() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	settings := b.Settings()
	if err := settings.Goto(); err != nil {
		return err
	}
	return harness.Eventually("a row is marked as this device", settings.MarksCurrentDevice)
}

func (w *world) sessionsLabelCells() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	settings := b.Settings()
	if err := settings.Goto(); err != nil {
		return err
	}
	want := []string{"Device", "Last active", "Signed in"}
	return harness.Eventually("the session cell labels are "+strings.Join(want, ", "), func() (bool, error) {
		labels, err := settings.FirstRowLabels()
		return slices.Equal(labels, want), err
	})
}

func (w *world) sessionsInTimezone(timezone, suffix, username, password string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.server.EnsureUser(username, password); err != nil {
		return err
	}
	if err := b.SetTimezone(timezone); err != nil {
		return err
	}
	if err := b.Login().Login(username, password); err != nil {
		return err
	}
	if err := w.seeDashboard(); err != nil {
		return err
	}
	settings := b.Settings()
	if err := settings.Goto(); err != nil {
		return err
	}
	probe := func() (int, error) {
		times, err := settings.FirstRowTimes()
		return len(times), err
	}
	if err := harness.EventuallyEq("the number of timestamps in the first row", 2, probe); err != nil {
		return err
	}
	return harness.Eventually("both timestamps are rendered in "+timezone, func() (bool, error) {
		times, err := settings.FirstRowTimes()
		return len(times) > 0 && allLocalTimestamps(times, suffix), err
	})
}

// localTimestamp is the fixed YYYY-MM-DD HH:MM GMT±H shape base.html renders,
// not locale-dependent.
var localTimestamp = regexp.MustCompile(`^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$`)

func allLocalTimestamps(times []string, suffix string) bool {
	return slices.IndexFunc(times, func(text string) bool {
		rest, found := strings.CutSuffix(text, suffix)
		return !found || !localTimestamp.MatchString(strings.TrimRight(rest, " "))
	}) < 0
}

func registerSharingSteps(sc *godog.ScenarioContext, w *world) {
	sc.Step(`^I share the workout$`, w.publishShare)
	sc.Step(`^I have shared the workout$`, w.publishShare)
	sc.Step(`^I revoke the share$`, w.revokeShare)
	sc.Step(`^a public share link is shown on the workout page$`, w.shareLinkIsShown)
	sc.Step(`^a guest can view the workout via the share URL$`, w.guestCanView)
	sc.Step(`^a guest visiting the share URL gets a 404$`, w.guestGets404)
}

// publishShare publishes a share link and remembers the path it produced.
func (w *world) publishShare() error {
	workout, err := w.workout()
	if err != nil {
		return err
	}
	if err := workout.Goto(); err != nil {
		return err
	}
	if err := workout.Share(); err != nil {
		return err
	}
	if err := harness.Eventually("a share link is on the page", func() (bool, error) {
		_, found, err := workout.ShareURL()
		return found, err
	}); err != nil {
		return err
	}
	w.shareURL, _, err = workout.ShareURL()
	return err
}

func (w *world) revokeShare() error {
	workout, err := w.workout()
	if err != nil {
		return err
	}
	if err := workout.Goto(); err != nil {
		return err
	}
	if err := workout.RevokeShare(); err != nil {
		return err
	}
	if err := w.onTheWorkout(); err != nil {
		return err
	}
	return harness.Eventually("the share block is gone", func() (bool, error) {
		has, err := workout.HasShareInfo()
		return !has, err
	})
}

var sharePath = regexp.MustCompile(`^/shared/[A-Za-z0-9_-]+$`)

func (w *world) shareLinkIsShown() error {
	workout, err := w.workout()
	if err != nil {
		return err
	}
	if err := workout.Goto(); err != nil {
		return err
	}
	if err := harness.Eventually("a share link is on the page", func() (bool, error) {
		_, found, err := workout.ShareURL()
		return found, err
	}); err != nil {
		return err
	}
	if !sharePath.MatchString(w.shareURL) {
		return fmt.Errorf("`%s` is not a share path", w.shareURL)
	}
	return nil
}

// Guests go over plain HTTP: the shared page has no scripts, and the revoked
// case needs the status code.
func (w *world) guestCanView() error {
	if err := w.requireShare(); err != nil {
		return err
	}
	response, err := w.server.Get(w.shareURL, "")
	if err != nil {
		return err
	}
	if response.Status != 200 {
		return fmt.Errorf("a guest got %d from the share URL", response.Status)
	}
	if !strings.Contains(response.Body, "Shared by") {
		return fmt.Errorf("the shared page does not name who shared it")
	}
	if !strings.Contains(response.Body, "set-row") {
		return fmt.Errorf("the shared page shows no sets")
	}
	return nil
}

func (w *world) guestGets404() error {
	if err := w.requireShare(); err != nil {
		return err
	}
	response, err := w.server.Get(w.shareURL, "")
	if err != nil {
		return err
	}
	if response.Status != 404 {
		return fmt.Errorf("a guest got %d from the revoked share URL", response.Status)
	}
	return nil
}

func registerStatsSteps(sc *godog.ScenarioContext, w *world) {
	sc.Step(`^I see the stats overview$`, w.seeStatsOverview)
	sc.Step(`^I see exercise-specific stats for the exercise I created$`, w.seeExerciseStats)
	sc.Step(`^the PR list shows my exercise$`, w.prListShowsExercise)
	sc.Step(`^the PR list shows (\d+) for my exercise in both the all-time and 1-month columns$`, w.prListShowsWeight)
}

func (w *world) seeStatsOverview() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	stats := b.Stats()
	if err := stats.GotoOverview(); err != nil {
		return err
	}
	return harness.Eventually("the stats overview is showing", stats.OverviewIsShowing)
}

func (w *world) seeExerciseStats() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.requireExercise(); err != nil {
		return err
	}
	stats := b.Stats()
	if err := stats.GotoExercise(w.exerciseID); err != nil {
		return err
	}
	if err := harness.EventuallyEq("the exercise heading", w.exerciseName, stats.ExerciseHeading); err != nil {
		return err
	}
	return harness.Eventually("the progress chart is drawn", stats.HasChart)
}

func (w *world) prListShowsExercise() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.requireExercise(); err != nil {
		return err
	}
	stats := b.Stats()
	if err := stats.GotoPRs(); err != nil {
		return err
	}
	return harness.EventuallyEq("PR rows for my exercise", 1, func() (int, error) {
		return stats.PRRowsFor(w.exerciseName)
	})
}

func (w *world) prListShowsWeight(weight int) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.requireExercise(); err != nil {
		return err
	}
	stats := b.Stats()
	if err := stats.GotoPRs(); err != nil {
		return err
	}
	// Just logged, so the rolling 1-month window carries the same number.
	for _, column := range []string{"PR (All)", "PR (1M)"} {
		err := harness.EventuallyEq("the "+column+" column", strconv.Itoa(weight), func() (string, error) {
			return stats.PRCell(w.exerciseName, column)
		})
		if err != nil {
			return err
		}
	}
	return nil
}

func registerUsersSteps(sc *godog.ScenarioContext, w *world) {
	sc.Step(`^another user exists$`, w.anotherUserExists)
	sc.Step(`^I create a new user via the admin UI$`, w.createUserViaUI)
	sc.Step(`^I promote that user to admin$`, func() error { return w.confirmUserAction("Promote", "Promote to admin") })
	sc.Step(`^I delete that user$`, func() error { return w.confirmUserAction("Delete", "Delete user") })
	sc.Step(`^I open the delete confirmation for that user$`, w.openDeleteConfirmation)
	sc.Step(`^I confirm with the wrong password$`, w.confirmWithWrongPassword)
	sc.Step(`^I see a confirmation error$`, w.seeConfirmationError)
	sc.Step(`^I see that user listed on the users page$`, w.userIsListed)
	sc.Step(`^I see that user listed as Admin$`, w.userIsAdmin)
	sc.Step(`^I do not see that user on the users page$`, w.userIsNotListed)
	sc.Step(`^I do not see the `+quoted+` button on the users page$`, w.noSuchButton)
	sc.Step(`^the users page does not let me delete my own account$`, w.cannotDeleteMyself)
}

func (w *world) anotherUserExists() error {
	username := w.unique("subject")
	if err := w.server.EnsureUser(username, harness.Password); err != nil {
		return err
	}
	w.otherUser = username
	return nil
}

func (w *world) createUserViaUI() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	username := w.unique("newbie")
	if err := b.Users().Create(username, harness.Password); err != nil {
		return err
	}
	if err := w.urlIs("/users"); err != nil {
		return err
	}
	w.otherUser = username
	return nil
}

// confirmUserAction opens a users-page row action, confirms it with the
// admin's password, and waits for the list to come back.
func (w *world) confirmUserAction(link, button string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.requireOtherUser(); err != nil {
		return err
	}
	if err := b.Users().OpenAction(w.otherUser, link); err != nil {
		return err
	}
	if err := b.ConfirmAction().Confirm(harness.Password, button); err != nil {
		return err
	}
	return w.urlIs("/users")
}

func (w *world) openDeleteConfirmation() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.requireOtherUser(); err != nil {
		return err
	}
	return b.Users().OpenAction(w.otherUser, "Delete")
}

func (w *world) confirmWithWrongPassword() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return b.ConfirmAction().Confirm("definitely-not-it", "Delete user")
}

func (w *world) seeConfirmationError() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return eventuallyContains("the confirmation error", "Password is incorrect", b.ConfirmAction().Error)
}

func (w *world) userIsListed() error {
	return w.userRows(1)
}

func (w *world) userIsNotListed() error {
	return w.userRows(0)
}

func (w *world) userRows(want int) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.requireOtherUser(); err != nil {
		return err
	}
	users := b.Users()
	if err := users.Goto(); err != nil {
		return err
	}
	return harness.EventuallyEq("rows for that user", want, func() (int, error) {
		return users.RowsFor(w.otherUser)
	})
}

func (w *world) userIsAdmin() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.requireOtherUser(); err != nil {
		return err
	}
	users := b.Users()
	if err := users.Goto(); err != nil {
		return err
	}
	return harness.Eventually("that user's row says admin", func() (bool, error) {
		text, err := users.RowText(w.otherUser)
		return strings.Contains(strings.ToLower(text), "admin"), err
	})
}

func (w *world) noSuchButton(label string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	users := b.Users()
	if err := users.Goto(); err != nil {
		return err
	}
	return harness.EventuallyEq(fmt.Sprintf("`%s` controls", label), 0, func() (int, error) {
		return users.LinksLabelled(label)
	})
}

func (w *world) cannotDeleteMyself() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	users := b.Users()
	if err := users.Goto(); err != nil {
		return err
	}
	err = harness.Eventually("my own row is marked as mine", func() (bool, error) {
		return users.MarksAsYou(harness.AdminUsername)
	})
	if err != nil {
		return err
	}
	return harness.EventuallyEq("delete links on my own row", 0, func() (int, error) {
		return users.DeleteLinksFor(harness.AdminUsername)
	})
}

func registerConfirmSteps(sc *godog.ScenarioContext, w *world) {
	sc.Step(`^I click Delete on my exercise$`, w.clickDeleteExercise)
	sc.Step(`^I click Delete on the workout$`, w.clickDeleteWorkout)
	sc.Step(`^I am asked to confirm$`, w.askedToConfirm)
	sc.Step(`^I confirm the action$`, w.confirmAction)
	sc.Step(`^I cancel the action$`, w.cancelAction)
	sc.Step(`^I am on the exercises page$`, func() error { return w.urlIs("/exercises") })
}

func (w *world) clickDeleteExercise() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.requireExercise(); err != nil {
		return err
	}
	return b.Exercises().ClickDelete(w.exerciseName)
}

func (w *world) clickDeleteWorkout() error {
	workout, err := w.workout()
	if err != nil {
		return err
	}
	if err := workout.Goto(); err != nil {
		return err
	}
	return workout.Delete()
}

func (w *world) askedToConfirm() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	path, err := b.Confirm().Path()
	if err != nil {
		return err
	}
	if !strings.HasSuffix(path, "/delete") {
		return fmt.Errorf("expected a delete confirmation page, got %s", path)
	}
	return nil
}

func (w *world) confirmAction() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return b.Confirm().Accept()
}

func (w *world) cancelAction() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return b.Confirm().Cancel()
}
