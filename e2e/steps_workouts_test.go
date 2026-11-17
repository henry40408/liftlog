package e2e

import (
	"fmt"
	"slices"
	"strconv"

	"github.com/cucumber/godog"

	"github.com/henry40408/liftlog/e2e/internal/harness"
)

func registerExerciseSteps(sc *godog.ScenarioContext, w *world) {
	sc.Step(`^I create a new exercise in category `+quoted+`$`, w.createExercise)
	sc.Step(`^I have an exercise in category `+quoted+`$`, w.haveExercise)
	sc.Step(`^the exercise I created is listed on the exercises page$`, w.exerciseIsListed)
	sc.Step(`^my exercise is still listed on the exercises page$`, w.exerciseIsListed)
	sc.Step(`^I rename my exercise$`, w.renameExercise)
	sc.Step(`^I delete my exercise$`, w.deleteExercise)
	sc.Step(`^my exercise is no longer listed on the exercises page$`, w.exerciseIsGone)
}

func (w *world) createExercise(category string) error {
	return w.addExercise(w.unique("Squat"), category)
}

func (w *world) haveExercise(category string) error {
	return w.addExercise(w.unique("Exercise"), category)
}

// addExercise creates an exercise and remembers what the server called it.
func (w *world) addExercise(name, category string) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	id, err := b.Exercises().Create(name, category)
	if err != nil {
		return err
	}
	w.exerciseName, w.exerciseID = name, id
	return nil
}

func (w *world) exerciseIsListed() error {
	return w.exerciseEntries(1)
}

func (w *world) exerciseIsGone() error {
	return w.exerciseEntries(0)
}

func (w *world) exerciseEntries(want int) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.requireExercise(); err != nil {
		return err
	}
	exercises := b.Exercises()
	if err := exercises.Goto(); err != nil {
		return err
	}
	return harness.EventuallyEq("entries naming my exercise", want, func() (int, error) {
		return exercises.EntriesNamed(w.exerciseName)
	})
}

func (w *world) renameExercise() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.requireExercise(); err != nil {
		return err
	}
	renamed := w.unique("Renamed")
	if err := b.Exercises().Rename(w.exerciseName, renamed); err != nil {
		return err
	}
	if err := w.urlIs("/exercises"); err != nil {
		return err
	}
	w.exerciseName = renamed
	return nil
}

func (w *world) deleteExercise() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	if err := w.requireExercise(); err != nil {
		return err
	}
	if err := b.Exercises().Delete(w.exerciseName); err != nil {
		return err
	}
	return w.urlIs("/exercises")
}

func registerWorkoutSteps(sc *godog.ScenarioContext, w *world) {
	sc.Step(`^I start a new workout for today$`, w.createWorkout)
	sc.Step(`^I have a workout$`, w.createWorkout)
	sc.Step(`^I have a workout with a set of (\d+) kg for (\d+) reps$`, w.haveWorkoutWithSet)
	sc.Step(`^I delete the workout$`, w.deleteWorkout)
	sc.Step(`^I edit the workout to date `+quoted+` with notes `+quoted+`$`, w.editWorkout)
	sc.Step(`^I am on the workout detail page$`, w.onTheWorkout)
	sc.Step(`^the workout I created is listed on the workouts page$`, w.workoutIsListed)
	sc.Step(`^the workout I deleted is not listed on the workouts page$`, w.workoutIsNotListed)
	sc.Step(`^I do not see the workout I created on the workouts page$`, w.workoutIsNotListed)
	sc.Step(`^the workout detail shows date `+quoted+` and notes `+quoted+`$`, w.workoutShowsDateAndNotes)
	sc.Step(`^I see the workouts empty state$`, w.seeEmptyState)
}

func (w *world) createWorkout() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	id, err := b.CreateWorkoutToday()
	if err != nil {
		return err
	}
	w.workoutID = id
	return nil
}

func (w *world) haveWorkoutWithSet(weight, reps int) error {
	if err := w.requireExercise(); err != nil {
		return fmt.Errorf("this step requires an exercise to exist first: %w", err)
	}
	if err := w.createWorkout(); err != nil {
		return err
	}
	workout, err := w.workout()
	if err != nil {
		return err
	}
	return workout.LogSet(harness.SetEntry{Exercise: w.exerciseName, Weight: fmt.Sprint(weight), Reps: fmt.Sprint(reps)})
}

func (w *world) deleteWorkout() error {
	workout, err := w.workout()
	if err != nil {
		return err
	}
	if err := workout.Goto(); err != nil {
		return err
	}
	if err := workout.Delete(); err != nil {
		return err
	}
	return w.urlIs("/workouts")
}

func (w *world) editWorkout(date, notes string) error {
	workout, err := w.workout()
	if err != nil {
		return err
	}
	if err := workout.SaveMetadata(date, notes); err != nil {
		return err
	}
	return w.onTheWorkout()
}

// onTheWorkout waits for the browser to be back on the scenario's workout.
func (w *world) onTheWorkout() error {
	if w.workoutID == "" {
		return fmt.Errorf("this step needs a workout, and the scenario has not created one")
	}
	return w.urlIs("/workouts/" + w.workoutID)
}

func (w *world) workoutIsListed() error {
	return w.workoutLinks(1)
}

func (w *world) workoutIsNotListed() error {
	return w.workoutLinks(0)
}

func (w *world) workoutLinks(want int) error {
	if w.workoutID == "" {
		return fmt.Errorf("this step needs a workout, and the scenario has not created one")
	}
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	workouts := b.Workouts()
	if err := workouts.Goto(); err != nil {
		return err
	}
	return harness.EventuallyEq("links to my workout", want, func() (int, error) {
		return workouts.LinksTo(w.workoutID)
	})
}

func (w *world) workoutShowsDateAndNotes(date, notes string) error {
	workout, err := w.workout()
	if err != nil {
		return err
	}
	if err := workout.Goto(); err != nil {
		return err
	}
	if err := harness.EventuallyEq("the workout heading", date, workout.Heading); err != nil {
		return err
	}
	return harness.Eventually("the workout notes", func() (bool, error) {
		text, found, err := workout.Notes()
		return found && text == notes, err
	})
}

func (w *world) seeEmptyState() error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	workouts := b.Workouts()
	if err := workouts.Goto(); err != nil {
		return err
	}
	return eventuallyContains("the empty state", "No workouts yet", workouts.EmptyState)
}

func registerSetSteps(sc *godog.ScenarioContext, w *world) {
	sc.Step(`^the (\w+) field suggests `+quoted+`$`, w.fieldSuggests)
	sc.Step(`^I log a set of (\d+) kg for (\d+) reps using the exercise I created$`, w.logSet)
	sc.Step(`^I log a set of (\d+) kg for (\d+) reps with RPE (\d+) using the exercise I created$`, w.logSetWithRPE)
	sc.Step(`^I log another set of (\d+) kg for (\d+) reps using the same exercise$`, w.logSet)
	sc.Step(`^I edit my set to (\d+) kg for (\d+) reps$`, w.editSet)
	sc.Step(`^I delete my set$`, w.deleteSet)
	sc.Step(`^I select the exercise I created on the workout page$`, w.selectExercise)
	sc.Step(`^I click clone on my set$`, w.cloneSet)
	sc.Step(`^I see my set logged at (\d+) kg for (\d+) reps$`, w.seeSetLogged)
	sc.Step(`^I see my set logged with RPE (\d+)$`, w.seeSetRPE)
	sc.Step(`^my set is no longer shown on the workout$`, w.setIsGone)
	sc.Step(`^my set is flagged as a PR$`, w.setIsAPR)
	sc.Step(`^I see two sets numbered 1 and 2$`, w.twoNumberedSets)
	sc.Step(`^the Last info shows `+quoted+`$`, w.lastInfoShows)
	sc.Step(`^the Add Set form is pre-filled with weight (\d+) and reps (\d+)$`, w.addSetFormPrefilled)
}

// openWorkout is the scenario's workout page with the browser on it.
func (w *world) openWorkout() (harness.WorkoutPage, error) {
	workout, err := w.workout()
	if err != nil {
		return workout, err
	}
	if err := w.requireExercise(); err != nil {
		return workout, err
	}
	return workout, workout.Goto()
}

func (w *world) fieldSuggests(field, value string) error {
	workout, err := w.workout()
	if err != nil {
		return err
	}
	offered, err := workout.Suggestions(field)
	if err != nil {
		return err
	}
	if !slices.Contains(offered, value) {
		return fmt.Errorf("the `%s` field offers %q, which does not include %q", field, offered, value)
	}
	return nil
}

func (w *world) logSet(weight, reps int) error {
	return w.logSetEntry(fmt.Sprint(weight), fmt.Sprint(reps), "")
}

func (w *world) logSetWithRPE(weight, reps, rpe int) error {
	return w.logSetEntry(fmt.Sprint(weight), fmt.Sprint(reps), fmt.Sprint(rpe))
}

func (w *world) logSetEntry(weight, reps, rpe string) error {
	workout, err := w.openWorkout()
	if err != nil {
		return err
	}
	return workout.LogSet(harness.SetEntry{Exercise: w.exerciseName, Weight: weight, Reps: reps, RPE: rpe})
}

func (w *world) editSet(weight, reps int) error {
	b, err := w.browserOf()
	if err != nil {
		return err
	}
	workout, err := w.openWorkout()
	if err != nil {
		return err
	}
	if err := workout.OpenEditSet(w.exerciseName); err != nil {
		return err
	}
	if err := b.SaveEditedSet(fmt.Sprint(weight), fmt.Sprint(reps)); err != nil {
		return err
	}
	return w.onTheWorkout()
}

func (w *world) deleteSet() error {
	workout, err := w.openWorkout()
	if err != nil {
		return err
	}
	if err := workout.DeleteSet(w.exerciseName); err != nil {
		return err
	}
	return w.onTheWorkout()
}

func (w *world) selectExercise() error {
	workout, err := w.openWorkout()
	if err != nil {
		return err
	}
	return workout.SelectExercise(w.exerciseName)
}

func (w *world) cloneSet() error {
	workout, err := w.openWorkout()
	if err != nil {
		return err
	}
	return workout.CloneSet(w.exerciseName)
}

func (w *world) seeSetLogged(weight, reps int) error {
	workout, err := w.openWorkout()
	if err != nil {
		return err
	}
	if err := w.eventuallyCell(workout, "weight", weight); err != nil {
		return err
	}
	return w.eventuallyCell(workout, "reps", reps)
}

func (w *world) seeSetRPE(rpe int) error {
	workout, err := w.openWorkout()
	if err != nil {
		return err
	}
	return w.eventuallyCell(workout, "rpe", rpe)
}

func (w *world) eventuallyCell(workout harness.WorkoutPage, cell string, want int) error {
	return harness.EventuallyEq("the "+cell+" cell", strconv.Itoa(want), func() (string, error) {
		return workout.Cell(w.exerciseName, cell)
	})
}

func (w *world) setIsGone() error {
	workout, err := w.openWorkout()
	if err != nil {
		return err
	}
	return harness.EventuallyEq("set rows for my exercise", 0, workout.RowCount(w.exerciseName))
}

// setIsAPR: a first-ever set is both the all-time and the 1-month best; the
// all-time badge wins, so the row shows "PR" rather than "PR 1M".
func (w *world) setIsAPR() error {
	workout, err := w.openWorkout()
	if err != nil {
		return err
	}
	return harness.Eventually("the PR badge reads PR", func() (bool, error) {
		badge, found, err := workout.PRBadge(w.exerciseName)
		return found && badge == "PR", err
	})
}

func (w *world) twoNumberedSets() error {
	workout, err := w.openWorkout()
	if err != nil {
		return err
	}
	return harness.Eventually("the set numbers are 1 and 2", func() (bool, error) {
		numbers, err := workout.SetNumbers(w.exerciseName)
		return slices.Equal(numbers, []string{"1", "2"}), err
	})
}

func (w *world) lastInfoShows(text string) error {
	workout, err := w.workout()
	if err != nil {
		return err
	}
	return harness.Eventually("the Last info RPE chip reads "+text, func() (bool, error) {
		chip, found, err := workout.LastInfoRPE()
		return found && chip == text, err
	})
}

func (w *world) addSetFormPrefilled(weight, reps int) error {
	if err := w.requireExercise(); err != nil {
		return err
	}
	workout, err := w.workout()
	if err != nil {
		return err
	}
	if err := harness.EventuallyEq("the selected exercise", w.exerciseID, workout.PrefilledExercise); err != nil {
		return err
	}
	if err := harness.EventuallyEq("the weight field", strconv.Itoa(weight), workout.PrefilledWeight); err != nil {
		return err
	}
	return harness.EventuallyEq("the reps field", strconv.Itoa(reps), workout.PrefilledReps)
}
