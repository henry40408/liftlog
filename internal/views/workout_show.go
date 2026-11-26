package views

import (
	_ "embed"
	"strconv"
	"time"

	"github.com/henry40408/liftlog/internal/chart"
	"github.com/henry40408/liftlog/internal/models"
)

// PrefillSet holds the Add Set form values from `?prefill=<log_id>` (no-JS
// Clone).
type PrefillSet struct {
	ExerciseID string
	Weight     float64
	Reps       int
	RPE        *int
}

// LastWeightRow is one row of the `<noscript>` last-weights list.
type LastWeightRow struct {
	ExerciseName string
	Weight       float64
	RPE          *int
	LoggedAt     time.Time
}

type ShareInfo struct {
	URL string
	// ExpiresAt nil means the link never expires.
	ExpiresAt *time.Time
}

// ShowWorkoutView is the data of the workout page.
type ShowWorkoutView struct {
	User                NavUser
	Workout             models.WorkoutSession
	Logs                []models.WorkoutLogWithExercise
	Exercises           []models.Exercise
	Categories          []models.ExerciseCategory
	ExerciseLastWeights []models.LastExerciseWeight
	LastWeightRows      []LastWeightRow
	WeightSuggestions   []float64
	RepSuggestions      []int
	Prefill             *PrefillSet
	Share               *ShareInfo
	Error               string
}

// showJS is the static half of the page script; the per-exercise data is
// emitted before it.
//
//go:embed scripts/workout_show.js
var showJS string

func ShowWorkout(view ShowWorkoutView) string {
	workout := view.Workout
	date := formatDate(workout.Date)
	body := group(
		nav(view.User),
		el("main", nil,
			el("div", []attr{a("class", "page-header")},
				el("h1", nil, text(date)),
				workoutSubtitle(workout.Notes),
			),
			workoutActions(workout, view.Share == nil),
			shareInfoBlock(workout, view.Share),
			el("h3", []attr{a("id", "add-set")}, text("Add Set")),
			el("div", []attr{a("class", "card card-accent"), a("style", "margin: var(--sp-4) 0;")},
				addSetForm(view),
				// No-JS stand-in for the last-weight hint, which needs the <select> change event.
				el("noscript", nil, lastWeightsTable(view.LastWeightRows)),
			),
			el("h2", nil, text("Sets")),
			errorBox(view.Error),
			setsList(workout, view.Logs),
			backLink("/workouts", "Back to Workouts"),
		),
		el("script", nil,
			node("\nvar exerciseLastWeights = {};\n"),
			lastWeightEntries(view.ExerciseLastWeights),
			node("\n"),
			node(showJS),
		),
	)
	return page("Workout "+date+" - LiftLog", body)
}

func workoutSubtitle(notes *string) node {
	if notes == nil {
		return ""
	}
	return el("div", []attr{a("class", "subtitle")}, el("em", nil, text(*notes)))
}

func workoutActions(workout models.WorkoutSession, showShareForm bool) node {
	children := []node{
		el("a", []attr{a("href", "/workouts/"+workout.ID+"/edit"), a("class", "btn btn-ghost btn-sm")}, text("Edit")),
		text(" "),
		el("a", []attr{
			a("href", "/workouts/"+workout.ID+"/delete"), a("class", "btn btn-danger btn-sm"),
			a("data-confirm", "Delete this workout? Every set recorded in it is deleted too, and this cannot be undone."),
		}, text("Delete")),
	}
	if showShareForm {
		children = append(children, text(" "), shareForm(workout))
	}
	return el("div", []attr{a("class", "actions mb-6")}, children...)
}

func shareForm(workout models.WorkoutSession) node {
	return el("form", []attr{a("action", "/workouts/"+workout.ID+"/share"), a("method", "post"), a("class", "share-form")},
		el("select", []attr{a("id", "expires_in_days"), a("name", "expires_in_days"), a("aria-label", "Share link expiry")},
			el("option", []attr{a("value", ""), flag("selected")}, text("Never expires")),
			el("option", []attr{a("value", "7")}, text("7 days")),
			el("option", []attr{a("value", "30")}, text("30 days")),
			el("option", []attr{a("value", "90")}, text("90 days")),
		),
		text(" "),
		el("button", []attr{a("type", "submit"), a("class", "btn btn-ghost btn-sm")}, text("Share")),
	)
}

func shareInfoBlock(workout models.WorkoutSession, share *ShareInfo) node {
	if share == nil {
		return ""
	}
	return el("div", []attr{a("class", "share-info")},
		el("p", []attr{a("style", "margin: 0 0 var(--sp-3) 0;")},
			text("Share link: "),
			el("a", []attr{a("href", share.URL), a("target", "_blank")}, text(share.URL)),
			text(" "),
			// Needs the clipboard API: hidden until base.html reveals it.
			el("button", []attr{
				a("type", "button"), a("class", "btn btn-sm btn-inline"), flag("hidden"), flag("data-requires-js"),
				a("data-copy-url", share.URL),
				a("style", "background:var(--accent);color:var(--text-inverse);border-color:var(--accent);"),
			}, text("Copy")),
		),
		shareExpiry(share.ExpiresAt),
		el("a", []attr{
			a("href", "/workouts/"+workout.ID+"/revoke-share"), a("class", "btn btn-danger btn-sm"),
			a("data-confirm", "Revoke the share link? Anyone who already has it loses access, and sharing again produces a different link."),
		}, text("Revoke Share")),
	)
}

func shareExpiry(expiresAt *time.Time) node {
	style := a("style", "margin: 0 0 var(--sp-3) 0;")
	if expiresAt == nil {
		return el("p", []attr{a("class", "muted"), style}, text("Never expires"))
	}
	return el("p", []attr{a("class", "muted"), style}, text("Expires "), utcTime(*expiresAt))
}

func addSetForm(view ShowWorkoutView) node {
	return el("form", []attr{a("method", "post"), a("action", "/workouts/"+view.Workout.ID+"/logs")},
		el("div", []attr{a("class", "form-group")},
			el("label", []attr{a("for", "exercise_id")}, text("Exercise")),
			text(" "),
			exerciseSelect(view),
			el("div", []attr{a("id", "exercise-last-weight-info"), a("class", "pr-info")}),
		),
		el("div", []attr{a("class", "form-group")},
			el("label", []attr{a("for", "weight")}, text("Weight")),
			text(" "),
			// Datalist, not <select>: suggestions, any weight still typeable.
			weightInput(view.Prefill),
			weightDatalist(view.WeightSuggestions),
		),
		el("div", []attr{a("class", "form-group")},
			el("label", []attr{a("for", "reps")}, text("Reps")),
			text(" "),
			repsInput(view.Prefill),
			repsDatalist(view.RepSuggestions),
		),
		el("div", []attr{a("class", "form-group")},
			el("label", []attr{a("for", "rpe")}, text("RPE (1-10, optional)")),
			text(" "),
			rpeInput(view.Prefill),
		),
		el("button", []attr{a("type", "submit")}, text("Add Set")),
	)
}

func exerciseSelect(view ShowWorkoutView) node {
	children := []node{el("option", []attr{a("value", "")}, text("-- Select --"))}
	for _, category := range view.Categories {
		children = append(children, categoryGroup(category, view.Exercises, view.Prefill))
	}
	return el("select", []attr{a("id", "exercise_id"), a("name", "exercise_id"), flag("required")}, children...)
}

func categoryGroup(category models.ExerciseCategory, exercises []models.Exercise, prefill *PrefillSet) node {
	var options []node
	for _, exercise := range exercises {
		if exercise.Category != category.Name {
			continue
		}
		attrs := []attr{a("value", exercise.ID)}
		if prefill != nil && prefill.ExerciseID == exercise.ID {
			attrs = append(attrs, flag("selected"))
		}
		options = append(options, el("option", attrs, text(exercise.Name)))
	}
	return el("optgroup", []attr{a("label", category.DisplayName)}, options...)
}

// weightStep is the finest increment the weight fields accept; some equipment
// is labelled with values such as 16.83.
const weightStep = "0.01"

func weightInput(prefill *PrefillSet) node {
	attrs := []attr{
		a("type", "number"), a("inputmode", "decimal"), a("id", "weight"), a("name", "weight"),
		a("step", weightStep), a("min", "0"), a("list", "weight-list"), a("autocomplete", "off"), flag("required"),
	}
	if prefill != nil {
		attrs = append(attrs, a("value", chart.Display(prefill.Weight)))
	}
	return void("input", attrs...)
}

func weightDatalist(suggestions []float64) node {
	if len(suggestions) == 0 {
		return ""
	}
	options := make([]node, len(suggestions))
	for i, w := range suggestions {
		options[i] = el("option", []attr{a("value", chart.Display(w))})
	}
	return el("datalist", []attr{a("id", "weight-list")}, options...)
}

func repsInput(prefill *PrefillSet) node {
	attrs := []attr{
		a("type", "number"), a("inputmode", "numeric"), a("id", "reps"), a("name", "reps"),
		a("min", "1"), a("list", "reps-list"), a("autocomplete", "off"), flag("required"),
	}
	if prefill != nil {
		attrs = append(attrs, a("value", strconv.Itoa(prefill.Reps)))
	}
	return void("input", attrs...)
}

func repsDatalist(suggestions []int) node {
	options := make([]node, len(suggestions))
	for i, r := range suggestions {
		options[i] = el("option", []attr{a("value", strconv.Itoa(r))})
	}
	return el("datalist", []attr{a("id", "reps-list")}, options...)
}

func rpeInput(prefill *PrefillSet) node {
	attrs := []attr{
		a("type", "number"), a("inputmode", "numeric"), a("id", "rpe"), a("name", "rpe"), a("min", "1"), a("max", "10"),
	}
	if prefill != nil && prefill.RPE != nil {
		attrs = append(attrs, a("value", strconv.Itoa(*prefill.RPE)))
	}
	return void("input", attrs...)
}

func lastWeightsTable(rows []LastWeightRow) node {
	if len(rows) == 0 {
		return ""
	}
	body := make([]node, len(rows))
	for i, row := range rows {
		body[i] = el("tr", nil,
			el("td", []attr{a("data-label", "Exercise")}, text(row.ExerciseName)),
			el("td", []attr{a("data-label", "Weight")}, text(chart.Display(row.Weight)+" kg")),
			el("td", []attr{a("data-label", "RPE")}, rpeCell(row.RPE)),
			el("td", []attr{a("data-label", "Logged")}, utcDate(row.LoggedAt)),
		)
	}
	return group(
		el("h4", []attr{a("style", "margin: var(--sp-5) 0 var(--sp-2) 0;")}, text("Last time")),
		el("table", []attr{a("class", "data-table")},
			el("thead", nil, el("tr", nil,
				el("th", nil, text("Exercise")), el("th", nil, text("Weight")),
				el("th", nil, text("RPE")), el("th", nil, text("Logged")))),
			el("tbody", nil, body...),
		),
	)
}

// utcDate is the date-only <time> element; the text is the no-JS fallback.
func utcDate(t time.Time) node {
	t = t.UTC()
	return el("time", []attr{a("datetime", rfc3339(t)), a("data-fmt", "date")}, text(formatDate(t)))
}

func setsList(workout models.WorkoutSession, logs []models.WorkoutLogWithExercise) node {
	if len(logs) == 0 {
		return el("p", []attr{a("class", "muted")}, text("No sets recorded yet."))
	}
	rows := []node{setsHeader()}
	for _, log := range logs {
		rows = append(rows, setRow(workout, log))
	}
	return el("div", []attr{a("class", "sets-list")}, rows...)
}

func setRow(workout models.WorkoutSession, log models.WorkoutLogWithExercise) node {
	return el("div", []attr{a("class", "set-row")},
		setCell("exercise", el("a", []attr{a("href", "/stats/exercise/"+log.ExerciseID)}, text(log.ExerciseName))),
		setCell("set", text(strconv.Itoa(log.SetNumber))),
		setCell("weight", text(chart.Display(log.Weight))),
		setCell("reps", text(strconv.Itoa(log.Reps))),
		setCell("rpe", rpeCell(log.RPE)),
		setCell("pr", prBadge(log)),
		setRowActions(workout, log),
	)
}

func prBadge(log models.WorkoutLogWithExercise) node {
	switch {
	case log.IsPR:
		return el("span", []attr{a("class", "pr-badge"), a("title", "All-time best for this exercise")}, text("PR"))
	case log.IsRecentPR:
		return el("span", []attr{a("class", "pr-badge pr-badge-recent"), a("title", "Best for this exercise in the last month")}, text("PR 1M"))
	}
	return ""
}

func setRowActions(workout models.WorkoutSession, log models.WorkoutLogWithExercise) node {
	logPath := "/workouts/" + workout.ID + "/logs/" + log.ID
	rpe := ""
	if log.RPE != nil {
		rpe = strconv.Itoa(*log.RPE)
	}
	return el("div", []attr{a("class", "set-row-actions")},
		el("a", []attr{a("href", logPath+"/edit"), a("class", "btn btn-ghost btn-sm")}, text("Edit")),
		text(" "),
		// href works without scripts; the handler in the page script fills in place.
		el("a", []attr{
			a("href", "?prefill="+log.ID+"#add-set"), a("class", "btn btn-ghost btn-sm"),
			a("data-clone-exercise", log.ExerciseID),
			a("data-clone-weight", chart.Display(log.Weight)),
			a("data-clone-reps", strconv.Itoa(log.Reps)),
			a("data-clone-rpe", rpe),
		}, text("Clone")),
		text(" "),
		el("a", []attr{
			a("href", logPath+"/delete"), a("class", "btn btn-danger btn-sm"), a("aria-label", "Delete set"),
			a("data-confirm", "Delete set "+strconv.Itoa(log.SetNumber)+" of "+log.ExerciseName+"?"),
		}, node("&times;")),
	)
}

func lastWeightEntries(entries []models.LastExerciseWeight) node {
	nodes := make([]node, len(entries))
	for i, entry := range entries {
		nodes[i] = lastWeightEntry(entry)
	}
	return group(nodes...)
}

// lastWeightEntry is one `exerciseLastWeights[...]` assignment of the page
// script.
func lastWeightEntry(entry models.LastExerciseWeight) node {
	rpe := "null"
	if entry.RPE != nil {
		rpe = strconv.Itoa(*entry.RPE)
	}
	return group(
		node("\nexerciseLastWeights[\""),
		text(entry.ExerciseID),
		node("\"] = {\n    weight: "),
		text(chart.Display(entry.Weight)),
		node(",\n    rpe: "),
		text(rpe),
		node(",\n    loggedAt: \""),
		text(rfc3339(entry.LoggedAt)),
		node("\"\n};\n"),
	)
}
