package main

import (
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strconv"
	"time"

	"github.com/henry40408/liftlog/e2e/internal/harness"
)

// demoTimezone is the lifter's timezone: set, so the dashboard does not nag
// about it, and the one the browser is pinned to, so a stamp reads the same on
// every machine.
const demoTimezone = "UTC"

// progressExercise is the lift whose progress page is photographed. It is on
// the first day of the split and gains weight steadily, so its chart climbs.
const progressExercise = "Bench Press"

// weeksOfTraining is how far back the demo history reaches. Sixteen weeks of
// three sessions fill the progress chart's "Last 20" without overflowing it.
const weeksOfTraining = 16

// lift is one exercise in a session: the weight it starts at and gains each
// week, rounded to what the plates allow, and the reps of every set.
type lift struct {
	name     string
	category string
	start    float64
	perWeek  float64
	plate    float64
	reps     []int
}

var (
	pushDay = []lift{
		{"Bench Press", "chest", 62.5, 1.7, 2.5, []int{5, 5, 5, 5}},
		{"Overhead Press", "shoulders", 37.5, 0.9, 2.5, []int{6, 6, 6}},
		{"Incline Dumbbell Press", "chest", 20, 0.8, 2, []int{10, 10, 8}},
		{"Triceps Pushdown", "arms", 25, 0.7, 2.5, []int{12, 12, 10}},
	}
	pullDay = []lift{
		{"Deadlift", "back", 90, 2.3, 2.5, []int{5, 5, 3}},
		{"Barbell Row", "back", 55, 1.2, 2.5, []int{8, 8, 8}},
		{"Lat Pulldown", "back", 50, 0.9, 2.5, []int{10, 10, 10}},
		{"Biceps Curl", "arms", 12, 0.4, 2, []int{12, 10, 10}},
	}
	legDay = []lift{
		{"Back Squat", "legs", 80, 2, 2.5, []int{5, 5, 5, 5}},
		{"Romanian Deadlift", "legs", 70, 1.5, 2.5, []int{8, 8, 8}},
		{"Leg Press", "legs", 140, 3, 5, []int{10, 10, 10}},
		{"Cable Crunch", "core", 35, 0.6, 2.5, []int{15, 12, 12}},
	}
	// split is the rotation: a session is the next day in it, so the newest
	// ones differ from each other.
	split = [][]lift{pushDay, pullDay, legDay}
)

// wobble nudges a week's weight by whole plates, so the chart is not a ruler.
var wobble = []int{0, 0, 1, 0, -1, 0, 1, 0, 0, -1, 0, 1, 0, 0, 0, 1}

// sessionNotes are what the lifter wrote after the newest sessions, newest
// first; older sessions carry none.
var sessionNotes = []string{
	"Squats moved fast today, depth felt solid.",
	"Deadlift grip held up, no straps needed.",
	"New bench PR, the last rep was slow but clean.",
	"Slept badly but showed up anyway.",
	"Rows felt heavy. Deload next week if it stays this way.",
	"Good session, shoulders warmed up quickly.",
}

// daysBetweenSessions spaces three sessions a week: two days apart, then a
// longer rest. It is how many days separate a session from the one before.
var daysBetweenSessions = []int{2, 2, 3}

// demoRoutes are the pages the screenshots open, where the path is not fixed.
type demoRoutes struct {
	progress string
}

var exerciseLink = regexp.MustCompile(`href="/stats/exercise/([^"]+)"[^>]*>([^<]+)<`)

// demoClient posts forms as the signed-in lifter.
type demoClient struct {
	http *http.Client
	base string
}

// seedDemo creates the lifter and sixteen weeks of training through the real
// endpoints, so the pages show what the app itself writes.
func seedDemo(server *harness.Server) (demoRoutes, error) {
	if err := server.EnsureUser(harness.AdminUsername, harness.Password); err != nil {
		return demoRoutes{}, err
	}
	signedIn, err := server.SignIn(harness.AdminUsername, harness.Password)
	if err != nil {
		return demoRoutes{}, err
	}
	client := demoClient{http: signedIn, base: server.BaseURL}
	if _, err := client.post("/settings/timezone", url.Values{"timezone": {demoTimezone}}); err != nil {
		return demoRoutes{}, err
	}
	exercises, err := client.createExercises()
	if err != nil {
		return demoRoutes{}, err
	}
	if err := client.train(exercises); err != nil {
		return demoRoutes{}, err
	}
	return demoRoutes{progress: "/stats/exercise/" + exercises[progressExercise]}, nil
}

// post sends a form and returns where the server redirects to, which is the
// new workout's page after /workouts.
func (c demoClient) post(path string, form url.Values) (string, error) {
	response, err := c.http.PostForm(c.base+path, form)
	if err != nil {
		return "", fmt.Errorf("posting %s: %w", path, err)
	}
	defer func() { _ = response.Body.Close() }()
	if response.StatusCode >= 400 {
		return "", fmt.Errorf("posting %s answered %d", path, response.StatusCode)
	}
	return response.Header.Get("Location"), nil
}

// createExercises adds every exercise of the split and returns their ids.
func (c demoClient) createExercises() (map[string]string, error) {
	for _, day := range split {
		for _, entry := range day {
			form := url.Values{"name": {entry.name}, "category": {entry.category}}
			if _, err := c.post("/exercises", form); err != nil {
				return nil, err
			}
		}
	}
	return c.exerciseIDs()
}

// exerciseIDs reads each exercise's id off its link on /exercises.
func (c demoClient) exerciseIDs() (map[string]string, error) {
	response, err := c.http.Get(c.base + "/exercises")
	if err != nil {
		return nil, err
	}
	defer func() { _ = response.Body.Close() }()
	page, err := io.ReadAll(response.Body)
	if err != nil {
		return nil, err
	}
	ids := map[string]string{}
	for _, match := range exerciseLink.FindAllStringSubmatch(string(page), -1) {
		ids[match[2]] = match[1]
	}
	return ids, nil
}

// train logs the whole history, oldest session first.
func (c demoClient) train(exercises map[string]string) error {
	dates := sessionDates(time.Now().UTC(), weeksOfTraining*len(daysBetweenSessions))
	for index, date := range dates {
		plan := sessionPlan{
			date:  date,
			notes: notesFor(len(dates) - 1 - index),
			week:  index / len(split),
			day:   split[index%len(split)],
		}
		if err := c.logSession(plan, exercises); err != nil {
			return err
		}
	}
	return nil
}

// sessionPlan is one workout to log.
type sessionPlan struct {
	date  time.Time
	notes string
	week  int
	day   []lift
}

// sessionDates is the date of each session, oldest first, the newest one day
// before today.
func sessionDates(today time.Time, sessions int) []time.Time {
	dates := make([]time.Time, sessions)
	daysAgo := 1
	for index := sessions - 1; index >= 0; index-- {
		dates[index] = today.AddDate(0, 0, -daysAgo)
		daysAgo += daysBetweenSessions[index%len(daysBetweenSessions)]
	}
	return dates
}

func notesFor(sessionsAgo int) string {
	if sessionsAgo >= len(sessionNotes) {
		return ""
	}
	return sessionNotes[sessionsAgo]
}

func (c demoClient) logSession(plan sessionPlan, exercises map[string]string) error {
	form := url.Values{"date": {plan.date.Format("2006-01-02")}, "notes": {plan.notes}}
	path, err := c.post("/workouts", form)
	if err != nil {
		return err
	}
	for position, entry := range plan.day {
		weight := weightIn(entry, plan.week, position)
		if err := c.logSets(path, exercises[entry.name], entry.reps, weight); err != nil {
			return err
		}
	}
	return nil
}

// weightIn is the working weight of a lift in a week: its start, plus what it
// gains each week, plus a plate either way, snapped to the plates. A lift's
// position in the session shifts where it sits in wobble, so the lifts of one
// session do not all dip together.
func weightIn(entry lift, week, position int) float64 {
	nudge := float64(wobble[(week+position)%len(wobble)]) * entry.plate
	raw := entry.start + entry.perWeek*float64(week) + nudge
	return float64(int(raw/entry.plate+0.5)) * entry.plate
}

// logSets adds one set per entry of reps, harder as the sets go on.
func (c demoClient) logSets(workoutPath, exerciseID string, reps []int, weight float64) error {
	for index, count := range reps {
		form := url.Values{
			"exercise_id": {exerciseID},
			"weight":      {strconv.FormatFloat(weight, 'f', -1, 64)},
			"reps":        {strconv.Itoa(count)},
			"rpe":         {strconv.Itoa(7 + index*3/len(reps))},
		}
		if _, err := c.post(workoutPath+"/logs", form); err != nil {
			return err
		}
	}
	return nil
}
