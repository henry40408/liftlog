// Package chart renders the exercise progress chart as SVG and encodes the
// data the client-side redraw reads. The script in
// internal/views/scripts/stats_exercise.js redraws the same chart, so both
// must change together.
package chart

import "github.com/henry40408/liftlog/internal/models"

// Metric is the series the chart plots.
type Metric string

const (
	TopSet Metric = "top_set"
	E1RM   Metric = "e1rm"
	Volume Metric = "volume"
)

// ParseMetric reads the `metric` query value. It comes from a user-editable
// query string, so an unknown value falls back to the default.
func ParseMetric(raw string) Metric {
	switch Metric(raw) {
	case E1RM:
		return E1RM
	case Volume:
		return Volume
	}
	return TopSet
}

// Value must agree with `metricValue` in stats_exercise.js.
func (m Metric) Value(p models.ChartPoint) float64 {
	switch m {
	case E1RM:
		return p.E1RM
	case Volume:
		return p.Volume
	}
	return p.TopWeight
}

// Range is how many sessions the chart covers. The default last 20 keeps early,
// light sessions from compressing the recent scale.
type Range string

const (
	Last20 Range = "20"
	All    Range = "all"
)

// ParseRange reads the `range` query value; unknown values fall back to the
// default.
func ParseRange(raw string) Range {
	if Range(raw) == All {
		return All
	}
	return Last20
}
