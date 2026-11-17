package server

import (
	"net/http"

	"github.com/henry40408/liftlog/internal/views"
)

// confirmPage is the server-side confirmation interstitial for destructive
// actions, so they still confirm with JavaScript off. The GET that renders it
// must be inert and apply the same ownership check as the POST it leads to.
//
// formAction is the URL the page was served from: GET confirms, POST acts.
// consequence should say what will be lost. User promote and delete render
// their own page, which also asks for the admin's password.
type confirmPage struct {
	ActionLabel string
	Consequence string
	FormAction  string
	CancelURL   string
}

func writeConfirmPage(w http.ResponseWriter, user ValidatedSession, page confirmPage) {
	writeHTML(w, http.StatusOK, views.Confirm(views.ConfirmView{
		User:        user.Nav(),
		ActionLabel: page.ActionLabel,
		Consequence: page.Consequence,
		FormAction:  page.FormAction,
		CancelURL:   page.CancelURL,
	}))
}
