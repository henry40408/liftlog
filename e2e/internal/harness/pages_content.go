package harness

import (
	"fmt"
	"strings"
)

// ConfirmPage is the server-rendered confirmation page a destructive link
// opens when no script intercepts it.
type ConfirmPage struct{ browser *Browser }

// Confirm returns the confirmation page object.
func (b *Browser) Confirm() ConfirmPage { return ConfirmPage{browser: b} }

const confirmButton = "main form[method=post] .btn-danger"

// Path waits for the page and returns the path it is served from.
func (p ConfirmPage) Path() (string, error) {
	if _, err := p.browser.Displayed(CSS(confirmButton)); err != nil {
		return "", err
	}
	return p.browser.Path()
}

// Accept presses the button that carries out the action, then waits for the
// redirect away from the confirmation page so the next navigation does not
// cancel the POST.
func (p ConfirmPage) Accept() error {
	here, err := p.Path()
	if err != nil {
		return err
	}
	if err := p.browser.ClickNavigating(CSS(confirmButton)); err != nil {
		return err
	}
	return Eventually("the POST redirects away from the confirmation page", func() (bool, error) {
		path, err := p.browser.Path()
		return path != here, err
	})
}

// Cancel backs out through the Cancel link.
func (p ConfirmPage) Cancel() error { return p.browser.ClickLink("Cancel") }

// DashboardPage is the dashboard (/).
type DashboardPage struct{ browser *Browser }

// Dashboard returns the dashboard page object.
func (b *Browser) Dashboard() DashboardPage { return DashboardPage{browser: b} }

// Goto navigates to /.
func (p DashboardPage) Goto() error { return p.browser.Goto("/") }

// IsShowing reports whether the dashboard heading is on screen: the signed-in
// marker.
func (p DashboardPage) IsShowing() (bool, error) {
	return p.browser.has(XPath("//h1[normalize-space(.)='Dashboard']"))
}

// ListsWorkout reports whether the workout is linked from Recent Workouts.
func (p DashboardPage) ListsWorkout(id string) (bool, error) {
	return p.browser.has(CSS(fmt.Sprintf(`.workout-list a[href="/workouts/%s"]`, id)))
}

// HasRecentHeading reports whether the Recent Workouts heading is there.
func (p DashboardPage) HasRecentHeading() (bool, error) {
	return p.browser.has(XPath("//h2[normalize-space(.)='Recent Workouts']"))
}

// Stat is the number on the summary card with the given label.
func (p DashboardPage) Stat(label string) (string, error) {
	text, found, err := p.browser.TextOf(XPath(
		"//div[contains(@class,'stat-card')][.//div[contains(@class,'stat-label')]" +
			"[normalize-space(.)=" + Quote(label) + "]]//div[contains(@class,'stat-value')]"))
	if err == nil && !found {
		return "", fmt.Errorf("no summary card labelled `%s`", label)
	}
	return text, err
}

// ActionsLead reports whether the quick actions render above both the
// summary and the workout list.
func (p DashboardPage) ActionsLead() (bool, error) {
	actions, err := p.browser.TopOf(".actions-lead")
	if err != nil {
		return false, err
	}
	stats, err := p.browser.TopOf(".stats-grid")
	if err != nil {
		return false, err
	}
	recent, err := p.browser.TopOf("h2")
	if err != nil {
		return false, err
	}
	return actions < stats && actions < recent, nil
}

// ExercisesPage is the exercises list and the forms reached from it.
type ExercisesPage struct{ browser *Browser }

// Exercises returns the exercises page object.
func (b *Browser) Exercises() ExercisesPage { return ExercisesPage{browser: b} }

// Goto navigates to /exercises.
func (p ExercisesPage) Goto() error { return p.browser.Goto("/exercises") }

// Create adds an exercise and returns its id, read off the list.
func (p ExercisesPage) Create(name, category string) (string, error) {
	if err := p.browser.Goto("/exercises/new"); err != nil {
		return "", err
	}
	if err := p.browser.Fill("name", name); err != nil {
		return "", err
	}
	if err := p.browser.SelectByValue("category", category); err != nil {
		return "", err
	}
	if err := p.browser.ClickButton("Add Exercise"); err != nil {
		return "", err
	}
	if err := EventuallyEq("the URL after the form post", "/exercises", p.browser.Path); err != nil {
		return "", err
	}
	return p.IDOf(name)
}

// Rename renames the exercise; the caller waits for the redirect.
func (p ExercisesPage) Rename(from, to string) error {
	if err := p.Goto(); err != nil {
		return err
	}
	row, err := p.row(from)
	if err != nil {
		return err
	}
	if err := row.ClickLinkIn("Edit"); err != nil {
		return err
	}
	if err := p.browser.Fill("name", to); err != nil {
		return err
	}
	return p.browser.ClickButton("Save Changes")
}

// Delete deletes the exercise (the window.confirm() is auto-accepted). It
// waits for the entry to leave the list: the POST redirects back to
// /exercises, so the URL never changes.
func (p ExercisesPage) Delete(name string) error {
	if err := p.ClickDelete(name); err != nil {
		return err
	}
	return Eventually(fmt.Sprintf("`%s` leaves the list", name), func() (bool, error) {
		count, err := p.EntriesNamed(name)
		return count == 0, err
	})
}

// ClickDelete clicks Delete without waiting for the outcome: with scripting
// off it opens the confirmation page.
func (p ExercisesPage) ClickDelete(name string) error {
	if err := p.Goto(); err != nil {
		return err
	}
	row, err := p.row(name)
	if err != nil {
		return err
	}
	return row.ClickLinkIn("Delete")
}

// EntriesNamed is how many entries on the list carry that name.
func (p ExercisesPage) EntriesNamed(name string) (int, error) {
	return p.browser.Count(XPath(exerciseLinkXPath(name)))
}

// ListedNames is every name on the list, for failure messages.
func (p ExercisesPage) ListedNames() ([]string, error) {
	return p.browser.TextsOf(CSS(".exercise-item > a"))
}

// IDOf is the exercise's id, read off its link to /stats/exercise/{id}.
func (p ExercisesPage) IDOf(name string) (string, error) {
	if err := p.Goto(); err != nil {
		return "", err
	}
	entry, err := p.browser.Optional(XPath(exerciseLinkXPath(name)))
	if err != nil {
		return "", err
	}
	if entry == nil {
		names, _ := p.ListedNames()
		return "", fmt.Errorf("`%s` is not on /exercises, which lists %v", name, names)
	}
	href, found, err := entry.Attr("href")
	if err != nil {
		return "", err
	}
	id, ok := strings.CutPrefix(href, "/stats/exercise/")
	if !found || !ok {
		return "", fmt.Errorf("`%s` links to %s, not to its stats page", name, href)
	}
	return id, nil
}

func (p ExercisesPage) row(name string) (*Element, error) {
	row, err := p.browser.Optional(XPath(
		"//div[contains(@class,'exercise-item')][.//a[normalize-space(.)=" + Quote(name) + "]]"))
	if err == nil && row == nil {
		return nil, fmt.Errorf("no exercise named `%s` on the list", name)
	}
	return row, err
}

// exerciseLinkXPath is the exercise's own link, the one carrying its name.
func exerciseLinkXPath(name string) string {
	return "//div[contains(@class,'exercise-item')]/a[normalize-space(.)=" + Quote(name) + "]"
}

// StatsPage is /stats, /stats/exercise/{id} and /stats/prs.
type StatsPage struct{ browser *Browser }

// Stats returns the stats page object.
func (b *Browser) Stats() StatsPage { return StatsPage{browser: b} }

// GotoOverview navigates to /stats.
func (p StatsPage) GotoOverview() error { return p.browser.Goto("/stats") }

// GotoExercise navigates to the exercise's stats page.
func (p StatsPage) GotoExercise(id string) error { return p.browser.Goto("/stats/exercise/" + id) }

// GotoPRs navigates to /stats/prs.
func (p StatsPage) GotoPRs() error { return p.browser.Goto("/stats/prs") }

// OverviewIsShowing reports whether the overview's heading and summary grid
// are on screen.
func (p StatsPage) OverviewIsShowing() (bool, error) {
	heading, err := p.browser.has(XPath("//h1[normalize-space(.)='Statistics']"))
	if err != nil {
		return false, err
	}
	grid, err := p.browser.has(CSS(".stats-grid"))
	return heading && grid, err
}

// ExerciseHeading is the exercise page's h1 as DOM text: it is
// text-transform: uppercase.
func (p StatsPage) ExerciseHeading() (string, error) {
	text, found, err := p.browser.DomText("h1")
	if err == nil && !found {
		return "", fmt.Errorf("the exercise stats page has no heading")
	}
	return text, err
}

// HasChart reports whether the progress chart is drawn, which needs a logged
// set.
func (p StatsPage) HasChart() (bool, error) {
	return p.browser.has(ID("exercise-chart"))
}

// PRRowsFor is how many rows of the PR list name that exercise.
func (p StatsPage) PRRowsFor(exercise string) (int, error) {
	return p.browser.Count(XPath(prRowXPath(exercise)))
}

// PRCell is one column of that exercise's PR row, addressed by data-label.
func (p StatsPage) PRCell(exercise, label string) (string, error) {
	text, found, err := p.browser.TextOf(XPath(prRowXPath(exercise) + "/td[@data-label=" + Quote(label) + "]"))
	if err == nil && !found {
		return "", fmt.Errorf("no `%s` cell for `%s` on the PR list", label, exercise)
	}
	return text, err
}

func prRowXPath(exercise string) string {
	return "//tbody/tr[td[@data-label='Exercise']//a[normalize-space(.)=" + Quote(exercise) + "]]"
}
