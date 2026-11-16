package models

import "time"

// RecentPRWindow is the "PR (1M)" window: rolling 30 days, so it never resets
// on the 1st.
const RecentPRWindow = 30 * 24 * time.Hour

func RecentPRWindowStart(now time.Time) time.Time {
	return now.Add(-RecentPRWindow)
}

type DynamicPR struct {
	ExerciseID   string
	ExerciseName string
	Value        float64
	AchievedAt   time.Time
}

// PersonalRecordSummary holds the all-time and recent-window PR; the recent
// fields are nil when the exercise has no logs in the window.
type PersonalRecordSummary struct {
	ExerciseID        string
	ExerciseName      string
	AllTimeValue      float64
	AllTimeAchievedAt time.Time
	RecentValue       *float64
	RecentAchievedAt  *time.Time
}

// RecentIsAllTime reports that the all-time PR was set inside the recent
// window.
func (p PersonalRecordSummary) RecentIsAllTime() bool {
	return p.RecentValue != nil && *p.RecentValue >= p.AllTimeValue
}

type LastExerciseWeight struct {
	ExerciseID string
	Weight     float64
	RPE        *int
	LoggedAt   time.Time
}
