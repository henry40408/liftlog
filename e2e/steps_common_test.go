package e2e

import (
	"fmt"

	"github.com/cucumber/godog"

	"github.com/henry40408/liftlog/e2e/internal/harness"
)

// quoted captures the inside of a double-quoted step argument.
const quoted = `"([^"]*)"`

// registerSteps wires every step to the scenario. A step returns its failure;
// godog reports it against the step. Assertions retry through
// harness.Eventually, since most follow a form post still in flight.
func registerSteps(sc *godog.ScenarioContext, w *world) {
	registerCommonSteps(sc, w)
	registerAuthSteps(sc, w)
	registerDashboardSteps(sc, w)
	registerExerciseSteps(sc, w)
	registerWorkoutSteps(sc, w)
	registerSetSteps(sc, w)
	registerSettingsSteps(sc, w)
	registerSharingSteps(sc, w)
	registerStatsSteps(sc, w)
	registerUsersSteps(sc, w)
	registerConfirmSteps(sc, w)
}

func registerCommonSteps(sc *godog.ScenarioContext, w *world) {
	sc.Step(`^I visit `+quoted+`$`, w.visit)
	sc.Step(`^the URL is `+quoted+`$`, w.urlIs)
	sc.Step(`^visiting the workout I created returns a 404$`, w.myWorkoutIs404)
	sc.Step(`^visiting `+quoted+` returns a 404$`, func(path string) error { return w.statusIs(path, 404) })
	sc.Step(`^visiting `+quoted+` returns a 403$`, func(path string) error { return w.statusIs(path, 403) })
}

func (w *world) visit(path string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return b.Goto(path)
}

func (w *world) myWorkoutIs404() error {
	if w.workoutID == "" {
		return fmt.Errorf("this step needs a workout, and the scenario has not created one")
	}
	return w.statusIs("/workouts/"+w.workoutID, 404)
}

func (w *world) statusIs(path string, want int) error {
	got, err := w.status(path)
	if err != nil {
		return err
	}
	if got != want {
		return fmt.Errorf("%s answered %d, not %d", path, got, want)
	}
	return nil
}

func registerAuthSteps(sc *godog.ScenarioContext, w *world) {
	sc.Step(`^a user `+quoted+` with password `+quoted+` exists$`, w.userExists)
	sc.Step(`^I am logged in as `+quoted+`$`, w.loggedInAs)
	sc.Step(`^I am logged in as `+quoted+` with password `+quoted+`$`, w.signIn)
	sc.Step(`^I log in as `+quoted+` with password `+quoted+`$`, w.logIn)
	sc.Step(`^I log out$`, w.logOut)
	sc.Step(`^I am logged in as a fresh non-admin user$`, w.loggedInAsFreshUser)
	sc.Step(`^I switch to a fresh non-admin user$`, w.switchToFreshUser)
	sc.Step(`^I see the dashboard$`, w.seeDashboard)
	sc.Step(`^I see the login page$`, w.seeLoginPage)
	sc.Step(`^I see the setup page$`, w.seeSetupPage)
	sc.Step(`^I see the login error `+quoted+`$`, w.seeLoginError)
	sc.Step(`^I submit the setup form with username `+quoted+` and password `+quoted+`$`, w.submitSetup)
	sc.Step(`^I submit the setup form with username `+quoted+` and password `+quoted+` and a wrong token$`, w.submitSetupWrongToken)
	sc.Step(`^I see the setup error `+quoted+`$`, w.seeSetupError)
}

func (w *world) userExists(username, password string) error {
	return w.server.EnsureUser(username, password)
}

func (w *world) loggedInAs(username string) error {
	return w.signIn(username, harness.Password)
}

func (w *world) logIn(username, password string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return b.Login().Login(username, password)
}

func (w *world) logOut() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return b.Nav().SignOut()
}

func (w *world) loggedInAsFreshUser() error {
	return w.signIn(w.unique("member"), harness.Password)
}

func (w *world) switchToFreshUser() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := b.ClearCookies(); err != nil {
		return err
	}
	return w.signIn(w.unique("other"), harness.Password)
}

func (w *world) seeDashboard() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.urlIs("/"); err != nil {
		return err
	}
	return harness.Eventually("the dashboard heading is showing", b.Dashboard().IsShowing)
}

func (w *world) seeLoginPage() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.urlIs("/auth/login"); err != nil {
		return err
	}
	return harness.Eventually("the login form is showing", b.Login().IsShowing)
}

func (w *world) seeSetupPage() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.urlIs("/auth/setup"); err != nil {
		return err
	}
	return harness.Eventually("the setup form is showing", b.Setup().IsShowing)
}

func (w *world) seeLoginError(message string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return eventuallyContains("the login error", message, b.Login().Error)
}

func (w *world) submitSetup(username, password string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return b.Setup().Submit(username, password)
}

func (w *world) submitSetupWrongToken(username, password string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	return b.Setup().SubmitWithToken(username, password, "ZZZZZ-ZZZZZ")
}

func (w *world) seeSetupError(message string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.urlIs("/auth/setup"); err != nil {
		return err
	}
	return eventuallyContains("the setup error", message, b.Setup().Error)
}

// signIn signs in through the UI and waits for the dashboard, creating the
// account first so a scenario can name a user without a Given for it.
func (w *world) signIn(username, password string) error {
	if err := w.server.EnsureUser(username, password); err != nil {
		return err
	}
	if err := w.logIn(username, password); err != nil {
		return err
	}
	return w.seeDashboard()
}

func registerDashboardSteps(sc *godog.ScenarioContext, w *world) {
	sc.Step(`^the dashboard lists the workout I created in Recent Workouts$`, w.dashboardListsWorkout)
	sc.Step(`^the dashboard actions sit above the summary and the workout list$`, w.dashboardActionsLead)
	sc.Step(`^the dashboard `+quoted+` count is (\d+)$`, w.dashboardCount)
}

func (w *world) dashboardListsWorkout() error {
	if w.workoutID == "" {
		return fmt.Errorf("this step needs a workout, and the scenario has not created one")
	}
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	dashboard := b.Dashboard()
	if err := dashboard.Goto(); err != nil {
		return err
	}
	if err := harness.Eventually("the Recent Workouts heading is showing", dashboard.HasRecentHeading); err != nil {
		return err
	}
	return harness.Eventually("the workout is on the dashboard", func() (bool, error) {
		return dashboard.ListsWorkout(w.workoutID)
	})
}

func (w *world) dashboardActionsLead() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	dashboard := b.Dashboard()
	if err := dashboard.Goto(); err != nil {
		return err
	}
	return harness.Eventually("the quick actions render first", dashboard.ActionsLead)
}

func (w *world) dashboardCount(label string, count int) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	dashboard := b.Dashboard()
	if err := dashboard.Goto(); err != nil {
		return err
	}
	return harness.EventuallyEq(fmt.Sprintf("the `%s` card", label), fmt.Sprint(count), func() (string, error) {
		return dashboard.Stat(label)
	})
}
