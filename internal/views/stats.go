package views

import (
	_ "embed"
	"strconv"
	"strings"
	"time"

	"github.com/henry40408/liftlog/internal/chart"
	"github.com/henry40408/liftlog/internal/models"
)

// StatsView is the data of the stats overview page.
type StatsView struct {
	User              NavUser
	WorkoutsThisWeek  int
	WorkoutsThisMonth int
	TotalVolume       float64
	TotalWorkouts     int
	PRs               []models.PersonalRecordSummary
}

func emptyDash() node {
	return el("span", []attr{a("class", "muted")}, node("&mdash;"))
}

func recentPRCell(pr models.PersonalRecordSummary) node {
	color := "var(--accent)"
	if pr.RecentIsAllTime() {
		color = "var(--gold)"
	}
	content := emptyDash()
	if pr.RecentValue != nil {
		content = text(chart.Display(*pr.RecentValue))
	}
	return el("td", []attr{a("data-label", "PR (1M)"), a("style", "color: "+color+"; font-weight: 600;")}, content)
}

func exerciseCell(pr models.PersonalRecordSummary) node {
	return el("td", []attr{a("data-label", "Exercise")},
		el("a", []attr{a("href", "/stats/exercise/"+pr.ExerciseID)}, text(pr.ExerciseName)))
}

func allTimePRCell(pr models.PersonalRecordSummary) node {
	return el("td", []attr{a("data-label", "PR (All)"), a("style", "color: var(--gold); font-weight: 600;")},
		text(chart.Display(pr.AllTimeValue)))
}

func dateCell(content node) node {
	return el("td", []attr{a("data-label", "Date")}, content)
}

func noPRs() node {
	return el("div", []attr{a("class", "empty-state")},
		el("p", []attr{a("class", "muted")}, text("No personal records yet. Start lifting!")))
}

func headerRow(labels ...string) node {
	cells := make([]node, len(labels))
	for i, label := range labels {
		cells[i] = el("th", nil, text(label))
	}
	return el("thead", nil, el("tr", nil, cells...))
}

func StatsIndex(view StatsView) string {
	body := group(
		nav(view.User),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text("Statistics"))),
			el("div", []attr{a("class", "stats-grid")},
				statCard(strconv.Itoa(view.TotalWorkouts), "Total Workouts"),
				statCard(strconv.Itoa(view.WorkoutsThisWeek), "This Week"),
				statCard(strconv.Itoa(view.WorkoutsThisMonth), "This Month"),
				statCard(chart.Display(view.TotalVolume), "Volume (Week)"),
			),
			el("h2", nil, text("Recent Personal Records")),
			recentPRs(view.PRs),
		),
	)
	return page("Stats - LiftLog", body)
}

func recentPRs(prs []models.PersonalRecordSummary) node {
	if len(prs) == 0 {
		return noPRs()
	}
	rows := make([]node, len(prs))
	for i, pr := range prs {
		rows[i] = el("tr", nil,
			exerciseCell(pr),
			allTimePRCell(pr),
			recentPRCell(pr),
			dateCell(utcDate(pr.AllTimeAchievedAt)),
		)
	}
	return group(
		el("table", []attr{a("class", "data-table")},
			headerRow("Exercise", "PR (All)", "PR (1M)", "Date"),
			el("tbody", nil, rows...),
		),
		el("p", []attr{a("class", "mt-4")}, el("a", []attr{a("href", "/stats/prs")}, text("View All PRs "), node("&rarr;"))),
	)
}

func PRs(user NavUser, prs []models.PersonalRecordSummary) string {
	body := group(
		nav(user),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text("Personal Records"))),
			allPRs(prs),
			backLink("/stats", "Back to Stats"),
		),
	)
	return page("Personal Records - LiftLog", body)
}

func allPRs(prs []models.PersonalRecordSummary) node {
	if len(prs) == 0 {
		return noPRs()
	}
	rows := make([]node, len(prs))
	for i, pr := range prs {
		rows[i] = el("tr", nil,
			exerciseCell(pr),
			allTimePRCell(pr),
			dateCell(utcDate(pr.AllTimeAchievedAt)),
			recentPRCell(pr),
			dateCell(recentPRDate(pr.RecentAchievedAt)),
		)
	}
	return el("table", []attr{a("class", "data-table")},
		headerRow("Exercise", "PR (All)", "Date", "PR (1M)", "Date"),
		el("tbody", nil, rows...),
	)
}

func recentPRDate(at *time.Time) node {
	if at == nil {
		return emptyDash()
	}
	return utcDate(*at)
}

// ExerciseStatsView is the data of the per-exercise stats page.
type ExerciseStatsView struct {
	User     NavUser
	Exercise models.Exercise
	History  []models.WorkoutLogWithExercise
	PR       *models.DynamicPR
	// SessionCount is the total session count for this exercise, for the
	// empty and sparse copy.
	SessionCount int
	// Chart is nil when fewer than two sessions are plotted.
	Chart *chart.Rendered
	// ChartDataJSON is chart.EncodeData of every point, for the client switcher.
	ChartDataJSON string
	// Metric and Range are the active tab, so the tab and the SVG agree.
	Metric chart.Metric
	Range  chart.Range
}

// chartJS redraws the chart in place; `{{METRIC}}` and `{{RANGE}}` are the
// server's selection. It must stay in sync with chart.Render.
//
//go:embed scripts/stats_exercise.js
var chartJS string

func ExerciseStats(view ExerciseStatsView) string {
	body := group(
		nav(view.User),
		el("main", nil,
			el("div", []attr{a("class", "page-header")},
				el("h1", nil, text(view.Exercise.Name)),
				el("div", []attr{a("class", "subtitle")}, text(view.Exercise.Category)),
			),
			el("h2", nil, text("Personal Record")),
			exercisePR(view.PR),
			el("h2", nil, text("Progress Trend")),
			el("div", []attr{a("class", "card"), a("style", "margin-bottom: var(--sp-6);")}, progressTrend(view)),
			el("h2", nil, text("Recent History")),
			historySection(view.History),
			backLink("/exercises", "Back to Exercises"),
		),
		chartScript(view),
	)
	return page(view.Exercise.Name+" Stats - LiftLog", body)
}

func chartScript(view ExerciseStatsView) node {
	if view.SessionCount < 2 {
		return ""
	}
	script := strings.NewReplacer("{{METRIC}}", string(view.Metric), "{{RANGE}}", string(view.Range)).Replace(chartJS)
	return el("script", nil, node(script))
}

func exercisePR(pr *models.DynamicPR) node {
	if pr == nil {
		return el("p", []attr{a("class", "muted")}, text("No PR for this exercise yet."))
	}
	return el("div", []attr{a("class", "card card-gold"), a("style", "margin-bottom: var(--sp-6); display: inline-block;")},
		el("div", []attr{a("style", "display: flex; align-items: baseline; gap: var(--sp-4);")},
			el("span", []attr{a("class", "stat-value"), a("style", "font-size: var(--font-4xl);")}, text(chart.Display(pr.Value))),
			text(" "),
			el("span", []attr{a("class", "text-secondary text-sm")},
				text("kg "), node("&middot;"), text(" "), utcDate(pr.AchievedAt)),
		),
	)
}

func historySection(history []models.WorkoutLogWithExercise) node {
	if len(history) == 0 {
		return el("p", []attr{a("class", "muted")}, text("No history for this exercise."))
	}
	rows := make([]node, len(history))
	for i, log := range history {
		rows[i] = el("tr", nil,
			el("td", nil, text(strconv.Itoa(log.SetNumber))),
			el("td", []attr{a("style", "color: var(--text-primary); font-weight: 500;")}, text(chart.Display(log.Weight))),
			el("td", nil, text(strconv.Itoa(log.Reps))),
			el("td", nil, rpeCell(log.RPE)),
			el("td", nil, prBadge(log)),
		)
	}
	return el("table", []attr{a("class", "data-table")},
		el("thead", nil, el("tr", nil,
			el("th", nil, text("Set")), el("th", nil, text("Weight")), el("th", nil, text("Reps")),
			el("th", nil, text("RPE")), el("th", nil))),
		el("tbody", nil, rows...),
	)
}

func tabClass(active bool) string {
	if active {
		return "btn btn-sm btn-tab is-active"
	}
	return "btn btn-sm btn-tab"
}

func metricTab(view ExerciseStatsView, metric chart.Metric, label string) node {
	return el("a", []attr{
		a("href", "?metric="+string(metric)+"&range="+string(view.Range)),
		a("class", tabClass(view.Metric == metric)),
		a("data-metric", string(metric)),
	}, text(label))
}

func rangeTab(view ExerciseStatsView, rng chart.Range, label string) node {
	return el("a", []attr{
		a("href", "?metric="+string(view.Metric)+"&range="+string(rng)),
		a("class", tabClass(view.Range == rng)),
		a("data-range", string(rng)),
	}, text(label))
}

func progressTrend(view ExerciseStatsView) node {
	if view.SessionCount == 0 {
		return el("p", []attr{a("class", "muted")}, text("No progress data yet — log this exercise to see your trend."))
	}
	return group(
		chartControls(view),
		el("div", []attr{a("id", "exercise-chart-wrap"), a("style", "position: relative;")},
			chartSVG(view),
			el("div", []attr{
				a("id", "exercise-chart-tooltip"),
				a("style", "position: absolute; display: none; pointer-events: none; background: var(--bg-elevated); border: 1px solid var(--border); padding: 8px 10px; font-size: 12px; border-radius: 6px; color: var(--text-primary); white-space: nowrap; z-index: 2;"),
			}),
		),
		singleSessionHint(view.SessionCount),
		chartLegend(),
		// ChartDataJSON has `</` escaped, so user-entered text cannot close
		// this script element.
		el("script", []attr{a("type", "application/json"), a("id", "exercise-chart-data")}, node(view.ChartDataJSON)),
	)
}

// chartControls are links so every series is reachable without scripts; the
// page script redraws in place instead.
func chartControls(view ExerciseStatsView) node {
	return el("div", []attr{
		a("id", "exercise-chart-controls"),
		a("style", "display: flex; gap: var(--sp-4); flex-wrap: wrap; margin-bottom: var(--sp-4);"),
	},
		el("div", []attr{a("role", "group"), a("aria-label", "Metric"), a("style", "display: flex; gap: var(--sp-2);")},
			metricTab(view, chart.TopSet, "Top Set"), text(" "),
			metricTab(view, chart.E1RM, "e1RM"), text(" "),
			metricTab(view, chart.Volume, "Volume"),
		),
		el("div", []attr{a("role", "group"), a("aria-label", "Range"), a("style", "display: flex; gap: var(--sp-2);")},
			rangeTab(view, chart.Last20, "Last 20"), text(" "),
			rangeTab(view, chart.All, "All"),
		),
	)
}

func singleSessionHint(sessionCount int) node {
	if sessionCount != 1 {
		return ""
	}
	return el("p", []attr{a("class", "muted"), a("style", "margin-top: var(--sp-4);")}, text("Need at least 2 sessions to draw a trend."))
}

func chartLegend() node {
	return el("div", []attr{a("style", "display: flex; gap: var(--sp-4); margin-top: var(--sp-3); font-size: 12px; color: var(--text-secondary);")},
		legendItem("var(--accent)", "Session"),
		text(" "),
		legendItem("var(--gold)", "Running PR"),
	)
}

func legendItem(color, label string) node {
	swatch := "display:inline-block;width:8px;height:8px;border-radius:50%;background:" + color + ";vertical-align:middle;margin-right:4px;"
	return el("span", nil, el("span", []attr{a("style", swatch)}), text(label))
}

func chartSVG(view ExerciseStatsView) node {
	return el("svg", []attr{
		a("id", "exercise-chart"), a("viewBox", "0 0 600 220"), a("width", "100%"),
		a("preserveAspectRatio", "xMidYMid meet"), a("role", "img"), a("aria-label", "Progress line chart"),
	},
		chartContent(view),
		// No-JS tooltips via SVG <title>; the client redraw replaces them.
		el("g", []attr{a("id", "chart-hit-areas")}, hitAreas(view.Chart)),
	)
}

func chartContent(view ExerciseStatsView) node {
	if view.Chart != nil {
		return chartBody(view.Chart)
	}
	if view.SessionCount != 1 {
		return ""
	}
	return el("circle", []attr{
		a("cx", "300"), a("cy", "110"), a("r", "5"), a("fill", "var(--accent)"),
		a("stroke", "var(--bg-root)"), a("stroke-width", "1.5"),
	})
}

func hitAreas(c *chart.Rendered) node {
	if c == nil {
		return ""
	}
	rects := make([]node, len(c.Points))
	for i, pt := range c.Points {
		rects[i] = el("rect", []attr{
			a("x", chart.Display(pt.HitX)), a("y", chart.Display(chart.PaddingTop)),
			a("width", chart.Display(pt.HitWidth)), a("height", chart.Display(c.PlotHeight)),
			a("fill", "transparent"), a("style", "cursor:crosshair"),
		}, el("title", nil, text(pt.Title)))
	}
	return group(rects...)
}

func chartBody(c *chart.Rendered) node {
	return group(
		el("g", []attr{a("id", "chart-y-grid")}, yGrid(c.YTicks)),
		el("g", []attr{a("id", "chart-x-labels")}, xLabels(c.XTicks)),
		el("polyline", []attr{
			a("id", "chart-line"), a("fill", "none"), a("stroke", "var(--accent)"), a("stroke-width", "2"),
			a("points", c.Polyline),
		}),
		el("g", []attr{a("id", "chart-dots")}, dots(c.Points)),
	)
}

func yGrid(ticks []chart.Tick) node {
	right := chart.Display(chart.Width - chart.PaddingRight)
	var children []node
	for _, tick := range ticks {
		y := chart.Display(tick.Pos)
		children = append(children,
			el("line", []attr{
				a("x1", chart.Display(chart.PaddingLeft)), a("x2", right), a("y1", y), a("y2", y),
				a("stroke", "var(--border-light)"), a("stroke-width", "1"),
			}),
			el("text", []attr{
				a("x", chart.Display(chart.PaddingLeft-6)), a("y", chart.Display(tick.Pos+4)),
				a("text-anchor", "end"), a("font-size", "11"), a("fill", "var(--text-secondary)"),
			}, text(tick.Label)),
		)
	}
	return group(children...)
}

func xLabels(ticks []chart.Tick) node {
	y := chart.Display(chart.Height - 8)
	children := make([]node, len(ticks))
	for i, tick := range ticks {
		children[i] = el("text", []attr{
			a("x", chart.Display(tick.Pos)), a("y", y),
			a("text-anchor", "middle"), a("font-size", "11"), a("fill", "var(--text-secondary)"),
		}, text(tick.Label))
	}
	return group(children...)
}

func dots(points []chart.Point) node {
	children := make([]node, len(points))
	for i, pt := range points {
		class, fill := "ll-dot", "var(--accent)"
		if pt.IsPR {
			class, fill = "ll-dot-pr", "var(--gold)"
		}
		children[i] = el("circle", []attr{
			a("cx", chart.Display(pt.X)), a("cy", chart.Display(pt.Y)), a("r", "4"),
			a("class", class), a("fill", fill), a("stroke", "var(--bg-root)"), a("stroke-width", "1.5"),
		})
	}
	return group(children...)
}
