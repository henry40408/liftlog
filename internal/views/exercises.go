package views

import "github.com/henry40408/liftlog/internal/models"

// ExercisesList groups the exercises under every category, in category order.
func ExercisesList(user NavUser, exercises []models.Exercise, categories []models.ExerciseCategory) string {
	sections := make([]node, 0, len(categories))
	for _, cat := range categories {
		sections = append(sections, categorySection(cat, exercises))
	}
	return page("Exercises - LiftLog", group(
		nav(user),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text("Exercises"))),
			el("p", []attr{a("class", "mb-6")},
				el("a", []attr{a("href", "/exercises/new"), a("class", "btn btn-primary")}, text("+ Add Exercise"))),
			group(sections...),
		),
	))
}

func categorySection(cat models.ExerciseCategory, exercises []models.Exercise) node {
	items := []node{el("div", []attr{a("class", "category-title")}, text(cat.DisplayName))}
	for _, ex := range exercises {
		if ex.Category == cat.Name {
			items = append(items, exerciseItem(ex))
		}
	}
	return el("div", []attr{a("class", "category-section")}, items...)
}

func exerciseItem(ex models.Exercise) node {
	return el("div", []attr{a("class", "exercise-item")},
		el("a", []attr{a("href", "/stats/exercise/"+ex.ID)}, text(ex.Name)),
		el("div", []attr{a("class", "actions")},
			el("a", []attr{a("href", "/exercises/"+ex.ID+"/edit"), a("class", "btn btn-ghost btn-sm")}, text("Edit")),
			text(" "),
			el("a", []attr{a("href", "/exercises/"+ex.ID+"/delete"), a("class", "btn btn-danger btn-sm"),
				a("data-confirm", "Delete "+ex.Name+"?")}, text("Delete")),
		),
	)
}

// NewExercise renders the add form; errMsg is empty when there is none.
func NewExercise(user NavUser, categories []models.ExerciseCategory, errMsg string) string {
	return exerciseForm(exerciseFormView{
		user:       user,
		title:      "New Exercise - LiftLog",
		heading:    "Add Exercise",
		action:     "/exercises",
		submit:     "Add Exercise",
		categories: categories,
		errMsg:     errMsg,
	})
}

// EditExercise renders the edit form for exercise.
func EditExercise(user NavUser, exercise models.Exercise, categories []models.ExerciseCategory, errMsg string) string {
	return exerciseForm(exerciseFormView{
		user:       user,
		title:      "Edit Exercise - LiftLog",
		heading:    "Edit Exercise",
		action:     "/exercises/" + exercise.ID,
		submit:     "Save Changes",
		categories: categories,
		errMsg:     errMsg,
		current:    &exercise,
	})
}

// exerciseFormView is shared by the add and edit pages; current is nil on add.
type exerciseFormView struct {
	user                                   NavUser
	title, heading, action, submit, errMsg string
	categories                             []models.ExerciseCategory
	current                                *models.Exercise
}

func exerciseForm(view exerciseFormView) string {
	return page(view.title, group(
		nav(view.user),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text(view.heading))),
			errorBox(view.errMsg),
			el("form", []attr{a("method", "post"), a("action", view.action)},
				labelledInput("Name", "name", nameInput(view.current)),
				labelledInput("Category", "category", el("select",
					[]attr{a("id", "category"), a("name", "category"), flag("required")},
					categoryOptions(view.categories, view.current)...)),
				el("button", []attr{a("type", "submit")}, text(view.submit)),
			),
			backLink("/exercises", "Back to Exercises"),
		),
	))
}

func nameInput(current *models.Exercise) node {
	attrs := []attr{a("type", "text"), a("id", "name"), a("name", "name")}
	if current != nil {
		attrs = append(attrs, a("value", current.Name))
	}
	return void("input", append(attrs, flag("required"), flag("autofocus"))...)
}

func categoryOptions(categories []models.ExerciseCategory, current *models.Exercise) []node {
	options := make([]node, 0, len(categories))
	for _, cat := range categories {
		attrs := []attr{a("value", cat.Name)}
		if current != nil && current.Category == cat.Name {
			attrs = append(attrs, flag("selected"))
		}
		options = append(options, el("option", attrs, text(cat.DisplayName)))
	}
	return options
}
