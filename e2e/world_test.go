package e2e

import (
	"context"
	"fmt"
	"slices"
	"strconv"
	"strings"
	"sync/atomic"

	"github.com/cucumber/godog"
	messages "github.com/cucumber/messages/go/v34"

	"github.com/henry40408/liftlog/e2e/internal/harness"
)

// scenarioCount hands out the per-scenario suffix.
var scenarioCount atomic.Uint64

// world is the state shared by the steps of one scenario: one browser,
// opened by a Before hook rather than on first use, because only the hook
// sees the scenario's @no-js tag. Every scenario shares one database, so
// fixture names carry a per-scenario suffix (see unique).
type world struct {
	server  *harness.Server
	browser *harness.Browser
	suffix  string

	// workoutID is the workout the scenario created, if it created one.
	workoutID string
	// exerciseID and exerciseName are the exercise the scenario created;
	// the name is the handle most assertions use.
	exerciseID   string
	exerciseName string
	// shareURL is the share path published for the workout, if shared.
	shareURL string
	// otherUser is the other account a users scenario acts on.
	otherUser string
}

// initializeScenario runs once per scenario, so every scenario gets its own
// world through the closure.
func initializeScenario(server *harness.Server) func(*godog.ScenarioContext) {
	return func(sc *godog.ScenarioContext) {
		w := &world{server: server, suffix: strconv.FormatUint(scenarioCount.Add(1), 16)}
		sc.Before(w.openBrowser)
		sc.After(w.closeBrowser)
		registerSteps(sc, w)
	}
}

func (w *world) openBrowser(ctx context.Context, scenario *godog.Scenario) (context.Context, error) {
	browser, err := harness.Open(w.server, scriptingFor(scenario))
	if err != nil {
		return ctx, fmt.Errorf("could not open a browser session: %w", err)
	}
	w.browser = browser
	return ctx, nil
}

// closeBrowser runs on every outcome, so a failed scenario leaves no Chrome.
func (w *world) closeBrowser(ctx context.Context, _ *godog.Scenario, _ error) (context.Context, error) {
	if w.browser != nil {
		w.browser.Close()
		w.browser = nil
	}
	return ctx, nil
}

// scriptingFor reads @no-js. Pickles carry their feature's tags too, so the
// tag works on a feature or a scenario.
func scriptingFor(scenario *godog.Scenario) harness.Scripting {
	if hasTag(scenario, "@no-js") {
		return harness.ScriptingDisabled
	}
	return harness.ScriptingEnabled
}

func hasTag(scenario *godog.Scenario, name string) bool {
	return slices.ContainsFunc(scenario.Tags, func(tag *messages.PickleTag) bool {
		return tag.Name == name
	})
}

// unique is a name no other scenario will produce.
func (w *world) unique(prefix string) string {
	return prefix + "-" + w.suffix
}

// browserOf is the scenario's browser, for steps that run after the hook.
func (w *world) browserOf() (*harness.Browser, error) {
	if w.browser == nil {
		return nil, fmt.Errorf("no browser session: the Before hook did not open one")
	}
	return w.browser, nil
}

func (w *world) workout() (harness.WorkoutPage, error) {
	if w.workoutID == "" {
		return harness.WorkoutPage{}, fmt.Errorf("this step needs a workout, and the scenario has not created one")
	}
	b, err := w.browserOf()
	if err != nil {
		return harness.WorkoutPage{}, err
	}
	return b.Workout(w.workoutID), nil
}

func (w *world) requireExercise() error {
	if w.exerciseName == "" {
		return fmt.Errorf("this step needs an exercise, and the scenario has not created one")
	}
	return nil
}

func (w *world) requireShare() error {
	if w.shareURL == "" {
		return fmt.Errorf("this step needs a shared workout, and the scenario has not shared one")
	}
	return nil
}

func (w *world) requireOtherUser() error {
	if w.otherUser == "" {
		return fmt.Errorf("this step needs another user, and the scenario has not named one")
	}
	return nil
}

// path polls the browser's current path, for EventuallyEq.
func (w *world) path() (string, error) {
	b, err := w.browserOf()
	if err != nil {
		return "", err
	}
	return b.Path()
}

// urlIs waits for the browser to be on path.
func (w *world) urlIs(path string) error {
	return harness.EventuallyEq("the URL", path, w.path)
}

// status is what path answers with for the holder of the browser's cookie.
func (w *world) status(path string) (int, error) {
	b, err := w.browserOf()
	if err != nil {
		return 0, err
	}
	session, err := b.SessionCookie()
	if err != nil {
		return 0, err
	}
	response, err := w.server.Get(path, session)
	return response.Status, err
}

// eventuallyContains waits for a banner or label to mention needle; a missing
// element counts as no match.
func eventuallyContains(what, needle string, read func() (string, bool, error)) error {
	return harness.Eventually(fmt.Sprintf("%s mentions `%s`", what, needle), func() (bool, error) {
		text, found, err := read()
		return found && strings.Contains(text, needle), err
	})
}
