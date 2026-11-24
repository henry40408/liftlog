// Command screenshots regenerates the screenshots README.md embeds, into
// ../screenshots/:
//
//	cd e2e && go run ./cmd/screenshots
//
// The lifter and sixteen weeks of training are written through the app's own
// endpoints, and the web fonts the pages ask a CDN for are blocked, so the
// pictures need no network and read the same on every machine. Dates are
// counted back from the day the command runs: a run on another day moves the
// date labels, and nothing else. Font rendering and the Chrome version differ
// by machine: an unrelated image changing in the diff means the browser
// changed, not the page.
package main

import (
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/henry40408/liftlog/e2e/internal/harness"
)

// fontHost serves the web fonts the layout links to. Blocking it keeps a
// machine's connection from deciding which font a picture is drawn in.
const fontHost = "fonts.bunny.net"

const (
	monitorWidth = 1280
	// phoneWidth is the iPhone 14's.
	phoneWidth = 390
)

// settle is how long a page gets once its animations are at their last frame,
// for it to be drawn.
const settle = 200 * time.Millisecond

// finishAnimations jumps every CSS animation to its last frame. The accent bar
// and the PR badges loop forever, and the page fades in; a picture taken
// mid-way would differ from the next run's.
const finishAnimations = `(() => {
	const style = document.createElement('style');
	style.textContent = '*, ::before, ::after { animation-duration: 1ms !important; ' +
		'animation-delay: 0s !important; animation-iteration-count: 1 !important; ' +
		'animation-fill-mode: forwards !important; }';
	document.head.append(style);
	return true;
})()`

// screen is a page at a viewport, written to <name>.png.
type screen struct {
	name     string
	path     func(demoRoutes) string
	viewport harness.Viewport
}

// The heights end each picture where its page is not cut through an element.
var screens = []screen{
	{"web-dashboard", func(demoRoutes) string { return "/" }, harness.Viewport{Width: monitorWidth, Height: 900}},
	{"web-progress", func(r demoRoutes) string { return r.progress }, harness.Viewport{Width: monitorWidth, Height: 1210}},
	{"web-dashboard-phone", func(demoRoutes) string { return "/" }, harness.Viewport{Width: phoneWidth, Height: 1280, Mobile: true}},
	{"web-progress-phone", func(r demoRoutes) string { return r.progress }, harness.Viewport{Width: phoneWidth, Height: 760, Mobile: true}},
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "screenshots:", err)
		os.Exit(1)
	}
}

func run() error {
	output := filepath.Join(harness.RepoRoot(), "screenshots")
	if err := os.MkdirAll(output, 0o755); err != nil {
		return err
	}
	server, err := harness.Start()
	if err != nil {
		return err
	}
	defer server.Stop()

	routes, err := seedDemo(server)
	if err != nil {
		return err
	}
	if err := capture(server, routes, output); err != nil {
		return err
	}
	fmt.Printf("screenshots: wrote %d images to %s\n", len(screens), output)
	return nil
}

// capture signs in on a browser of its own and shoots every screen.
func capture(server *harness.Server, routes demoRoutes, output string) error {
	browser, err := harness.Open(server, harness.ScriptingEnabled)
	if err != nil {
		return err
	}
	defer browser.Close()
	if err := prepare(browser); err != nil {
		return err
	}
	if err := browser.Login().Goto(); err != nil {
		return err
	}
	if err := browser.Login().Login(harness.AdminUsername, harness.Password); err != nil {
		return err
	}
	for _, shot := range screens {
		if err := take(browser, shot, routes, output); err != nil {
			return err
		}
	}
	return nil
}

func prepare(browser *harness.Browser) error {
	if err := browser.SetTimezone(demoTimezone); err != nil {
		return err
	}
	return browser.BlockHost(fontHost)
}

func take(browser *harness.Browser, shot screen, routes demoRoutes, output string) error {
	if err := browser.SetViewport(shot.viewport); err != nil {
		return err
	}
	if err := browser.Goto(shot.path(routes)); err != nil {
		return err
	}
	if err := browser.WaitForLoad(); err != nil {
		return err
	}
	if _, err := harness.Eval[bool](browser, finishAnimations); err != nil {
		return err
	}
	time.Sleep(settle)
	return browser.Screenshot(filepath.Join(output, shot.name+".png"))
}
