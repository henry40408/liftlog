package server

import (
	"errors"
	"net/http"
	"strconv"

	"github.com/henry40408/liftlog/internal/chart"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/repo"
	"github.com/henry40408/liftlog/internal/views"
)

// setFields are the reps, weight and RPE of the Add Set and edit forms.
type setFields struct {
	reps   int
	weight float64
	rpe    *int
}

// deserializeError uses a fixed reason: the strconv text would leak Go
// internals to the client.
func deserializeError(field string) error {
	return formError(http.StatusUnprocessableEntity,
		"Failed to deserialize form body: "+field+": invalid number")
}

// parseSetFields reads the numeric fields. A blank RPE means none; reps and
// weight must be present.
func parseSetFields(form formValues) (setFields, error) {
	rawReps, err := form.required("reps")
	if err != nil {
		return setFields{}, err
	}
	reps, err := strconv.ParseInt(rawReps, 10, 32)
	if err != nil {
		return setFields{}, deserializeError("reps")
	}
	rawWeight, err := form.required("weight")
	if err != nil {
		return setFields{}, err
	}
	weight, err := parseFiniteFloat(rawWeight)
	if err != nil {
		return setFields{}, deserializeError("weight")
	}
	rpe, err := parseOptionalInt(form.optional("rpe"))
	if err != nil {
		return setFields{}, deserializeError("rpe")
	}
	return setFields{reps: int(reps), weight: weight, rpe: rpe}, nil
}

func parseOptionalInt(raw string) (*int, error) {
	if raw == "" {
		return nil, nil
	}
	value, err := strconv.ParseInt(raw, 10, 32)
	if err != nil {
		return nil, err
	}
	number := int(value)
	return &number, nil
}

func (a *App) addLog(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	form, err := readForm(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	exerciseID, err := form.required("exercise_id")
	if err != nil {
		a.fail(w, r, err)
		return
	}
	fields, err := parseSetFields(form)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	workout, err := a.ownedWorkout(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	// exercise_id comes from the form, so check it is the caller's too.
	_, err = a.exercises.FindOwned(r.Context(), exerciseID, user.UserID)
	if err != nil {
		a.fail(w, r, orNotFound(err, exerciseNotFound))
		return
	}
	setNumber, err := a.workouts.NextSetNumber(r.Context(), workout.ID, exerciseID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	_, err = a.workouts.CreateLog(r.Context(), repo.NewLog{
		SessionID: workout.ID, ExerciseID: exerciseID, SetNumber: setNumber,
		Reps: fields.reps, Weight: fields.weight, RPE: fields.rpe,
	})
	if err != nil {
		a.fail(w, r, err)
		return
	}
	redirect(w, "/workouts/"+workout.ID)
}

func (a *App) confirmDeleteLog(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	workout, err := a.ownedWorkout(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	// Finding the log among the workout's own proves ownership; a foreign id
	// is a 404, as in deleteLog.
	logs, err := a.workouts.FindLogsBySessionWithPR(r.Context(), workout.ID, user.UserID, a.recentPRStart())
	if err != nil {
		a.fail(w, r, err)
		return
	}
	log, found := findLog(logs, r.PathValue("log_id"))
	if !found {
		a.fail(w, r, notFound("Set not found"))
		return
	}
	writeConfirmPage(w, user, confirmPage{
		ActionLabel: "Delete set",
		Consequence: "Set " + strconv.Itoa(log.SetNumber) + " of " + log.ExerciseName + " — " +
			chart.Display(log.Weight) + " kg × " + strconv.Itoa(log.Reps) + " — will be permanently deleted.",
		FormAction: "/workouts/" + workout.ID + "/logs/" + log.ID + "/delete",
		CancelURL:  "/workouts/" + workout.ID,
	})
}

func findLog(logs []models.WorkoutLogWithExercise, id string) (models.WorkoutLogWithExercise, bool) {
	for _, log := range logs {
		if log.ID == id {
			return log, true
		}
	}
	return models.WorkoutLogWithExercise{}, false
}

func (a *App) deleteLog(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	workout, err := a.ownedWorkout(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if _, err := a.workouts.DeleteLog(r.Context(), r.PathValue("log_id"), workout.ID); err != nil {
		a.fail(w, r, err)
		return
	}
	redirect(w, "/workouts/"+workout.ID)
}

func (a *App) editLogPage(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	workout, err := a.ownedWorkout(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	log, err := a.workouts.FindLogByID(r.Context(), r.PathValue("log_id"))
	if errors.Is(err, repo.ErrNotFound) {
		a.fail(w, r, notFound("Log not found"))
		return
	}
	if err != nil {
		a.fail(w, r, err)
		return
	}
	if log.SessionID != workout.ID {
		a.fail(w, r, notFound("Log not found"))
		return
	}
	exercise, err := a.exercises.FindByID(r.Context(), log.ExerciseID)
	if err != nil {
		a.fail(w, r, orNotFound(err, exerciseNotFound))
		return
	}
	writeHTML(w, http.StatusOK, views.EditLog(views.EditLogView{
		User: user.Nav(), Workout: workout, Log: log, ExerciseName: exercise.Name,
	}))
}

func (a *App) updateLog(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	form, err := readForm(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	fields, err := parseSetFields(form)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	workout, err := a.ownedWorkout(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	_, err = a.workouts.UpdateLog(r.Context(), repo.LogEdit{
		ID: r.PathValue("log_id"), SessionID: workout.ID,
		Reps: fields.reps, Weight: fields.weight, RPE: fields.rpe,
	})
	if err != nil {
		a.fail(w, r, err)
		return
	}
	redirect(w, "/workouts/"+workout.ID)
}
