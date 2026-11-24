package views

import (
	"fmt"
	"regexp"
	"slices"
	"testing"

	"github.com/henry40408/liftlog/internal/chart"
)

var xLabelPattern = regexp.MustCompile(`<text [^>]*text-anchor="([a-z]+)"[^>]*>([^<]*)</text>`)

// anchorsByLabel reads "<label>=<anchor>" off every <text> the markup holds.
func anchorsByLabel(markup node) []string {
	var found []string
	for _, match := range xLabelPattern.FindAllStringSubmatch(string(markup), -1) {
		found = append(found, match[2]+"="+match[1])
	}
	return found
}

func TestXLabelsEndOnlyTheLastLabelAtTheRightEdge(t *testing.T) {
	// Two labels are the fewest a chart draws, five the most.
	for _, count := range []int{2, 3, 5} {
		ticks := make([]chart.Tick, count)
		want := make([]string, count)
		for i := range ticks {
			label := fmt.Sprintf("L%d", i)
			ticks[i] = chart.Tick{Pos: float64(44 + i*100), Label: label}
			want[i] = label + "=middle"
		}
		want[count-1] = fmt.Sprintf("L%d=end", count-1)

		if got := anchorsByLabel(xLabels(ticks)); !slices.Equal(got, want) {
			t.Errorf("%d labels: got %v, want %v", count, got, want)
		}
	}
}
