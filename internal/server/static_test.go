package server

import (
	"bytes"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/henry40408/liftlog/internal/version"
)

func TestFaviconAndTouchIcon(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})

	svg := ta.get("/favicon.svg", "")
	wantStatus(t, svg, http.StatusOK)
	if got := svg.Header().Get("Content-Type"); got != "image/svg+xml" {
		t.Errorf("Content-Type = %q", got)
	}
	if got := svg.Header().Get("Cache-Control"); got != "public, max-age=86400" {
		t.Errorf("Cache-Control = %q", got)
	}
	if !strings.HasPrefix(svg.text(), "<svg") {
		t.Errorf("body does not start with <svg: %.40q", svg.text())
	}

	png := ta.get("/apple-touch-icon.png", "")
	wantStatus(t, png, http.StatusOK)
	if got := png.Header().Get("Content-Type"); got != "image/png" {
		t.Errorf("Content-Type = %q", got)
	}
	if !bytes.HasPrefix(png.Body.Bytes(), []byte("\x89PNG\r\n\x1a\n")) {
		t.Error("body is not a PNG")
	}
}

func TestStaticAssetsStayCacheableForASignedInUser(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()

	for path, wantType := range map[string]string{
		"/favicon.svg":          "image/svg+xml",
		"/assets/app.css":       "text/css; charset=utf-8",
		"/assets/app.js":        "text/javascript; charset=utf-8",
		"/apple-touch-icon.png": "image/png",
	} {
		response := ta.get(path, admin.cookie)
		wantStatus(t, response, http.StatusOK)
		if got := response.Header().Get("Content-Type"); got != wantType {
			t.Errorf("%s Content-Type = %q, want %q", path, got, wantType)
		}
		if got := response.Header().Get("Cache-Control"); got != "public, max-age=86400" {
			t.Errorf("%s Cache-Control = %q", path, got)
		}
	}
}

func TestAssetBodies(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	wantContains(t, ta.get("/assets/app.css", ""), ":root")
	wantContains(t, ta.get("/assets/app.js", ""), "LiftLog")
}

func TestHealthReportsTheVersion(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	response := ta.get("/health", "")

	wantStatus(t, response, http.StatusOK)
	if got := response.Header().Get("Content-Type"); got != "application/json" {
		t.Errorf("Content-Type = %q", got)
	}
	var got map[string]string
	if err := json.Unmarshal(response.Body.Bytes(), &got); err != nil {
		t.Fatalf("body is not JSON: %v", err)
	}
	if got["status"] != "ok" || got["git_version"] != version.Version() {
		t.Fatalf("body = %s", response.text())
	}
	if !strings.HasPrefix(response.text(), `{"status":"ok","git_version":`) {
		t.Fatalf("field order changed: %s", response.text())
	}
}

func TestErrorMapping(t *testing.T) {
	tests := []struct {
		name       string
		err        error
		wantStatus int
		wantBody   string
	}{
		{"not found shows its message", notFound("User not found"), http.StatusNotFound, "User not found"},
		{"bad request shows its message", badRequest("Cannot delete your own account"), http.StatusBadRequest, "Cannot delete your own account"},
		{"internal hides the cause", internalError(errors.New("secret detail")), http.StatusInternalServerError, "Internal error"},
		{"repository errors are database errors", errors.New("secret detail"), http.StatusInternalServerError, "Database error"},
	}
	ta := newTestApp(t, testAppOptions{})
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			recorder := httptest.NewRecorder()
			ta.app.fail(recorder, newRequest(http.MethodGet, "/x", "", ""), test.err)

			if recorder.Code != test.wantStatus || recorder.Body.String() != test.wantBody {
				t.Fatalf("got %d %q, want %d %q", recorder.Code, recorder.Body.String(), test.wantStatus, test.wantBody)
			}
		})
	}
	if !strings.Contains(ta.logs.String(), "secret detail") {
		t.Error("the 500 cause was not logged")
	}
}

func TestParseFiniteFloatRejectsInfinityAndNaN(t *testing.T) {
	for _, accepted := range []string{"0", "62.5", "-3", "1e3"} {
		if _, err := parseFiniteFloat(accepted); err != nil {
			t.Errorf("parseFiniteFloat(%q) failed: %v", accepted, err)
		}
	}
	for _, rejected := range []string{"inf", "-Inf", "NaN", "1e999", "", "abc"} {
		if _, err := parseFiniteFloat(rejected); err == nil {
			t.Errorf("parseFiniteFloat(%q) succeeded", rejected)
		}
	}
}
