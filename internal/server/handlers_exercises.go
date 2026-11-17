package server

import (
	"errors"
	"net/http"
	"strings"

	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/repo"
	"github.com/henry40408/liftlog/internal/views"
)

const (
	exerciseNotFound  = "Exercise not found"
	exerciseNameError = "Exercise name is required"
)

func (a *App) ownedExercise(r *http.Request, user ValidatedSession) (models.Exercise, error) {
	exercise, err := a.exercises.FindOwned(r.Context(), r.PathValue("id"), user.UserID)
	return exercise, orNotFound(err, exerciseNotFound)
}

func (a *App) exercisesList(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	exercises, err := a.exercises.FindAvailableForUser(r.Context(), user.UserID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, http.StatusOK, views.ExercisesList(user.Nav(), exercises, models.Categories))
}

func (a *App) newExercisePage(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	writeHTML(w, http.StatusOK, views.NewExercise(user.Nav(), models.Categories, ""))
}

// exerciseFields are the name and category of the exercise forms.
type exerciseFields struct {
	name     string
	category string
}

func readExerciseFields(w http.ResponseWriter, r *http.Request) (exerciseFields, error) {
	form, err := readForm(w, r)
	if err != nil {
		return exerciseFields{}, err
	}
	name, err := form.required("name")
	if err != nil {
		return exerciseFields{}, err
	}
	category, err := form.required("category")
	if err != nil {
		return exerciseFields{}, err
	}
	return exerciseFields{name: name, category: category}, nil
}

func (a *App) createExercise(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	fields, err := readExerciseFields(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if strings.TrimSpace(fields.name) == "" {
		writeHTML(w, http.StatusOK, views.NewExercise(user.Nav(), models.Categories, exerciseNameError))
		return
	}
	_, err = a.exercises.Create(r.Context(), repo.NewExercise{
		Name: fields.name, Category: fields.category, UserID: user.UserID,
	})
	if err != nil {
		a.fail(w, r, err)
		return
	}
	redirect(w, "/exercises")
}

func (a *App) editExercisePage(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	exercise, err := a.ownedExercise(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, http.StatusOK, views.EditExercise(user.Nav(), exercise, models.Categories, ""))
}

func (a *App) updateExercise(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	fields, err := readExerciseFields(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	exercise, err := a.ownedExercise(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if strings.TrimSpace(fields.name) == "" {
		writeHTML(w, http.StatusOK, views.EditExercise(user.Nav(), exercise, models.Categories, exerciseNameError))
		return
	}
	_, err = a.exercises.Update(r.Context(), repo.ExerciseEdit{
		ID: exercise.ID, UserID: user.UserID, Name: fields.name, Category: fields.category,
	})
	if err != nil {
		a.fail(w, r, err)
		return
	}
	redirect(w, "/exercises")
}

func (a *App) confirmDeleteExercise(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	exercise, err := a.ownedExercise(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeConfirmPage(w, user, confirmPage{
		ActionLabel: "Delete exercise",
		Consequence: exercise.Name + " will be permanently deleted. " +
			"If any workout still logs it, the deletion will be refused instead.",
		FormAction: "/exercises/" + exercise.ID + "/delete",
		CancelURL:  "/exercises",
	})
}

func (a *App) deleteExercise(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	exercise, err := a.ownedExercise(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	_, err = a.exercises.Delete(r.Context(), exercise.ID, user.UserID)
	if errors.Is(err, repo.ErrExerciseInUse) {
		a.fail(w, r, badRequest(err.Error()))
		return
	}
	if err != nil {
		a.fail(w, r, err)
		return
	}
	redirect(w, "/exercises")
}
