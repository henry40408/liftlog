package models

import "time"

// ExerciseSessionMetric is one row per workout session that contains the
// queried exercise. Date is a calendar date (UTC midnight).
type ExerciseSessionMetric struct {
	Date      time.Time
	TopWeight float64
	TopReps   int
	Volume    float64
}

// ChartPoint is a chart-ready point, serialised into the page for the
// client-side switch handler.
type ChartPoint struct {
	Date      time.Time `json:"date"`
	TopWeight float64   `json:"top_weight"`
	TopReps   int       `json:"top_reps"`
	Volume    float64   `json:"volume"`
	E1RM      float64   `json:"e1rm"`
}

// NewChartPoint derives the Epley estimated one-rep max from the top set.
func NewChartPoint(m ExerciseSessionMetric) ChartPoint {
	return ChartPoint{
		Date:      m.Date,
		TopWeight: m.TopWeight,
		TopReps:   m.TopReps,
		Volume:    m.Volume,
		E1RM:      m.TopWeight * (1 + float64(m.TopReps)/30),
	}
}
