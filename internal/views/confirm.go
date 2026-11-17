package views

// ConfirmView is the server-side confirmation interstitial: GET shows it, only
// the POST acts.
type ConfirmView struct {
	User        NavUser
	ActionLabel string
	Consequence string
	FormAction  string
	CancelURL   string
}

func Confirm(view ConfirmView) string {
	return page(view.ActionLabel+" - LiftLog", group(
		nav(view.User),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text(view.ActionLabel))),
			el("p", []attr{a("class", "mb-4")}, text(view.Consequence)),
			el("div", []attr{a("class", "actions")},
				el("form", []attr{a("method", "post"), a("action", view.FormAction)},
					el("button", []attr{a("type", "submit"), a("class", "btn btn-danger")}, text(view.ActionLabel)),
				),
				el("a", []attr{a("href", view.CancelURL), a("class", "btn btn-ghost")}, text("Cancel")),
			),
		),
	))
}
