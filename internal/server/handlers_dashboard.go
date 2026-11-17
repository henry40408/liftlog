package server

import (
	"context"
	"net/http"

	"github.com/henry40408/liftlog/internal/timezone"
	"github.com/henry40408/liftlog/internal/views"
)

// activitySummary is the rolling-window numbers the dashboard and the stats
// overview share.
type activitySummary struct {
	workoutsThisWeek  int
	workoutsThisMonth int
	totalVolume       float64
	// timezone is nil until the user picks one.
	timezone *string
}

// summarizeActivity counts over windows of calendar days in the user's own
// timezone, because workout dates are stored as dates there.
func (a *App) summarizeActivity(ctx context.Context, userID string) (activitySummary, error) {
	zone, err := a.users.FindTimezone(ctx, userID)
	if err != nil {
		return activitySummary{}, err
	}
	today := timezone.DateAt(a.now(), zone)
	weekStart := timezone.WindowStart(today, 7)
	monthStart := timezone.WindowStart(today, 30)

	week, err := a.workouts.CountSessionsSince(ctx, userID, weekStart)
	if err != nil {
		return activitySummary{}, err
	}
	month, err := a.workouts.CountSessionsSince(ctx, userID, monthStart)
	if err != nil {
		return activitySummary{}, err
	}
	volume, err := a.workouts.TotalVolumeSince(ctx, userID, weekStart)
	if err != nil {
		return activitySummary{}, err
	}
	return activitySummary{workoutsThisWeek: week, workoutsThisMonth: month, totalVolume: volume, timezone: zone}, nil
}

func (a *App) dashboard(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	summary, err := a.summarizeActivity(r.Context(), user.UserID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	recent, err := a.workouts.FindSessionsByUserPaginated(r.Context(), user.UserID, 5, 0)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, http.StatusOK, views.Dashboard(views.DashboardView{
		User:              user.Nav(),
		WorkoutsThisWeek:  int64(summary.workoutsThisWeek),
		WorkoutsThisMonth: int64(summary.workoutsThisMonth),
		TotalVolume:       summary.totalVolume,
		RecentWorkouts:    recent,
		TimezoneUnset:     summary.timezone == nil,
	}))
}
