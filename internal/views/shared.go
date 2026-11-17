package views

import (
	"strings"
	"time"

	"github.com/henry40408/liftlog/internal/db"
)

// errorBox renders nothing when there is no error.
func errorBox(message string) node {
	if message == "" {
		return ""
	}
	return el("div", []attr{a("class", "error")}, text(message))
}

func formGroup(children ...node) node {
	return el("div", []attr{a("class", "form-group")}, children...)
}

// labelledInput is a form group of a label and one input; the " " between them
// is the whitespace maud writes for a line break in the source.
func labelledInput(label, id string, input node) node {
	return formGroup(
		el("label", []attr{a("for", id)}, text(label)),
		text(" "),
		input,
	)
}

func backLink(href, label string) node {
	return el("a", []attr{a("href", href), a("class", "back-link")}, node("&larr;"), text(" "+label))
}

// utcTime is the <time> element every page uses; the text is the no-JS
// fallback that app.js rewrites into the browser's timezone.
func utcTime(t time.Time) node {
	t = t.UTC()
	return el("time", []attr{a("datetime", rfc3339(t)), a("data-fmt", "datetime")},
		text(t.Format("2006-01-02 15:04 UTC")))
}

// rfc3339 matches chrono's to_rfc3339: a 0, 3, 6 or 9 digit fraction and a
// numeric +00:00 offset, which is db.FormatTime with a T for the space.
func rfc3339(t time.Time) string {
	return strings.Replace(db.FormatTime(t), " ", "T", 1)
}

func formatDate(t time.Time) string { return t.Format("2006-01-02") }
