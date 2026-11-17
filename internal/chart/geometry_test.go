package chart

import (
	"math"
	"strings"
	"testing"
)

func approx(got, want float64) bool { return math.Abs(got-want) < 0.005 }

func TestRenderGeometryMatchesThePlotArea(t *testing.T) {
	rendered := Render(pointsWithWeights(100, 110), TopSet, All)
	// The plot area is 544 x 178; the values 100..110 are padded to 99..111.
	if rendered.PlotHeight != 178 {
		t.Errorf("PlotHeight = %v, want 178", rendered.PlotHeight)
	}
	if rendered.Polyline != "44.00,177.17 588.00,28.83" {
		t.Errorf("polyline = %q", rendered.Polyline)
	}
	wantTicks := []Tick{{14, "111"}, {73.33, "107"}, {132.67, "103"}, {192, "99"}}
	if len(rendered.YTicks) != len(wantTicks) {
		t.Fatalf("got %d y ticks, want %d", len(rendered.YTicks), len(wantTicks))
	}
	for i, want := range wantTicks {
		got := rendered.YTicks[i]
		if !approx(got.Pos, want.Pos) || got.Label != want.Label {
			t.Errorf("YTicks[%d] = %+v, want %+v", i, got, want)
		}
	}
	p := rendered.Points
	if len(p) != 2 {
		t.Fatalf("got %d points, want 2", len(p))
	}
	if !approx(p[0].HitWidth, 272) || !approx(p[1].HitX, 316) || !approx(p[1].HitWidth, 272) {
		t.Errorf("hit bands = %+v", p)
	}
}

func TestRenderXTicksPickEvenlySpreadSessions(t *testing.T) {
	rendered := Render(pointsWithWeights(1, 2, 3, 4, 5, 6, 7), TopSet, All)
	var labels []string
	for _, tick := range rendered.XTicks {
		labels = append(labels, tick.Label)
	}
	want := "01-01 01-02 01-04 01-05 01-07"
	if got := strings.Join(labels, " "); got != want {
		t.Errorf("labels = %q, want %q", got, want)
	}
}
