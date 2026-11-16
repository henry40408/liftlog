package chart

import (
	"fmt"
	"math"
	"strconv"
	"strings"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
)

const (
	Width        = 600.0
	Height       = 220.0
	PaddingLeft  = 44.0
	PaddingRight = 12.0
	PaddingTop   = 14.0
	paddingBot   = 28.0

	maxLabels = 5
	lastN     = 20
)

// Tick is an axis label at a pixel position.
type Tick struct {
	Pos   float64
	Label string
}

// Point is one plotted session. HitX and HitWidth are its hover band; Title is
// the no-JS tooltip and mirrors what the client builds.
type Point struct {
	X, Y, HitX, HitWidth float64
	IsPR                 bool
	Title                string
}

// Rendered is the precomputed geometry of the server-rendered SVG.
type Rendered struct {
	PlotHeight float64
	Points     []Point
	// Polyline is the `points` attribute, e.g. "10.00,20.00 50.00,60.00".
	Polyline string
	YTicks   []Tick
	XTicks   []Tick
}

// Render lays out the requested series. It returns nil when fewer than two
// sessions would be plotted.
func Render(points []models.ChartPoint, metric Metric, rng Range) *Rendered {
	slice := points
	if rng == Last20 && len(slice) > lastN {
		slice = slice[len(slice)-lastN:]
	}
	if len(slice) < 2 {
		return nil
	}

	values := make([]float64, len(slice))
	for i, p := range slice {
		values[i] = metric.Value(p)
	}
	yMin, yMax := paddedYRange(values)
	plotW := Width - PaddingLeft - PaddingRight
	plotH := Height - PaddingTop - paddingBot

	rendered := layoutPoints(slice, values, layout{yMin: yMin, yMax: yMax, plotW: plotW, plotH: plotH})
	parts := make([]string, len(rendered))
	for i, p := range rendered {
		parts[i] = fmt.Sprintf("%.2f,%.2f", p.X, p.Y)
	}
	return &Rendered{
		PlotHeight: plotH,
		Points:     rendered,
		Polyline:   strings.Join(parts, " "),
		YTicks:     yAxisTicks(yMin, yMax, plotH),
		XTicks:     xAxisTicks(slice, rendered),
	}
}

type layout struct{ yMin, yMax, plotW, plotH float64 }

// layoutPoints spaces points evenly by session, not calendar time, so rest
// weeks do not stretch the axis. A point is a PR when it beats every earlier
// plotted value, as on the client.
func layoutPoints(slice []models.ChartPoint, values []float64, l layout) []Point {
	n := len(slice)
	lastIndex := float64(n - 1)
	// Hover bands: full width per interior point, half at the ends.
	bandW := l.plotW / math.Max(lastIndex, 1)
	runningMax := math.Inf(-1)
	out := make([]Point, n)
	for i, p := range slice {
		// The float64() conversions stop the compiler fusing a+b*c into an FMA,
		// which would round differently from the Rust server on arm64.
		x := PaddingLeft + float64((float64(i)/lastIndex)*l.plotW)
		y := PaddingTop + float64((1-(values[i]-l.yMin)/(l.yMax-l.yMin))*l.plotH)
		isPR := values[i] > runningMax
		if isPR {
			runningMax = values[i]
		}
		hitX, hitWidth := x-bandW/2, bandW
		if i == 0 {
			hitX = PaddingLeft
		}
		if i == 0 || i == n-1 {
			hitWidth = bandW / 2
		}
		out[i] = Point{X: x, Y: y, IsPR: isPR, HitX: hitX, HitWidth: hitWidth, Title: tooltip(p)}
	}
	return out
}

func tooltip(p models.ChartPoint) string {
	return db.FormatDate(p.Date) +
		"\nTop: " + Display(p.TopWeight) + " kg × " + strconv.Itoa(p.TopReps) +
		fmt.Sprintf("\ne1RM: %.1f kg\nVolume: %.0f kg", p.E1RM, p.Volume)
}

// paddedYRange keeps the line off the edges.
func paddedYRange(values []float64) (float64, float64) {
	lo, hi := math.Inf(1), math.Inf(-1)
	for _, v := range values {
		lo, hi = math.Min(lo, v), math.Max(hi, v)
	}
	if math.Abs(hi-lo) < 1e-9 {
		return lo - 1, hi + 1
	}
	// The conversion stops arm64 from fusing the multiply into the add below,
	// which would round differently from Rust.
	pad := float64((hi - lo) * 0.1)
	return lo - pad, hi + pad
}

func yAxisTicks(yMin, yMax, plotH float64) []Tick {
	ticks := make([]Tick, 4)
	for i := range ticks {
		frac := float64(i) / 3
		ticks[i] = Tick{
			Pos:   PaddingTop + float64(frac*plotH),
			Label: fmt.Sprintf("%.0f", yMax-float64(frac*(yMax-yMin))),
		}
	}
	return ticks
}

// xAxisTicks labels at most maxLabels points; the caller guarantees at least
// two.
func xAxisTicks(slice []models.ChartPoint, rendered []Point) []Tick {
	n := len(slice)
	count := min(n, maxLabels)
	ticks := make([]Tick, count)
	for i := range ticks {
		idx := i * (n - 1) / (count - 1)
		ticks[i] = Tick{Pos: rendered[idx].X, Label: slice[idx].Date.Format("01-02")}
	}
	return ticks
}
