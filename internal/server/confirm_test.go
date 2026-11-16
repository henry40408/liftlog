package server

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestWriteConfirmPageRendersTheInterstitial(t *testing.T) {
	recorder := httptest.NewRecorder()

	writeConfirmPage(recorder, ValidatedSession{Username: "lifter"}, confirmPage{
		ActionLabel: "Delete workout",
		Consequence: "All sets will be lost.",
		FormAction:  "/workouts/1/delete",
		CancelURL:   "/workouts/1",
	})

	if recorder.Code != http.StatusOK {
		t.Fatalf("status = %d", recorder.Code)
	}
	body := recorder.Body.String()
	for _, fragment := range []string{"Delete workout", "All sets will be lost.", `action="/workouts/1/delete"`, `href="/workouts/1"`} {
		if !strings.Contains(body, fragment) {
			t.Errorf("page lacks %q", fragment)
		}
	}
}
