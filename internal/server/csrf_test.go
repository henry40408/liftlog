package server

import (
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/henry40408/liftlog/internal/models"
)

// csrfProbe posts to the login route: it needs no session, and the handler's
// "Invalid username or password" page (200) proves the guard let it through.
func csrfProbe(ta *testApp, headers map[string]string, host string) response {
	request := newRequest(http.MethodPost, "/auth/login", loginForm("lifter", "wrong-password").Encode(), "")
	if host != "" {
		request.Host = host
	}
	for name, value := range headers {
		request.Header.Set(name, value)
	}
	return ta.do(request)
}

func TestCSRFRejectsCrossSiteRequests(t *testing.T) {
	tests := []struct {
		name    string
		headers map[string]string
		host    string
	}{
		{"cross-site", map[string]string{"Sec-Fetch-Site": "cross-site"}, ""},
		{"same-site is not same-origin", map[string]string{"Sec-Fetch-Site": "same-site"}, ""},
		{"mismatched origin", map[string]string{"Origin": "https://evil.example"}, "app.example.com"},
		{"port mismatch", map[string]string{"Origin": "http://app.example.com:9999"}, "app.example.com:8080"},
		{"host without port", map[string]string{"Origin": "http://app.example.com:8080"}, "app.example.com"},
		{"other subdomain", map[string]string{"Origin": "https://evil.app.example.com"}, "app.example.com"},
		{"null origin", map[string]string{"Origin": "null"}, "app.example.com"},
		{"unparseable origin", map[string]string{"Origin": "not-an-origin"}, "app.example.com"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			ta := newTestApp(t, testAppOptions{})
			response := csrfProbe(ta, test.headers, test.host)
			wantStatus(t, response, http.StatusForbidden)
			if response.text() != "" {
				t.Fatalf("the 403 has a body: %q", response.text())
			}
		})
	}
}

func TestCSRFAllowsSameOriginAndHeaderlessClients(t *testing.T) {
	tests := []struct {
		name    string
		headers map[string]string
		host    string
	}{
		{"same-origin", map[string]string{"Sec-Fetch-Site": "same-origin"}, ""},
		{"none (typed address)", map[string]string{"Sec-Fetch-Site": "none"}, ""},
		{"no fetch metadata and no origin", nil, ""},
		{"matching origin on a plain-HTTP address", map[string]string{"Origin": "http://192.168.1.5:8080"}, "192.168.1.5:8080"},
		{"matching origin on a default port", map[string]string{"Origin": "https://app.example.com"}, "app.example.com"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			ta := newTestApp(t, testAppOptions{})
			response := csrfProbe(ta, test.headers, test.host)
			wantStatus(t, response, http.StatusOK)
			wantContains(t, response, "Invalid username or password")
			if got := len(ta.auditEventsNamed("auth.login.failed")); got != 1 {
				t.Fatalf("login.failed events = %d, want 1: the handler did not run\nlogs:\n%s", got, ta.logs.String())
			}
		})
	}
}

func TestCSRFFetchMetadataOverridesOrigin(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	response := csrfProbe(ta, map[string]string{
		"Sec-Fetch-Site": "cross-site",
		"Origin":         "http://app.example.com",
	}, "app.example.com")
	wantStatus(t, response, http.StatusForbidden)
}

func TestCSRFLetsSafeMethodsThrough(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	// OPTIONS has no route, so the mux answering 405 shows the guard let it by.
	tests := []struct {
		method     string
		wantStatus int
		wantBody   string
	}{
		{http.MethodGet, http.StatusOK, `"status":"ok"`},
		{http.MethodHead, http.StatusOK, ""},
		{http.MethodOptions, http.StatusMethodNotAllowed, ""},
	}
	for _, test := range tests {
		t.Run(test.method, func(t *testing.T) {
			request := withHeader(newRequest(test.method, "/health", "", ""), "Sec-Fetch-Site", "cross-site")
			response := ta.do(request)
			wantStatus(t, response, test.wantStatus)
			if test.wantBody != "" {
				wantContains(t, response, test.wantBody)
			}
		})
	}
}

// Parity with the Rust test the_skipped_methods_are_exactly_the_ones_the_layer_never_rejects:
// only GET, HEAD and OPTIONS skip the guard.
func TestCSRFRejectsEveryOtherMethodCrossOrigin(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	methods := []string{http.MethodTrace, http.MethodPost, http.MethodPut, http.MethodPatch, http.MethodDelete}
	for _, method := range methods {
		t.Run(method, func(t *testing.T) {
			request := withHeader(newRequest(method, "/health", "", ""), "Sec-Fetch-Site", "cross-site")
			response := ta.do(request)
			wantStatus(t, response, http.StatusForbidden)
			if response.text() != "" {
				t.Fatalf("the 403 has a body: %q", response.text())
			}
		})
	}
}

func TestCSRFProtectsAuthenticatedWrites(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	target := ta.createUser("target", strongPassword, models.RoleUser)

	request := withHeader(newRequest(http.MethodPost, "/users/"+target.ID+"/delete",
		url.Values{"current_password": {strongPassword}}.Encode(), admin.cookie), "Sec-Fetch-Site", "cross-site")

	wantStatus(t, ta.do(request), http.StatusForbidden)
	if _, err := ta.app.users.FindByID(t.Context(), target.ID); err != nil {
		t.Fatal("the cross-site delete went through")
	}
}

func TestCSRFRejectionIsAudited(t *testing.T) {
	tests := []struct {
		name       string
		headers    map[string]string
		wantReason string
	}{
		{"fetch metadata", map[string]string{"Sec-Fetch-Site": "cross-site", "Origin": "https://evil.example"}, "sec_fetch_site"},
		{"origin fallback", map[string]string{"Origin": "https://evil.example"}, "origin_fallback"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			ta := newTestApp(t, testAppOptions{})
			csrfProbe(ta, test.headers, "app.example.com")

			event := ta.onlyAuditEvent("csrf.rejected")
			if event["reason"] != test.wantReason || event["method"] != "POST" ||
				event["origin"] != "https://evil.example" || event["path"] != "/auth/login" {
				t.Fatalf("event = %v", event)
			}
		})
	}
}

func TestCSRFAuditTruncatesTheOrigin(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	csrfProbe(ta, map[string]string{"Origin": "https://" + strings.Repeat("a", 400) + ".example"}, "app.example.com")

	origin, _ := ta.onlyAuditEvent("csrf.rejected")["origin"].(string)
	if len(origin) != 256 {
		t.Fatalf("logged origin has %d characters, want 256", len(origin))
	}
}
