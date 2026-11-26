package views

import (
	"strconv"
	"time"

	"github.com/henry40408/liftlog/internal/chart"
	"github.com/henry40408/liftlog/internal/models"
)

func notesOrDash(notes *string) node {
	if notes == nil {
		return node("&mdash;")
	}
	return text(*notes)
}

// rpeCell is the RPE of a set, or "-" when none was recorded.
func rpeCell(rpe *int) node {
	if rpe == nil {
		return text("-")
	}
	return text(strconv.Itoa(*rpe))
}

// WorkoutsListView is the data of the workouts list page.
type WorkoutsListView struct {
	User       NavUser
	Workouts   []models.WorkoutSession
	Page       int
	TotalPages int
}

func WorkoutsList(view WorkoutsListView) string {
	body := group(
		nav(view.User),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text("Workouts"))),
			el("p", []attr{a("class", "mb-6")},
				el("a", []attr{a("href", "/workouts/new"), a("class", "btn btn-primary")}, text("+ New Workout"))),
			workoutList(view),
		),
	)
	return page("Workouts - LiftLog", body)
}

func workoutList(view WorkoutsListView) node {
	if len(view.Workouts) == 0 {
		return el("div", []attr{a("class", "empty-state")},
			el("p", []attr{a("class", "muted")}, text("No workouts yet. Time to hit the iron.")))
	}
	items := make([]node, len(view.Workouts))
	for i, w := range view.Workouts {
		items[i] = el("div", []attr{a("class", "workout-item")},
			el("a", []attr{a("href", "/workouts/"+w.ID)}, text(formatDate(w.Date))),
			text(" "),
			el("span", []attr{a("class", "notes")}, notesOrDash(w.Notes)),
		)
	}
	list := el("div", []attr{a("class", "workout-list")}, items...)
	if view.TotalPages <= 1 {
		return list
	}
	return group(list, pagination(view.Page, view.TotalPages))
}

func pagination(current, total int) node {
	children := []node{el("span", nil, text("Page "+strconv.Itoa(current)+" / "+strconv.Itoa(total)))}
	if current > 1 {
		children = append(children, text(" "),
			el("a", []attr{a("href", "/workouts?page="+strconv.Itoa(current-1))}, node("&larr;"), text(" Previous")))
	}
	if current < total {
		children = append(children, text(" "),
			el("a", []attr{a("href", "/workouts?page="+strconv.Itoa(current+1))}, text("Next "), node("&rarr;")))
	}
	return el("div", []attr{a("class", "pagination")}, children...)
}

// NewWorkoutView is the data of the new-workout form.
type NewWorkoutView struct {
	User  NavUser
	Today time.Time
	// UseBrowserToday is set when the user has no saved timezone: the server
	// only knows UTC, so the page falls back to the browser's date.
	UseBrowserToday bool
	Error           string
}

// browserTodayJS swaps the server's UTC "today" for the browser's, unless the
// user changed it (e.g. a bfcache restore).
const browserTodayJS = `
/* No saved timezone: swap the server's UTC "today" for the browser's, unless
   the user changed it (e.g. a bfcache restore). */
(function () {
    var dateEl = document.getElementById('date');
    if (dateEl.value !== dateEl.dataset.serverToday) {
        return;
    }
    var browserToday = window.LiftLog.formatLocalDate(new Date().toISOString());
    if (browserToday) {
        dateEl.value = browserToday;
    }
})();
`

func NewWorkout(view NewWorkoutView) string {
	today := formatDate(view.Today)
	body := group(
		nav(view.User),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text("New Workout"))),
			errorBox(view.Error),
			el("form", []attr{a("method", "post"), a("action", "/workouts")},
				dateField(a("value", today), a("data-server-today", today)),
				notesField(nil),
				el("button", []attr{a("type", "submit")}, text("Create Workout")),
			),
			backLink("/workouts", "Back to Workouts"),
		),
		browserTodayScript(view.UseBrowserToday),
	)
	return page("New Workout - LiftLog", body)
}

func browserTodayScript(enabled bool) node {
	if !enabled {
		return ""
	}
	return el("script", nil, node(browserTodayJS))
}

func dateField(extra ...attr) node {
	attrs := append([]attr{a("type", "date"), a("id", "date"), a("name", "date")}, extra...)
	return labelledInput("Date", "date", void("input", append(attrs, flag("required"))...))
}

func notesField(notes *string) node {
	var content node
	if notes != nil {
		content = text(*notes)
	}
	return labelledInput("Notes (optional)", "notes",
		el("textarea", []attr{a("id", "notes"), a("name", "notes"), a("rows", "3")}, content))
}

func EditWorkout(user NavUser, workout models.WorkoutSession, errorMessage string) string {
	body := group(
		nav(user),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text("Edit Workout"))),
			errorBox(errorMessage),
			el("form", []attr{a("method", "post"), a("action", "/workouts/"+workout.ID)},
				dateField(a("value", formatDate(workout.Date))),
				notesField(workout.Notes),
				el("button", []attr{a("type", "submit")}, text("Save Changes")),
			),
			backLink("/workouts/"+workout.ID, "Back to Workout"),
		),
	)
	return page("Edit Workout - LiftLog", body)
}

// EditLogView is the data of the edit-set form.
type EditLogView struct {
	User         NavUser
	Workout      models.WorkoutSession
	Log          models.WorkoutLog
	ExerciseName string
	Error        string
}

func EditLog(view EditLogView) string {
	workout, log := view.Workout, view.Log
	rpe := []attr{a("type", "number"), a("inputmode", "numeric"), a("id", "rpe"), a("name", "rpe"), a("min", "1"), a("max", "10")}
	if log.RPE != nil {
		rpe = append(rpe, a("value", strconv.Itoa(*log.RPE)))
	}
	weight := void("input",
		a("type", "number"), a("inputmode", "decimal"), a("id", "weight"), a("name", "weight"),
		a("step", weightStep), a("min", "0"), a("value", chart.Display(log.Weight)), flag("required"))
	reps := void("input",
		a("type", "number"), a("inputmode", "numeric"), a("id", "reps"), a("name", "reps"),
		a("min", "1"), a("value", strconv.Itoa(log.Reps)), flag("required"))
	body := group(
		nav(view.User),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text("Edit Set"))),
			errorBox(view.Error),
			el("form", []attr{a("method", "post"), a("action", "/workouts/"+workout.ID+"/logs/"+log.ID)},
				disabledField("Exercise", "text", view.ExerciseName),
				disabledField("Set Number", "number", strconv.Itoa(log.SetNumber), a("inputmode", "numeric")),
				labelledInput("Weight", "weight", weight),
				labelledInput("Reps", "reps", reps),
				labelledInput("RPE (1-10, optional)", "rpe", void("input", rpe...)),
				el("button", []attr{a("type", "submit")}, text("Save Changes")),
			),
			backLink("/workouts/"+workout.ID, "Back to Workout"),
		),
	)
	return page("Edit Set - LiftLog", body)
}

func disabledField(label, inputType, value string, extra ...attr) node {
	attrs := append([]attr{a("type", inputType)}, extra...)
	attrs = append(attrs, a("value", value), flag("disabled"), a("style", "opacity: 0.6;"))
	return el("div", []attr{a("class", "form-group")},
		el("label", nil, text(label)),
		text(" "),
		void("input", attrs...),
	)
}

// SharedWorkout is the public, read-only view of a shared workout.
func SharedWorkout(workout models.WorkoutSession, logs []models.WorkoutLogWithExercise, ownerUsername string) string {
	date := formatDate(workout.Date)
	body := el("main", []attr{a("style", "max-width: 960px; margin: 0 auto; padding: var(--sp-8) var(--sp-6);")},
		el("div", []attr{a("class", "page-header")},
			el("h1", nil, text(date)),
			el("div", []attr{a("class", "subtitle")}, text("Shared by "),
				el("strong", []attr{a("style", "color: var(--text-primary);")}, text(ownerUsername))),
		),
		sharedNotes(workout.Notes),
		el("h2", nil, text("Sets")),
		sharedSets(logs),
		el("div", []attr{a("class", "mt-8")},
			el("p", []attr{a("class", "muted text-sm")},
				text("This is a shared workout from "), el("a", []attr{a("href", "/")}, text("LiftLog")), text("."))),
	)
	return page("Shared Workout "+date+" - LiftLog", body)
}

func sharedNotes(notes *string) node {
	if notes == nil {
		return ""
	}
	return el("p", []attr{a("class", "muted"), a("style", "margin-bottom: var(--sp-6);")}, el("em", nil, text(*notes)))
}

func sharedSets(logs []models.WorkoutLogWithExercise) node {
	if len(logs) == 0 {
		return el("p", []attr{a("class", "muted")}, text("No sets recorded."))
	}
	rows := []node{setsHeader()}
	for _, log := range logs {
		rows = append(rows, sharedRow(log))
	}
	return el("div", []attr{a("class", "sets-list")}, rows...)
}

func setsHeader() node {
	cells := []string{"Exercise", "Set", "Weight", "Reps", "RPE"}
	children := make([]node, 0, len(cells)+1)
	for _, c := range cells {
		children = append(children, el("div", nil, text(c)))
	}
	children = append(children, el("div", nil))
	return el("div", []attr{a("class", "sets-header")}, children...)
}

func setCell(kind string, content node, extra ...attr) node {
	attrs := append([]attr{a("class", "set-cell set-cell-"+kind)}, extra...)
	return el("div", attrs, content)
}

func sharedRow(log models.WorkoutLogWithExercise) node {
	return el("div", []attr{a("class", "set-row")},
		setCell("exercise", text(log.ExerciseName), a("style", "color: var(--text-primary);")),
		setCell("set", text(strconv.Itoa(log.SetNumber))),
		setCell("weight", text(chart.Display(log.Weight))),
		setCell("reps", text(strconv.Itoa(log.Reps))),
		setCell("rpe", rpeCell(log.RPE)),
		setCell("pr", ""),
	)
}
