package server

import (
	"errors"
	"math"
	"net/http"
	"net/url"
	"slices"
	"strconv"
	"strings"
	"time"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/repo"
	"github.com/henry40408/liftlog/internal/timezone"
	"github.com/henry40408/liftlog/internal/views"
)

const workoutsPerPage = 10

const workoutNotFound = "Workout not found"

// orNotFound turns the repository's missing-row error into the 404 the
// handlers show; any other error passes through.
func orNotFound(err error, message string) error {
	if errors.Is(err, repo.ErrNotFound) {
		return notFound(message)
	}
	return err
}

// ownedWorkout is a workout of the signed-in user. A missing and a foreign
// workout are the same 404, so existence is not disclosed.
func (a *App) ownedWorkout(r *http.Request, user ValidatedSession) (models.WorkoutSession, error) {
	workout, err := a.workouts.FindOwnedSession(r.Context(), r.PathValue("id"), user.UserID)
	return workout, orNotFound(err, workoutNotFound)
}

// maxPage keeps (page-1)*workoutsPerPage far from int overflow; no real
// journal has this many pages.
const maxPage = 1_000_000

// parsePage reads ?page=, which defaults to 1 and is clamped to 1..maxPage.
func parsePage(query url.Values) (int, error) {
	if !query.Has("page") {
		return 1, nil
	}
	page, err := strconv.ParseInt(query.Get("page"), 10, 64)
	if err != nil {
		return 0, badRequest("Failed to deserialize query string: page: invalid number")
	}
	return int(min(max(page, 1), maxPage)), nil
}

func (a *App) workoutsList(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	page, err := parsePage(r.URL.Query())
	if err != nil {
		a.fail(w, r, err)
		return
	}
	workouts, err := a.workouts.FindSessionsByUserPaginated(r.Context(), user.UserID, workoutsPerPage, (page-1)*workoutsPerPage)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	total, err := a.workouts.CountSessionsByUser(r.Context(), user.UserID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, http.StatusOK, views.WorkoutsList(views.WorkoutsListView{
		User:       user.Nav(),
		Workouts:   workouts,
		Page:       page,
		TotalPages: (total + workoutsPerPage - 1) / workoutsPerPage,
	}))
}

func (a *App) newWorkoutPage(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	zone, err := a.users.FindTimezone(r.Context(), user.UserID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, http.StatusOK, views.NewWorkout(views.NewWorkoutView{
		User:            user.Nav(),
		Today:           timezone.TodayIn(zone),
		UseBrowserToday: zone == nil,
	}))
}

// workoutFields is the date and notes of the new and edit forms.
type workoutFields struct {
	date  time.Time
	notes *string
}

func readWorkoutFields(w http.ResponseWriter, r *http.Request) (workoutFields, error) {
	form, err := readForm(w, r)
	if err != nil {
		return workoutFields{}, err
	}
	rawDate, err := form.required("date")
	if err != nil {
		return workoutFields{}, err
	}
	date, err := db.ParseDate(rawDate)
	if err != nil {
		return workoutFields{}, formError(http.StatusUnprocessableEntity,
			"Failed to deserialize form body: date: invalid date")
	}
	return workoutFields{date: date, notes: optionalField(form, "notes")}, nil
}

func (a *App) createWorkout(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	fields, err := readWorkoutFields(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	workout, err := a.workouts.CreateSession(r.Context(), user.UserID, fields.date, fields.notes)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	redirect(w, "/workouts/"+workout.ID)
}

func (a *App) showWorkout(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	workout, err := a.ownedWorkout(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	view, err := a.workoutPageView(r, user, workout)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, http.StatusOK, views.ShowWorkout(view))
}

func (a *App) editWorkoutPage(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	workout, err := a.ownedWorkout(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, http.StatusOK, views.EditWorkout(user.Nav(), workout, ""))
}

func (a *App) updateWorkout(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	fields, err := readWorkoutFields(w, r)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	id := r.PathValue("id")
	_, err = a.workouts.UpdateSession(r.Context(), repo.SessionEdit{
		ID: id, UserID: user.UserID, Date: &fields.date, Notes: fields.notes,
	})
	if err != nil {
		a.fail(w, r, err)
		return
	}
	redirect(w, "/workouts/"+id)
}

// setCountSentence tells what deleting the workout takes with it.
func setCountSentence(sets int) string {
	switch sets {
	case 0:
		return "It has no sets recorded."
	case 1:
		return "Its 1 recorded set will be deleted with it."
	default:
		return "All " + strconv.Itoa(sets) + " of its recorded sets will be deleted with it."
	}
}

func (a *App) confirmDeleteWorkout(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	workout, err := a.ownedWorkout(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	logs, err := a.workouts.FindLogsBySessionWithPR(r.Context(), workout.ID, user.UserID, a.recentPRStart())
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeConfirmPage(w, user, confirmPage{
		ActionLabel: "Delete workout",
		Consequence: "The workout on " + db.FormatDate(workout.Date) + " will be permanently deleted. " +
			setCountSentence(len(logs)) + " This cannot be undone.",
		FormAction: "/workouts/" + workout.ID + "/delete",
		CancelURL:  "/workouts/" + workout.ID,
	})
}

// deleteWorkout needs no separate ownership lookup: the delete is scoped to
// the user, so a foreign id deletes nothing.
func (a *App) deleteWorkout(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	if _, err := a.workouts.DeleteSession(r.Context(), r.PathValue("id"), user.UserID); err != nil {
		a.fail(w, r, err)
		return
	}
	redirect(w, "/workouts")
}

func (a *App) recentPRStart() time.Time {
	return models.RecentPRWindowStart(a.now())
}

// optionalField is nil when the form lacks the field; a blank value is kept.
func optionalField(form formValues, name string) *string {
	if !form.values.Has(name) {
		return nil
	}
	value := form.values.Get(name)
	return &value
}

// workoutPageView gathers everything the workout page shows.
func (a *App) workoutPageView(r *http.Request, user ValidatedSession, workout models.WorkoutSession) (views.ShowWorkoutView, error) {
	ctx := r.Context()
	logs, err := a.workouts.FindLogsBySessionWithPR(ctx, workout.ID, user.UserID, a.recentPRStart())
	if err != nil {
		return views.ShowWorkoutView{}, err
	}
	exercises, err := a.exercises.FindAvailableForUser(ctx, user.UserID)
	if err != nil {
		return views.ShowWorkoutView{}, err
	}
	lastWeights, err := a.workouts.LastWeightPerExerciseByUser(ctx, user.UserID)
	if err != nil {
		return views.ShowWorkoutView{}, err
	}
	return views.ShowWorkoutView{
		User:                user.Nav(),
		Workout:             workout,
		Logs:                logs,
		Exercises:           exercises,
		Categories:          models.Categories,
		ExerciseLastWeights: lastWeights,
		LastWeightRows:      lastWeightRows(exercises, lastWeights),
		WeightSuggestions:   weightSuggestions(logs, lastWeights),
		RepSuggestions:      models.RepSchemes,
		Prefill:             prefillFrom(logs, r.URL.Query().Get("prefill")),
		Share:               shareInfo(workout, a.now()),
	}, nil
}

// prefillFrom looks the log up among the workout's own logs only, so a
// foreign or unknown id fills nothing.
func prefillFrom(logs []models.WorkoutLogWithExercise, logID string) *views.PrefillSet {
	log, found := findLog(logs, logID)
	if !found {
		return nil
	}
	return &views.PrefillSet{ExerciseID: log.ExerciseID, Weight: log.Weight, Reps: log.Reps, RPE: log.RPE}
}

// lastWeightRows lists each exercise's last weight, sorted flat by name:
// exercises arrive ordered by category.
func lastWeightRows(exercises []models.Exercise, last []models.LastExerciseWeight) []views.LastWeightRow {
	var rows []views.LastWeightRow
	for _, exercise := range exercises {
		index := slices.IndexFunc(last, func(l models.LastExerciseWeight) bool { return l.ExerciseID == exercise.ID })
		if index < 0 {
			continue
		}
		rows = append(rows, views.LastWeightRow{
			ExerciseName: exercise.Name,
			Weight:       last[index].Weight,
			RPE:          last[index].RPE,
			LoggedAt:     last[index].LoggedAt,
		})
	}
	slices.SortStableFunc(rows, func(a, b views.LastWeightRow) int {
		return strings.Compare(a.ExerciseName, b.ExerciseName)
	})
	return rows
}

// weightSuggestions are the Add Set datalist weights, ascending and distinct:
// this workout's sets (the next set usually repeats one) plus each exercise's
// last weight (the first set of a lift). Zero is how bodyweight sets are
// stored, so it is not a pickable weight.
func weightSuggestions(logs []models.WorkoutLogWithExercise, last []models.LastExerciseWeight) []float64 {
	var weights []float64
	for _, log := range logs {
		weights = append(weights, log.Weight)
	}
	for _, entry := range last {
		weights = append(weights, entry.Weight)
	}
	weights = slices.DeleteFunc(weights, func(w float64) bool {
		return w <= 0 || math.IsNaN(w) || math.IsInf(w, 0)
	})
	slices.Sort(weights)
	return slices.Compact(weights)
}

// shareInfo is nil unless the workout has a live share link; an expired token
// may linger until the hourly sweep clears it.
func shareInfo(workout models.WorkoutSession, now time.Time) *views.ShareInfo {
	if workout.ShareToken == nil {
		return nil
	}
	if workout.ShareExpiresAt != nil && !workout.ShareExpiresAt.After(now) {
		return nil
	}
	return &views.ShareInfo{URL: "/shared/" + *workout.ShareToken, ExpiresAt: workout.ShareExpiresAt}
}
