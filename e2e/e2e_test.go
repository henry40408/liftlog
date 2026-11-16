package e2e

import (
	"fmt"
	"os"
	"runtime"
	"strings"
	"testing"

	"github.com/cucumber/godog"

	"github.com/henry40408/liftlog/e2e/internal/harness"
)

// ConcurrencyCeiling is the most scenarios, and so browsers, to run at once,
// whatever the machine: more browsers than cores makes pages settle slower
// than the steps wait.
const ConcurrencyCeiling = 4

const defaultFeatures = "features"

// pass is one godog run over a set of scenarios.
type pass struct {
	name string
	tags string
}

// TestE2E is the suite: two sequential passes over one server, @bootstrap
// first on the empty database, then everything else. @no-js scenarios run
// with scripting disabled (see scriptingFor).
func TestE2E(t *testing.T) {
	server, err := harness.Start()
	if err != nil {
		t.Fatal(err)
	}
	defer server.Stop()

	if err := preflight(server); err != nil {
		t.Fatal(err)
	}

	passes := []pass{
		{name: "bootstrap", tags: "@bootstrap"},
		{name: "rest", tags: "~@bootstrap"},
	}
	// Both passes run before either can fail the test.
	for _, p := range passes {
		if status := run(server, p); status != 0 {
			t.Errorf("pass %q failed with status %d", p.name, status)
		}
	}
}

// preflight opens and closes one browser, so a missing Chromium gives one
// clear error rather than one per scenario.
func preflight(server *harness.Server) error {
	fmt.Fprintf(os.Stderr, "e2e: browser: %s\n", harness.BrowserChoice())
	browser, err := harness.Open(server, harness.ScriptingEnabled)
	if err != nil {
		return err
	}
	browser.Close()
	return nil
}

func run(server *harness.Server, p pass) int {
	suite := godog.TestSuite{
		Name:                p.name,
		ScenarioInitializer: initializeScenario(server),
		Options: &godog.Options{
			Format: "pretty",
			// ANSI codes make CI logs unreadable.
			NoColors:    os.Getenv("CI") != "",
			Output:      os.Stdout,
			Paths:       featurePaths(),
			Tags:        p.tags,
			Concurrency: min(runtime.NumCPU(), ConcurrencyCeiling),
			// Undefined and pending steps fail rather than skip.
			Strict: true,
		},
	}
	return suite.Run()
}

// featurePaths is E2E_FEATURES, a comma-separated list of feature paths, for
// running one file locally. It defaults to every feature.
func featurePaths() []string {
	value := os.Getenv("E2E_FEATURES")
	if value == "" {
		return []string{defaultFeatures}
	}
	return strings.Split(value, ",")
}
