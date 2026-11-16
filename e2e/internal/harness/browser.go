package harness

import (
	"context"
	"encoding/json"
	"fmt"
	"net/url"
	"os"
	"time"

	"github.com/chromedp/cdproto/cdp"
	"github.com/chromedp/cdproto/emulation"
	"github.com/chromedp/cdproto/network"
	"github.com/chromedp/cdproto/page"
	"github.com/chromedp/chromedp"
)

// ScenarioTimeout bounds one scenario, so a wedged Chrome cannot hang the run.
const ScenarioTimeout = 3 * time.Minute

// The width stays above the 480px breakpoint where .data-table collapses into
// cards, which the active-sessions scenarios depend on.
const (
	windowWidth  = 1280
	windowHeight = 720
	closeTimeout = 10 * time.Second
)

// ChromeEnv names the variable that pins the browser binary.
const ChromeEnv = "E2E_CHROME"

// Scripting says whether the page's own scripts run: the @no-js split.
type Scripting bool

const (
	// ScriptingEnabled is the scripted path.
	ScriptingEnabled Scripting = true
	// ScriptingDisabled is the @no-js path: the page's own scripts never run.
	ScriptingDisabled Scripting = false
)

// Browser is a headless Chrome of its own, scoped to one scenario.
//
// One process per scenario isolates cookies, and lets @no-js disable
// scripting before the first document exists.
type Browser struct {
	server *Server
	ctx    context.Context
	cancel context.CancelFunc
}

// Open starts a headless Chrome with the page's scripts on or off.
func Open(server *Server, scripting Scripting) (*Browser, error) {
	timeoutCtx, cancelTimeout := context.WithTimeout(context.Background(), ScenarioTimeout)
	allocCtx, cancelAlloc := chromedp.NewExecAllocator(timeoutCtx, allocatorOptions()...)
	tabCtx, cancelTab := chromedp.NewContext(allocCtx)
	browser := &Browser{
		server: server,
		ctx:    tabCtx,
		cancel: func() {
			cancelTab()
			cancelAlloc()
			cancelTimeout()
		},
	}

	if err := browser.start(scripting); err != nil {
		browser.cancel()
		return nil, err
	}
	return browser, nil
}

// BrowserChoice says which browser Open will start, for the run's log.
func BrowserChoice() string {
	if path := os.Getenv(ChromeEnv); path != "" {
		return ChromeEnv + "=" + path
	}
	return "chromedp's default search (" + ChromeEnv + " is unset)"
}

func allocatorOptions() []chromedp.ExecAllocatorOption {
	options := append([]chromedp.ExecAllocatorOption{}, chromedp.DefaultExecAllocatorOptions[:]...)
	options = append(options, chromedp.WindowSize(windowWidth, windowHeight))
	// chromedp's own search prefers a chromium on PATH over google-chrome, so
	// CI pins the binary it reports.
	if path := os.Getenv(ChromeEnv); path != "" {
		options = append(options, chromedp.ExecPath(path))
	}
	return options
}

func (b *Browser) start(scripting Scripting) error {
	if err := chromedp.Do(b.ctx); err != nil {
		return fmt.Errorf("could not start a browser (%s): a local Chromium or Chrome is required "+
			"(`brew install --cask ungoogled-chromium`), or set %s to its path: %w", BrowserChoice(), ChromeEnv, err)
	}
	b.acceptDialogs()
	if err := b.resetViewport(); err != nil {
		return err
	}
	if scripting == ScriptingEnabled {
		return nil
	}
	return send(b, emulation.SetScriptExecutionDisabled, emulation.SetScriptExecutionDisabledParams{Value: true})
}

// acceptDialogs auto-accepts the window.confirm() that the page raises for
// a[data-confirm]; otherwise the click that raised it never returns.
func (b *Browser) acceptDialogs() {
	opened := chromedp.Events(b.ctx, page.JavascriptDialogOpening)
	go func() {
		for _, err := range opened {
			if err != nil {
				return
			}
			_, _ = chromedp.Call(b.ctx, page.HandleJavaScriptDialog, page.HandleJavaScriptDialogParams{Accept: true})
		}
	}()
}

// Close ends the browser, and the profile directory goes with it.
func (b *Browser) Close() {
	closing, cancel := context.WithTimeout(b.ctx, closeTimeout)
	defer cancel()
	_ = chromedp.Cancel(closing)
	b.cancel()
}

// resetViewport sets the viewport to exactly the window size: the window size
// flag alone leaves it shorter than 720 px.
func (b *Browser) resetViewport() error {
	return send(b, emulation.SetDeviceMetricsOverride, emulation.SetDeviceMetricsOverrideParams{
		Width:             windowWidth,
		Height:            windowHeight,
		DeviceScaleFactor: 1,
	})
}

// SetTimezone pins the browser's timezone on the live session.
func (b *Browser) SetTimezone(timezone string) error {
	return send(b, emulation.SetTimezoneOverride, emulation.SetTimezoneOverrideParams{TimezoneID: timezone})
}

// ClearCookies drops every cookie, which is how a scenario becomes a different
// user.
func (b *Browser) ClearCookies() error {
	_, err := chromedp.Call(b.ctx, network.ClearBrowserCookies, cdp.Empty{})
	return err
}

// SessionCookie is the session cookie the browser holds, empty when it is not
// signed in. Plain HTTP, so not the __Host- variant.
func (b *Browser) SessionCookie() (string, error) {
	result, err := chromedp.Call(b.ctx, network.GetCookies, network.GetCookiesParams{URLs: []string{b.server.BaseURL}})
	if err != nil {
		return "", err
	}
	for _, cookie := range result.Cookies {
		if cookie.Name == "session" {
			return cookie.Value, nil
		}
	}
	return "", nil
}

func send[P any](b *Browser, command cdp.Command[P, cdp.Empty], params P) error {
	_, err := chromedp.Call(b.ctx, command, params)
	return err
}

// Goto navigates to a path on the server and waits for the page to load.
func (b *Browser) Goto(path string) error {
	return chromedp.Do(b.ctx, chromedp.Navigate(b.server.URL(path)))
}

// WaitForLoad waits for document.readyState to be "complete".
//
// CDP does not wait for a page load after a click, so every read or action
// that follows a navigation waits first; otherwise a lookup can run against
// the old document.
func (b *Browser) WaitForLoad() error {
	return Eventually("the page to finish loading", func() (bool, error) {
		state, err := Eval[string](b, "document.readyState")
		return state == "complete", err
	})
}

// DocumentID identifies the current document: it differs after any
// navigation, even one back to the same URL.
func (b *Browser) DocumentID() (float64, error) {
	return Eval[float64](b, "performance.timeOrigin")
}

// URL is the page's current URL.
func (b *Browser) URL() (*url.URL, error) {
	location, err := chromedp.Run(b.ctx, chromedp.Location())
	if err != nil {
		return nil, err
	}
	return url.Parse(location)
}

// Path is the current URL's path, the shape the steps assert against.
func (b *Browser) Path() (string, error) {
	current, err := b.URL()
	if err != nil {
		return "", err
	}
	return current.Path, nil
}

// Eval evaluates a JavaScript expression in the page and decodes the result.
//
// Reads go through JavaScript rather than DOM-domain queries, because
// chromedp's queries wait forever by default and the suite's lookups are
// immediate. It also runs under @no-js, and is not subject to the page CSP.
func Eval[T any](b *Browser, expression string) (T, error) {
	return chromedp.Run(b.ctx, chromedp.Evaluate[T](expression))
}

// jsLiteral renders a Go value as a JavaScript literal.
func jsLiteral(value any) string {
	encoded, err := json.Marshal(value)
	if err != nil {
		// Only strings, numbers and bools are passed; this cannot happen.
		panic(fmt.Sprintf("encoding %v as a JavaScript literal: %v", value, err))
	}
	return string(encoded)
}
