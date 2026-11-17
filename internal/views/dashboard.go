package views

import (
	"strconv"

	"github.com/henry40408/liftlog/internal/models"
)

// DashboardView is the home page data. TimezoneUnset shows the prompt to pick
// a timezone.
type DashboardView struct {
	User              NavUser
	WorkoutsThisWeek  int64
	WorkoutsThisMonth int64
	TotalVolume       float64
	RecentWorkouts    []models.WorkoutSession
	TimezoneUnset     bool
}

func Dashboard(view DashboardView) string {
	return page("Dashboard - LiftLog", group(
		nav(view.User),
		el("main", nil,
			el("div", []attr{a("class", "page-header")},
				el("h1", nil, text("Dashboard")),
				el("div", []attr{a("class", "subtitle")}, text("Your training overview")),
			),
			el("div", []attr{a("class", "actions actions-lead")},
				el("a", []attr{a("href", "/workouts/new"), a("class", "btn btn-primary")}, text("+ New Workout")),
				text(" "),
				el("a", []attr{a("href", "/stats/prs"), a("class", "btn btn-ghost")}, text("View PRs")),
			),
			timezonePrompt(view.TimezoneUnset),
			el("div", []attr{a("class", "stats-grid")},
				statCard(strconv.FormatInt(view.WorkoutsThisWeek, 10), "This Week"),
				statCard(strconv.FormatInt(view.WorkoutsThisMonth, 10), "This Month"),
				statCard(strconv.FormatFloat(view.TotalVolume, 'f', -1, 64), "Volume (Week)"),
			),
			el("h2", nil, text("Recent Workouts")),
			recentWorkouts(view.RecentWorkouts),
		),
	))
}

func timezonePrompt(unset bool) node {
	if !unset {
		return ""
	}
	return el("div", []attr{a("class", "alert alert-warning"), a("role", "status")},
		el("span", []attr{a("class", "alert-text")}, text("Your timezone is not set, so weekly and monthly stats use UTC dates.")),
		text(" "),
		el("a", []attr{a("href", "/settings#timezone"), a("class", "btn btn-primary btn-sm")}, text("Set timezone")),
	)
}

func statCard(value, label string) node {
	return el("div", []attr{a("class", "stat-card")},
		el("div", []attr{a("class", "stat-value")}, text(value)),
		el("div", []attr{a("class", "stat-label")}, text(label)),
	)
}

func recentWorkouts(workouts []models.WorkoutSession) node {
	if len(workouts) == 0 {
		return el("div", []attr{a("class", "empty-state")},
			el("p", []attr{a("class", "muted")}, text("No workouts yet.")),
			el("a", []attr{a("href", "/workouts/new"), a("class", "btn btn-primary")}, text("Start Your First Workout")),
		)
	}
	items := make([]node, 0, len(workouts))
	for _, w := range workouts {
		items = append(items, recentWorkoutItem(w))
	}
	return group(
		el("div", []attr{a("class", "workout-list")}, items...),
		el("p", []attr{a("class", "mt-4")},
			el("a", []attr{a("href", "/workouts")}, text("View All Workouts "), node("&rarr;"))),
	)
}

func recentWorkoutItem(w models.WorkoutSession) node {
	notes := node("&mdash;")
	if w.Notes != nil {
		notes = text(*w.Notes)
	}
	return el("div", []attr{a("class", "workout-item")},
		el("a", []attr{a("href", "/workouts/"+w.ID)}, text(formatDate(w.Date))),
		text(" "),
		el("span", []attr{a("class", "notes")}, notes),
	)
}
