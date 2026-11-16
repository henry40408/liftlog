package chart

import (
	"strconv"
	"strings"

	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/models"
)

// EncodeData serialises the points like the Rust server did, field order and
// float spelling included, for the client-side redraw. `</` is escaped so
// user-entered text cannot close the surrounding <script> element.
func EncodeData(points []models.ChartPoint) string {
	var b strings.Builder
	b.WriteString("[")
	for i, p := range points {
		if i > 0 {
			b.WriteString(",")
		}
		b.WriteString(`{"date":"` + db.FormatDate(p.Date) + `"`)
		b.WriteString(`,"top_weight":` + jsonFloat(p.TopWeight))
		b.WriteString(`,"top_reps":` + strconv.Itoa(p.TopReps))
		b.WriteString(`,"volume":` + jsonFloat(p.Volume))
		b.WriteString(`,"e1rm":` + jsonFloat(p.E1RM) + "}")
	}
	b.WriteString("]")
	return strings.ReplaceAll(b.String(), "</", `<\/`)
}
