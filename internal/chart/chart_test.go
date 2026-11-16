package chart

import (
	"math"
	"strings"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/models"
)

func day(d int) time.Time { return time.Date(2026, 1, d, 0, 0, 0, 0, time.UTC) }

func pointsWithWeights(weights ...float64) []models.ChartPoint {
	points := make([]models.ChartPoint, len(weights))
	for i, w := range weights {
		points[i] = models.NewChartPoint(models.ExerciseSessionMetric{
			Date: day(i + 1), TopWeight: w, TopReps: 5, Volume: w * 5,
		})
	}
	return points
}

func TestParseMetricFallsBackToTopSet(t *testing.T) {
	cases := map[string]Metric{
		"top_set": TopSet, "e1rm": E1RM, "volume": Volume,
		"": TopSet, "bogus": TopSet, "E1RM": TopSet,
	}
	for in, want := range cases {
		if got := ParseMetric(in); got != want {
			t.Errorf("ParseMetric(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestParseRangeFallsBackToLast20(t *testing.T) {
	cases := map[string]Range{"all": All, "20": Last20, "": Last20, "bogus": Last20, "ALL": Last20}
	for in, want := range cases {
		if got := ParseRange(in); got != want {
			t.Errorf("ParseRange(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestMetricValueSelectsTheSeries(t *testing.T) {
	p := models.ChartPoint{TopWeight: 1, E1RM: 2, Volume: 3}
	if TopSet.Value(p) != 1 || E1RM.Value(p) != 2 || Volume.Value(p) != 3 {
		t.Errorf("wrong series selected: %v %v %v", TopSet.Value(p), E1RM.Value(p), Volume.Value(p))
	}
}

func TestJSONFloatMatchesSerde(t *testing.T) {
	cases := map[float64]string{
		0:                    "0.0",
		100:                  "100.0",
		102.5:                "102.5",
		112.75000000000001:   "112.75000000000001",
		-3:                   "-3.0",
		0.5:                  "0.5",
		0.0001:               "0.0001",
		0.00001:              "0.00001",
		0.000001:             "1e-6",
		1.5e-7:               "1.5e-7",
		1e15:                 "1000000000000000.0",
		1e16:                 "1e16",
		1.2345e20:            "1.2345e20",
		123456789012345680.0: "1.2345678901234568e17",
	}
	for in, want := range cases {
		if got := jsonFloat(in); got != want {
			t.Errorf("jsonFloat(%v) = %q, want %q", in, got, want)
		}
	}
}

func TestDisplayNeverUsesExponentForm(t *testing.T) {
	cases := map[float64]string{100: "100", 102.5: "102.5", 1e21: "1000000000000000000000", 0.25: "0.25"}
	for in, want := range cases {
		if got := Display(in); got != want {
			t.Errorf("Display(%v) = %q, want %q", in, got, want)
		}
	}
}

func TestEncodeDataKeepsFieldOrderAndFloatSpelling(t *testing.T) {
	points := []models.ChartPoint{{Date: day(2), TopWeight: 100, TopReps: 5, Volume: 500, E1RM: 116.66666666666667}}
	want := `[{"date":"2026-01-02","top_weight":100.0,"top_reps":5,"volume":500.0,"e1rm":116.66666666666667}]`
	if got := EncodeData(points); got != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestEncodeDataOfNoPointsIsAnEmptyArray(t *testing.T) {
	if got := EncodeData(nil); got != "[]" {
		t.Errorf("got %s", got)
	}
}

func TestRenderNeedsTwoPoints(t *testing.T) {
	if Render(nil, TopSet, All) != nil || Render(pointsWithWeights(100), TopSet, All) != nil {
		t.Error("fewer than two points must not render")
	}
}

func TestRenderLast20KeepsTheNewestSessions(t *testing.T) {
	weights := make([]float64, 25)
	for i := range weights {
		weights[i] = float64(100 + i)
	}
	points := pointsWithWeights(weights...)
	if got := len(Render(points, TopSet, Last20).Points); got != 20 {
		t.Errorf("last 20 plotted %d points", got)
	}
	rendered := Render(points, TopSet, All)
	if got := len(rendered.Points); got != 25 {
		t.Errorf("all plotted %d points", got)
	}
	last20 := Render(points, TopSet, Last20)
	if got := last20.XTicks[0].Label; got != "01-06" {
		t.Errorf("first plotted date = %s, want 01-06", got)
	}
}

func TestRenderMarksTheRunningBestAsPR(t *testing.T) {
	rendered := Render(pointsWithWeights(100, 90, 110, 110, 120), TopSet, All)
	var got []bool
	for _, p := range rendered.Points {
		got = append(got, p.IsPR)
	}
	want := []bool{true, false, true, false, true}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("IsPR = %v, want %v", got, want)
		}
	}
}

func TestRenderFlatSeriesStillSpansARange(t *testing.T) {
	rendered := Render(pointsWithWeights(100, 100), TopSet, All)
	if rendered.Points[0].Y != rendered.Points[1].Y {
		t.Errorf("flat series must stay flat: %v", rendered.Points)
	}
	if rendered.YTicks[0].Label != "101" || rendered.YTicks[3].Label != "99" {
		t.Errorf("flat range must pad by 1: %v", rendered.YTicks)
	}
}

func TestRenderHitBandsCoverThePlotWithoutGaps(t *testing.T) {
	rendered := Render(pointsWithWeights(1, 2, 3), TopSet, All)
	p := rendered.Points
	if p[0].HitX != PaddingLeft || p[0].HitWidth != p[1].HitWidth/2 || p[2].HitWidth != p[1].HitWidth/2 {
		t.Errorf("end bands must be half width: %+v", p)
	}
	if p[0].HitX+p[0].HitWidth != p[1].HitX || p[1].HitX+p[1].HitWidth != p[2].HitX {
		t.Errorf("bands leave a gap: %+v", p)
	}
}

func TestRenderTooltipAndPolyline(t *testing.T) {
	rendered := Render(pointsWithWeights(100, 110), TopSet, All)
	wantTitle := "2026-01-01\nTop: 100 kg × 5\ne1RM: 116.7 kg\nVolume: 500 kg"
	if got := rendered.Points[0].Title; got != wantTitle {
		t.Errorf("title = %q, want %q", got, wantTitle)
	}
	if !strings.HasPrefix(rendered.Polyline, "44.00,") || strings.Count(rendered.Polyline, " ") != 1 {
		t.Errorf("polyline = %q", rendered.Polyline)
	}
}

func TestRenderLabelsAtMostFiveDates(t *testing.T) {
	if got := len(Render(pointsWithWeights(1, 2), TopSet, All).XTicks); got != 2 {
		t.Errorf("two points labelled %d", got)
	}
	if got := len(Render(pointsWithWeights(1, 2, 3, 4, 5, 6, 7), TopSet, All).XTicks); got != 5 {
		t.Errorf("seven points labelled %d", got)
	}
}

func TestNonFiniteFloats(t *testing.T) {
	for _, tc := range []struct {
		value         float64
		display, json string
	}{
		{math.Inf(1), "inf", "null"},
		{math.Inf(-1), "-inf", "null"},
		{math.NaN(), "NaN", "null"},
	} {
		if got := Display(tc.value); got != tc.display {
			t.Errorf("Display(%v) = %q, want %q", tc.value, got, tc.display)
		}
		if got := jsonFloat(tc.value); got != tc.json {
			t.Errorf("jsonFloat(%v) = %q, want %q", tc.value, got, tc.json)
		}
	}
}
