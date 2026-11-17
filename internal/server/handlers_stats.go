package server

import (
	"errors"
	"net/http"

	"github.com/henry40408/liftlog/internal/chart"
	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/repo"
	"github.com/henry40408/liftlog/internal/views"
)

// historyLimit is how many of the newest sets the exercise page lists.
const historyLimit = 50

func (a *App) statsIndex(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	summary, err := a.summarizeActivity(r.Context(), user.UserID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	total, err := a.workouts.CountSessionsByUser(r.Context(), user.UserID)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	prs, err := a.workouts.PRSummariesByUser(r.Context(), user.UserID, models.RecentPRWindowStart(a.now()))
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, http.StatusOK, views.StatsIndex(views.StatsView{
		User:              user.Nav(),
		WorkoutsThisWeek:  summary.workoutsThisWeek,
		WorkoutsThisMonth: summary.workoutsThisMonth,
		TotalVolume:       summary.totalVolume,
		TotalWorkouts:     total,
		PRs:               prs,
	}))
}

func (a *App) prsList(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	prs, err := a.workouts.PRSummariesByUser(r.Context(), user.UserID, models.RecentPRWindowStart(a.now()))
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, http.StatusOK, views.PRs(user.Nav(), prs))
}

func (a *App) exerciseStats(w http.ResponseWriter, r *http.Request) {
	user, ok := requireUser(w, r)
	if !ok {
		return
	}
	view, err := a.exerciseStatsView(r, user)
	if err != nil {
		a.fail(w, r, err)
		return
	}
	writeHTML(w, http.StatusOK, views.ExerciseStats(view))
}

func (a *App) exerciseStatsView(r *http.Request, user ValidatedSession) (views.ExerciseStatsView, error) {
	ctx := r.Context()
	exerciseID := r.PathValue("id")
	// Scoped fetch: another user's exercise is a 404, never a 403.
	exercise, err := a.exercises.FindOwned(ctx, exerciseID, user.UserID)
	if err != nil {
		return views.ExerciseStatsView{}, orNotFound(err, exerciseNotFound)
	}
	history, err := a.workouts.ExerciseHistoryWithPR(ctx, repo.ExerciseHistoryQuery{
		UserID:     user.UserID,
		ExerciseID: exerciseID,
		Limit:      historyLimit,
		Since:      models.RecentPRWindowStart(a.now()),
	})
	if err != nil {
		return views.ExerciseStatsView{}, err
	}
	pr, err := a.maxWeight(r, user.UserID, exerciseID)
	if err != nil {
		return views.ExerciseStatsView{}, err
	}
	metrics, err := a.workouts.SessionMetricsForExercise(ctx, user.UserID, exerciseID)
	if err != nil {
		return views.ExerciseStatsView{}, err
	}

	points := make([]models.ChartPoint, 0, len(metrics))
	for _, metric := range metrics {
		points = append(points, models.NewChartPoint(metric))
	}
	query := r.URL.Query()
	selectedMetric := chart.ParseMetric(query.Get("metric"))
	selectedRange := chart.ParseRange(query.Get("range"))
	return views.ExerciseStatsView{
		User:          user.Nav(),
		Exercise:      exercise,
		History:       history,
		PR:            pr,
		SessionCount:  len(points),
		Chart:         chart.Render(points, selectedMetric, selectedRange),
		ChartDataJSON: chart.EncodeData(points),
		Metric:        selectedMetric,
		Range:         selectedRange,
	}, nil
}

// maxWeight is nil when the user never logged the exercise.
func (a *App) maxWeight(r *http.Request, userID, exerciseID string) (*models.DynamicPR, error) {
	pr, err := a.workouts.MaxWeightForExercise(r.Context(), userID, exerciseID)
	if errors.Is(err, repo.ErrNotFound) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	return &pr, nil
}
