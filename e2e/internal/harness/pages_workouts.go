package harness

import (
	"fmt"
	"regexp"
	"slices"
)

// WorkoutsPage is the workouts list (/workouts).
type WorkoutsPage struct{ browser *Browser }

// Workouts returns the workouts list page object.
func (b *Browser) Workouts() WorkoutsPage { return WorkoutsPage{browser: b} }

// Goto navigates to /workouts.
func (p WorkoutsPage) Goto() error { return p.browser.Goto("/workouts") }

// LinksTo is how many links on the page point at that workout.
func (p WorkoutsPage) LinksTo(id string) (int, error) {
	return p.browser.Count(CSS(fmt.Sprintf(`a[href="/workouts/%s"]`, id)))
}

// EmptyState is the empty-state text, with false when the list has workouts.
func (p WorkoutsPage) EmptyState() (string, bool, error) {
	return p.browser.TextOf(CSS(".empty-state"))
}

// CreateWorkoutToday creates a workout dated today (the server pre-fills the date)
// and returns its id.
func (b *Browser) CreateWorkoutToday() (string, error) {
	if err := b.Goto("/workouts/new"); err != nil {
		return "", err
	}
	if err := b.ClickButton("Create Workout"); err != nil {
		return "", err
	}
	// The click returns before the redirect is followed.
	if err := Eventually("the browser reaches the new workout", func() (bool, error) {
		_, err := b.WorkoutIDFromURL()
		return err == nil, nil
	}); err != nil {
		return "", err
	}
	return b.WorkoutIDFromURL()
}

var workoutPath = regexp.MustCompile(`^/workouts/([0-9a-fA-F-]+)$`)

// WorkoutIDFromURL is the workout id in the current URL. It must look like an
// id, or /workouts/new would pass.
func (b *Browser) WorkoutIDFromURL() (string, error) {
	path, err := b.Path()
	if err != nil {
		return "", err
	}
	match := workoutPath.FindStringSubmatch(path)
	if match == nil {
		return "", fmt.Errorf("not on a workout detail page: %s", path)
	}
	return match[1], nil
}

// WorkoutPage is a workout's detail page (/workouts/{id}), and everything
// hanging off it.
type WorkoutPage struct {
	browser *Browser
	id      string
}

// Workout returns the detail page object for the workout.
func (b *Browser) Workout(id string) WorkoutPage { return WorkoutPage{browser: b, id: id} }

// Goto navigates to the workout.
func (p WorkoutPage) Goto() error { return p.browser.Goto("/workouts/" + p.id) }

// SelectExercise picks an exercise in the Add Set form; its change draws the
// "last weight" hint from data embedded in the page.
func (p WorkoutPage) SelectExercise(name string) error {
	return p.browser.SelectByLabel("exercise_id", name)
}

// Suggestions are the values an Add Set field's datalist offers, in document
// order. They are resolved through the field's list attribute, so a detached
// list fails.
func (p WorkoutPage) Suggestions(field string) ([]string, error) {
	input, err := p.browser.Query(ID(field))
	if err != nil {
		return nil, err
	}
	list, found, err := input.Attr("list")
	if err != nil {
		return nil, err
	}
	if !found {
		return nil, fmt.Errorf("the `%s` field points at no datalist", field)
	}
	options, err := p.browser.QueryAll(CSS("#" + list + " option"))
	if err != nil {
		return nil, err
	}
	var values []string
	for _, option := range options {
		value, found, err := option.Attr("value")
		if err != nil {
			return nil, err
		}
		if found {
			values = append(values, value)
		}
	}
	if len(values) == 0 {
		return nil, fmt.Errorf("the `%s` field points at `%s`, which offers nothing", field, list)
	}
	return values, nil
}

// SetEntry is the Add Set form's contents. RPE is empty to leave it blank.
type SetEntry struct {
	Exercise string
	Weight   string
	Reps     string
	RPE      string
}

// LogSet fills and submits the Add Set form, then waits for the row count to
// grow: the POST redirects back to the same URL, and an existing row of the
// same exercise would satisfy a mere presence check.
func (p WorkoutPage) LogSet(entry SetEntry) error {
	before, err := p.rows(entry.Exercise)
	if err != nil {
		return err
	}
	if err := p.SelectExercise(entry.Exercise); err != nil {
		return err
	}
	if err := p.browser.Fill("weight", entry.Weight); err != nil {
		return err
	}
	if err := p.browser.Fill("reps", entry.Reps); err != nil {
		return err
	}
	if entry.RPE != "" {
		if err := p.browser.Fill("rpe", entry.RPE); err != nil {
			return err
		}
	}
	if err := p.browser.ClickButton("Add Set"); err != nil {
		return err
	}
	return EventuallyEq("the set rows after adding one", len(before)+1, p.RowCount(entry.Exercise))
}

// RowCount is a probe for how many set rows are logged against the exercise.
func (p WorkoutPage) RowCount(exercise string) func() (int, error) {
	return func() (int, error) {
		rows, err := p.rows(exercise)
		return len(rows), err
	}
}

func (p WorkoutPage) rows(exercise string) ([]*Element, error) {
	return p.browser.QueryAll(XPath(setRowXPath(exercise)))
}

func (p WorkoutPage) row(exercise string) (*Element, error) {
	row, err := p.browser.Optional(XPath(setRowXPath(exercise)))
	if err == nil && row == nil {
		return nil, fmt.Errorf("no set row for `%s`", exercise)
	}
	return row, err
}

// Cell is one cell of the first row, by the set-cell-* suffix that names it.
func (p WorkoutPage) Cell(exercise, cell string) (string, error) {
	row, err := p.row(exercise)
	if err != nil {
		return "", err
	}
	element, err := row.Find(CSS(".set-cell-" + cell))
	if err != nil {
		return "", err
	}
	return element.Text()
}

// SetNumbers is the set-cell-set value of every row for the exercise, sorted.
func (p WorkoutPage) SetNumbers(exercise string) ([]string, error) {
	rows, err := p.rows(exercise)
	if err != nil {
		return nil, err
	}
	var numbers []string
	for _, row := range rows {
		cell, err := row.Find(CSS(".set-cell-set"))
		if err != nil {
			return nil, err
		}
		text, err := cell.Text()
		if err != nil {
			return nil, err
		}
		numbers = append(numbers, text)
	}
	slices.Sort(numbers)
	return numbers, nil
}

// PRBadge is the PR badge on the first row, with false when the set is not a
// record.
func (p WorkoutPage) PRBadge(exercise string) (string, bool, error) {
	row, err := p.row(exercise)
	if err != nil {
		return "", false, err
	}
	badge, err := row.FindOptional(CSS(".pr-badge"))
	if err != nil || badge == nil {
		return "", false, err
	}
	text, err := badge.Text()
	return text, err == nil, err
}

// OpenEditSet opens the edit form for the first set of the exercise.
func (p WorkoutPage) OpenEditSet(exercise string) error {
	row, err := p.row(exercise)
	if err != nil {
		return err
	}
	if err := row.ClickLinkIn("Edit"); err != nil {
		return err
	}
	_, err = p.browser.Displayed(XPath("//h1[normalize-space(.)='Edit Set']"))
	return err
}

// DeleteSet deletes the first set of the exercise via its x link, addressed
// by aria-label. It waits for the row to go: the POST redirects back to the
// same URL.
func (p WorkoutPage) DeleteSet(exercise string) error {
	row, err := p.row(exercise)
	if err != nil {
		return err
	}
	if err := row.ClickNavigatingIn(CSS(`a[aria-label="Delete set"]`)); err != nil {
		return err
	}
	return Eventually("the set row leaves the workout", func() (bool, error) {
		rows, err := p.rows(exercise)
		return len(rows) == 0, err
	})
}

// CloneSet clicks Clone on the first set of the exercise. With scripts on it
// fills the form in place instead of navigating, so it only clicks.
func (p WorkoutPage) CloneSet(exercise string) error {
	row, err := p.row(exercise)
	if err != nil {
		return err
	}
	formState := func() string {
		exercise, _ := p.browser.ValueOf("exercise_id")
		weight, _ := p.browser.ValueOf("weight")
		return currentURL(p.browser) + "|" + exercise + "|" + weight
	}
	before := formState()
	return clickUntil(p.browser, clickSpec{
		what: "Clone",
		locate: func() (*Element, error) {
			return displayedIn(p.browser, row, XPath(".//a[normalize-space(.)='Clone']"))
		},
		tookEffect: func() (bool, error) {
			return formState() != before, nil
		},
	})
}

// Delete deletes the whole workout.
func (p WorkoutPage) Delete() error { return p.browser.ClickLink("Delete") }

// LastInfoRPE is the "last weight" hint's RPE chip, which only a script draws.
func (p WorkoutPage) LastInfoRPE() (string, bool, error) {
	return p.browser.TextOf(CSS("#exercise-last-weight-info .rpe-chip"))
}

// Share publishes a share link for the workout.
func (p WorkoutPage) Share() error {
	return p.browser.ClickNavigating(XPath("//form[contains(@action,'/share')]//button[normalize-space(.)='Share']"))
}

// ShareURL is the public share path, with false when the workout is not
// shared.
func (p WorkoutPage) ShareURL() (string, bool, error) {
	link, err := p.browser.Optional(CSS(`.share-info a[href^="/shared/"]`))
	if err != nil || link == nil {
		return "", false, err
	}
	href, found, err := link.Attr("href")
	return href, found, err
}

// RevokeShare withdraws the share link.
func (p WorkoutPage) RevokeShare() error { return p.browser.ClickLink("Revoke Share") }

// HasShareInfo reports whether a share block is on the page at all.
func (p WorkoutPage) HasShareInfo() (bool, error) { return p.browser.has(CSS(".share-info")) }

// Heading is the h1, which the detail page renders as the workout's date.
func (p WorkoutPage) Heading() (string, error) {
	heading, err := p.browser.Displayed(CSS("h1"))
	if err != nil {
		return "", err
	}
	return heading.Text()
}

// Notes is the notes line under the heading, with false when there is none.
func (p WorkoutPage) Notes() (string, bool, error) {
	return p.browser.TextOf(CSS(".subtitle em"))
}

// PrefilledExercise is the exercise the Add Set form has selected, as its id.
func (p WorkoutPage) PrefilledExercise() (string, error) { return p.browser.ValueOf("exercise_id") }

// PrefilledWeight is the weight the Add Set form holds.
func (p WorkoutPage) PrefilledWeight() (string, error) { return p.browser.ValueOf("weight") }

// PrefilledReps is the reps the Add Set form holds.
func (p WorkoutPage) PrefilledReps() (string, error) { return p.browser.ValueOf("reps") }

// SaveMetadata rewrites the date (set directly) and notes on the workout
// metadata form (/workouts/{id}/edit), and submits.
func (p WorkoutPage) SaveMetadata(date, notes string) error {
	if err := p.browser.Goto("/workouts/" + p.id + "/edit"); err != nil {
		return err
	}
	if err := p.browser.SetValue("date", date); err != nil {
		return err
	}
	if err := p.browser.Fill("notes", notes); err != nil {
		return err
	}
	return p.browser.ClickButton("Save Changes")
}

// SaveEditedSet rewrites the weight and reps on the set form, reached from a
// row's Edit, and submits.
func (b *Browser) SaveEditedSet(weight, reps string) error {
	if err := b.Fill("weight", weight); err != nil {
		return err
	}
	if err := b.Fill("reps", reps); err != nil {
		return err
	}
	return b.ClickButton("Save Changes")
}

// setRowXPath is the set rows naming exactly that exercise.
func setRowXPath(exercise string) string {
	return "//div[contains(@class,'set-row')]" +
		"[.//div[contains(@class,'set-cell-exercise')][normalize-space(.)=" + Quote(exercise) + "]]"
}
