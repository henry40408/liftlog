package server

import (
	"net/http"
	"net/netip"
	"net/url"
	"strings"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/auth"
	"github.com/henry40408/liftlog/internal/config"
	"github.com/henry40408/liftlog/internal/models"
)

func loginForm(username, password string) url.Values {
	return url.Values{"username": {username}, "password": {password}}
}

func setupForm(username, password, token string) url.Values {
	return url.Values{"username": {username}, "password": {password}, "setup_token": {token}}
}

func wantStatus(t *testing.T, got response, want int) {
	t.Helper()
	if got.status() != want {
		t.Fatalf("status = %d, want %d; body: %s", got.status(), want, got.text())
	}
}

func wantRedirect(t *testing.T, got response, location string) {
	t.Helper()
	wantStatus(t, got, http.StatusSeeOther)
	if got.location() != location {
		t.Fatalf("Location = %q, want %q", got.location(), location)
	}
}

func wantContains(t *testing.T, got response, fragment string) {
	t.Helper()
	if !strings.Contains(got.text(), fragment) {
		t.Fatalf("body lacks %q:\n%s", fragment, got.text())
	}
}

func TestLoginPageRedirectsToSetupWhenThereAreNoUsers(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	wantRedirect(t, ta.get("/auth/login", ""), "/auth/setup")
}

func TestLoginPageRendersWhenUsersExist(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	ta.createUser("lifter", strongPassword, models.RoleUser)
	response := ta.get("/auth/login", "")
	wantStatus(t, response, http.StatusOK)
	wantContains(t, response, "Login")
	if cacheControl := response.Header().Get("Cache-Control"); cacheControl != "" {
		t.Fatalf("anonymous login page has Cache-Control %q", cacheControl)
	}
}

func TestLoginPageRedirectsASignedInUserHome(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	wantRedirect(t, ta.get("/auth/login", admin.cookie), "/")
}

func TestValidLoginRedirectsAndSetsTheCookie(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	ta.createUser("lifter", strongPassword, models.RoleUser)

	response := ta.postForm("/auth/login", loginForm("lifter", strongPassword), "")

	wantRedirect(t, response, "/")
	cookie := response.sessionCookie()
	for _, attribute := range []string{"session=", "Path=/", "HttpOnly", "SameSite=Lax", "Max-Age=604800"} {
		if !strings.Contains(cookie, attribute) {
			t.Errorf("cookie %q lacks %q", cookie, attribute)
		}
	}
	if strings.Contains(cookie, "Secure") {
		t.Errorf("plain-HTTP cookie is Secure: %q", cookie)
	}
	if got := response.Header().Get("Cache-Control"); got != "no-cache, no-store, must-revalidate" {
		t.Errorf("Cache-Control = %q", got)
	}
	if got := response.Header().Get("Pragma"); got != "no-cache" {
		t.Errorf("Pragma = %q", got)
	}
}

func TestLoginUsesTheHostPrefixWhenSecure(t *testing.T) {
	ta := newTestApp(t, testAppOptions{cookieSecure: true})
	ta.createUser("lifter", strongPassword, models.RoleUser)

	response := ta.postForm("/auth/login", loginForm("lifter", strongPassword), "")

	cookie := response.sessionCookie()
	if !strings.HasPrefix(cookie, "__Host-session=") || !strings.Contains(cookie, "Secure") {
		t.Fatalf("cookie = %q, want a Secure __Host-session", cookie)
	}
}

func TestLoginFailuresShowOneMessage(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	ta.createUser("lifter", strongPassword, models.RoleUser)

	for name, form := range map[string]url.Values{
		"wrong password": loginForm("lifter", "not-the-password"),
		"unknown user":   loginForm("ghost", strongPassword),
	} {
		t.Run(name, func(t *testing.T) {
			response := ta.postForm("/auth/login", form, "")
			wantStatus(t, response, http.StatusOK)
			wantContains(t, response, "Invalid username or password")
			if response.sessionCookie() != "" {
				t.Fatal("a failed login set a session cookie")
			}
		})
	}
}

func TestLoginDoesNotTrimTheUsername(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	ta.createUser(" spaced ", strongPassword, models.RoleUser)
	wantContains(t, ta.postForm("/auth/login", loginForm("spaced", strongPassword), ""), "Invalid username or password")
	wantRedirect(t, ta.postForm("/auth/login", loginForm(" spaced ", strongPassword), ""), "/")
}

func TestLoginWithoutAFormBodyIsRejectedLikeAxum(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	request := newRequest(http.MethodPost, "/auth/login", "", "")
	request.Header.Del("Content-Type")
	wantStatus(t, ta.do(request), http.StatusUnsupportedMediaType)

	response := ta.postForm("/auth/login", url.Values{"username": {"a"}}, "")
	wantStatus(t, response, http.StatusUnprocessableEntity)
	wantContains(t, response, "missing field `password`")
}

func TestLogoutClearsEverything(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()

	response := ta.postForm("/auth/logout", url.Values{}, admin.cookie)

	wantRedirect(t, response, "/auth/login")
	if cookie := response.sessionCookie(); !strings.Contains(cookie, "Max-Age=0") {
		t.Errorf("removal cookie = %q, want Max-Age=0", cookie)
	}
	if got := response.Header().Get("Clear-Site-Data"); got != `"cache", "cookies", "storage"` {
		t.Errorf("Clear-Site-Data = %q", got)
	}
	if count := ta.count("SELECT COUNT(*) FROM sessions WHERE token = ?", admin.token); count != 0 {
		t.Errorf("session row survived logout")
	}
	if ta.auditEventsNamed("session.destroyed")[0]["reason"] != "logout" {
		t.Errorf("session.destroyed event lacks reason logout")
	}
}

func TestLogoutDoesNotReissueTheCookieWhenASlideWasDue(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	ta.ageSessionTouch(admin.token, 2)

	response := ta.postForm("/auth/logout", url.Values{}, admin.cookie)

	if cookies := response.setCookies(); len(cookies) != 1 || !strings.Contains(cookies[0], "Max-Age=0") {
		t.Fatalf("Set-Cookie = %q, want exactly the removal cookie", cookies)
	}
}

func TestLogoutRemovalCookieIsSecureWhenEnabled(t *testing.T) {
	ta := newTestApp(t, testAppOptions{cookieSecure: true})
	admin := ta.signInAdmin()

	cookie := ta.postForm("/auth/logout", url.Values{}, admin.cookie).sessionCookie()

	if !strings.HasPrefix(cookie, "__Host-session=") || !strings.Contains(cookie, "Secure") || !strings.Contains(cookie, "Max-Age=0") {
		t.Fatalf("removal cookie = %q", cookie)
	}
}

func TestLoginWhileASlideIsDueSetsOnlyTheNewSessionCookie(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	ta.ageSessionTouch(admin.token, 2)

	response := ta.postForm("/auth/login", loginForm("admin", strongPassword), admin.cookie)

	wantRedirect(t, response, "/")
	var sessionCookies []string
	for _, line := range response.setCookies() {
		if strings.HasPrefix(line, "session=") {
			sessionCookies = append(sessionCookies, line)
		}
	}
	if len(sessionCookies) != 1 || strings.HasPrefix(sessionCookies[0], "session="+admin.token+";") {
		t.Fatalf("session Set-Cookie lines = %q, want exactly one, for the new token", sessionCookies)
	}
}

// Sign Out must be a real submit button so it works without JavaScript.
func TestSignOutButtonWorksWithoutJavaScript(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	member := ta.signInUser()

	response := ta.get("/", member.cookie)

	wantStatus(t, response, http.StatusOK)
	wantContains(t, response, `<form action="/auth/logout" method="post"`)
	wantContains(t, response, `<button type="submit" class="sign-out-btn">`)
}

func TestLogoutRequiresASession(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	wantRedirect(t, ta.postForm("/auth/logout", url.Values{}, ""), "/auth/login")
}

func TestSetupPageRendersOnlyWithoutUsers(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	wantContains(t, ta.get("/auth/setup", ""), `name="setup_token"`)
	ta.createUser("lifter", strongPassword, models.RoleUser)
	wantRedirect(t, ta.get("/auth/setup", ""), "/auth/login")
}

func TestSetupCreatesAnAdminAndSignsIn(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})

	response := ta.postForm("/auth/setup", setupForm("  owner  ", strongPassword, testSetupToken), "")

	wantRedirect(t, response, "/")
	if response.sessionCookie() == "" {
		t.Fatal("setup set no session cookie")
	}
	if got := response.Header().Get("Cache-Control"); got != "no-cache, no-store, must-revalidate" {
		t.Errorf("Cache-Control = %q", got)
	}
	user, err := ta.app.users.FindByUsername(t.Context(), "owner")
	if err != nil || !user.Role.IsAdmin() {
		t.Fatalf("setup user = %+v, %v; want an admin named owner", user, err)
	}
	if ta.auditEventsNamed("session.created")[0]["reason"] != "setup" {
		t.Error("session.created event lacks reason setup")
	}
	wantStatus(t, ta.get("/", response.cookieHeader()), http.StatusOK)
}

func TestSetupRedirectsToLoginWhenUsersExist(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	ta.createUser("lifter", strongPassword, models.RoleUser)
	wantRedirect(t, ta.postForm("/auth/setup", setupForm("owner", strongPassword, testSetupToken), ""), "/auth/login")
}

func TestSetupValidation(t *testing.T) {
	tests := []struct {
		name     string
		username string
		password string
		want     string
	}{
		{"empty username", "", strongPassword, "Username is required"},
		{"short password", "owner", "short", "Password must be at least 12 characters"},
		{"one under the minimum", "owner", "gymrat.2026", "Password must be at least 12 characters"},
		{"at the minimum", "owner", "gymrat.2026!", ""},
		{"too long", "owner", strings.Repeat("x", 129), "at most 128 characters"},
		{"weak but long enough", "owner", "MyPassword12", "too"},
		{"derived from the username", "henrylifts.42x", "henrylifts.42x", ""},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			ta := newTestApp(t, testAppOptions{})
			response := ta.postForm("/auth/setup", setupForm(test.username, test.password, testSetupToken), "")
			switch test.name {
			case "at the minimum":
				wantRedirect(t, response, "/")
			case "derived from the username", "weak but long enough":
				wantStatus(t, response, http.StatusOK)
				if response.location() != "" || response.sessionCookie() != "" {
					t.Fatal("a weak password created a user")
				}
				if strings.Contains(response.text(), "at least 12 characters") {
					t.Fatalf("rejected on length, want strength:\n%s", response.text())
				}
			default:
				wantStatus(t, response, http.StatusOK)
				wantContains(t, response, test.want)
			}
		})
	}
}

func TestFreshSessionSetsNoCookie(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	response := ta.get("/", admin.cookie)
	wantStatus(t, response, http.StatusOK)
	if cookies := response.setCookies(); len(cookies) != 0 {
		t.Fatalf("a fresh session set cookies: %q", cookies)
	}
}

func TestBadSessionsRedirectToLogin(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	idle := ta.signInAdmin()
	ta.expireSession(idle.token)
	old := ta.signIn("old", models.RoleUser)
	ta.ageSessionCreation(old.token, 91)

	for name, cookie := range map[string]string{
		"idle-expired":      idle.cookie,
		"over the absolute": old.cookie,
		"unknown token":     "session=nonsense",
	} {
		t.Run(name, func(t *testing.T) {
			wantRedirect(t, ta.get("/", cookie), "/auth/login")
		})
	}
	if ta.onlyAuditEvent("session.rejected")["reason"] != "unknown_token" {
		t.Error("unknown token was not audited as rejected")
	}
	if len(ta.auditEventsNamed("session.expired")) != 2 {
		t.Errorf("want 2 session.expired events, got %d", len(ta.auditEventsNamed("session.expired")))
	}
}

func TestSecureAppAcceptsOnlyTheHostPrefixedCookie(t *testing.T) {
	ta := newTestApp(t, testAppOptions{cookieSecure: true})
	admin := ta.signInAdmin()

	wantStatus(t, ta.get("/", admin.cookie), http.StatusOK)
	wantRedirect(t, ta.get("/", "session="+admin.token), "/auth/login")
}

func TestSlidingReissuesTheCookie(t *testing.T) {
	for _, secure := range []bool{false, true} {
		ta := newTestApp(t, testAppOptions{cookieSecure: secure})
		admin := ta.signInAdmin()
		ta.ageSessionTouch(admin.token, 2)

		cookie := ta.get("/", admin.cookie).sessionCookie()

		name := "session="
		if secure {
			name = "__Host-session="
		}
		if !strings.HasPrefix(cookie, name+admin.token) || !strings.Contains(cookie, "Max-Age=604800") {
			t.Errorf("secure=%v: cookie = %q", secure, cookie)
		}
	}
}

func TestSecureFlowLoginSlideLogout(t *testing.T) {
	ta := newTestApp(t, testAppOptions{cookieSecure: true})
	ta.createUser("lifter", strongPassword, models.RoleUser)

	login := ta.postForm("/auth/login", loginForm("lifter", strongPassword), "")
	cookie := login.cookieHeader()
	if !strings.HasPrefix(cookie, "__Host-session=") {
		t.Fatalf("login cookie = %q", cookie)
	}
	token := strings.TrimPrefix(cookie, "__Host-session=")
	ta.ageSessionTouch(token, 2)
	if slid := ta.get("/", cookie).sessionCookie(); !strings.HasPrefix(slid, "__Host-session=") {
		t.Fatalf("slid cookie = %q", slid)
	}
	logout := ta.postForm("/auth/logout", url.Values{}, cookie)
	if removal := logout.sessionCookie(); !strings.HasPrefix(removal, "__Host-session=") || !strings.Contains(removal, "Max-Age=0") {
		t.Fatalf("removal cookie = %q", removal)
	}
}

func TestAuthenticatedPagesAreNotCacheable(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	response := ta.get("/", admin.cookie)
	if got := response.Header().Get("Cache-Control"); got != "no-cache, no-store, must-revalidate" {
		t.Errorf("Cache-Control = %q", got)
	}
	if got := response.Header().Get("Pragma"); got != "no-cache" {
		t.Errorf("Pragma = %q", got)
	}
}

func TestLoginThrottleAfterTooManyFailures(t *testing.T) {
	ta := newTestApp(t, testAppOptions{loginLimit: 3})
	ta.createUser("lifter", strongPassword, models.RoleUser)
	for range 3 {
		wantStatus(t, ta.postForm("/auth/login", loginForm("lifter", "wrong-password"), ""), http.StatusOK)
	}
	response := ta.postForm("/auth/login", loginForm("lifter", strongPassword), "")
	wantStatus(t, response, http.StatusTooManyRequests)
	wantContains(t, response, "Too many login attempts")
	if len(ta.auditEventsNamed("auth.login.throttled")) != 1 {
		t.Error("throttle was not audited")
	}
}

func TestSuccessfulLoginsReleaseTheirAttempt(t *testing.T) {
	ta := newTestApp(t, testAppOptions{loginLimit: 2})
	ta.createUser("lifter", strongPassword, models.RoleUser)
	for range 5 {
		wantRedirect(t, ta.postForm("/auth/login", loginForm("lifter", strongPassword), ""), "/")
	}
}

func TestLoginBudgetsArePerPeer(t *testing.T) {
	ta := newTestApp(t, testAppOptions{loginLimit: 1})
	ta.createUser("lifter", strongPassword, models.RoleUser)
	send := func(peer string) response {
		request := newRequest(http.MethodPost, "/auth/login", loginForm("lifter", "wrong-password").Encode(), "")
		return ta.do(withPeer(request, peer))
	}
	wantStatus(t, send("10.0.0.1:1000"), http.StatusOK)
	wantStatus(t, send("10.0.0.1:1001"), http.StatusTooManyRequests)
	wantStatus(t, send("10.0.0.2:1000"), http.StatusOK)
}

func TestClientIPResolution(t *testing.T) {
	loopback := []netip.Addr{netip.MustParseAddr("127.0.0.1")}
	tests := []struct {
		name    string
		options testAppOptions
		peer    string
		headers map[string][]string
		wantIP  string
	}{
		{"untrusted peer forging XFF", testAppOptions{proxyHeader: config.TrustXForwardedFor, trustedProxies: loopback},
			"203.0.113.9:1", map[string][]string{"X-Forwarded-For": {"198.51.100.7"}}, "203.0.113.9"},
		{"loopback peer uses the rightmost hop", testAppOptions{proxyHeader: config.TrustXForwardedFor, trustedProxies: loopback},
			"127.0.0.1:1", map[string][]string{"X-Forwarded-For": {"198.51.100.7, 198.51.100.8"}}, "198.51.100.8"},
		{"duplicate lines use the last line", testAppOptions{proxyHeader: config.TrustXForwardedFor, trustedProxies: loopback},
			"127.0.0.1:1", map[string][]string{"X-Forwarded-For": {"198.51.100.1", "198.51.100.2"}}, "198.51.100.2"},
		{"header ignored when not configured", testAppOptions{},
			"127.0.0.1:1", map[string][]string{"X-Forwarded-For": {"198.51.100.7"}}, "127.0.0.1"},
		{"X-Real-IP ignored when XFF is configured", testAppOptions{proxyHeader: config.TrustXForwardedFor, trustedProxies: loopback},
			"127.0.0.1:1", map[string][]string{"X-Real-Ip": {"198.51.100.7"}}, "127.0.0.1"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			ta := newTestApp(t, test.options)
			ta.createUser("lifter", strongPassword, models.RoleUser)
			request := withPeer(newRequest(http.MethodPost, "/auth/login", loginForm("lifter", "wrong-password").Encode(), ""), test.peer)
			for name, values := range test.headers {
				request.Header[name] = values
			}
			ta.do(request)
			if got := ta.onlyAuditEvent("auth.login.failed")["client_ip"]; got != test.wantIP {
				t.Fatalf("client_ip = %v, want %s", got, test.wantIP)
			}
		})
	}
}

func TestLoginBackoffDelaysTheNextAttempt(t *testing.T) {
	schedule := auth.BackoffSchedule{FreeAttempts: 1, Base: 80 * time.Millisecond, Max: 320 * time.Millisecond, Window: time.Minute}
	ta := newTestApp(t, testAppOptions{loginBackoff: &schedule})
	ta.createUser("lifter", strongPassword, models.RoleUser)

	ta.postForm("/auth/login", loginForm("lifter", "wrong-password"), "")
	start := time.Now()
	request := withPeer(newRequest(http.MethodPost, "/auth/login", loginForm("lifter", "wrong-password").Encode(), ""), "10.0.0.9:1")
	ta.do(request)

	if elapsed := time.Since(start); elapsed < schedule.Base {
		t.Fatalf("second failure took %v, want at least %v", elapsed, schedule.Base)
	}
}

func TestPublicRoutesCarryTheBaselineHeaders(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	responses := map[string]response{
		"normal":        ta.get("/", admin.cookie),
		"auth redirect": ta.get("/users", ""),
		"unknown route": ta.get("/shared/no-such-token", ""),
		"csrf 403": ta.do(withHeader(newRequest(http.MethodPost, "/auth/login", "", ""),
			"Sec-Fetch-Site", "cross-site")),
	}
	for name, response := range responses {
		t.Run(name, func(t *testing.T) {
			want := map[string]string{
				"Content-Security-Policy": "frame-ancestors 'none'",
				"X-Frame-Options":         "DENY",
				"X-Content-Type-Options":  "nosniff",
				"Referrer-Policy":         "strict-origin-when-cross-origin",
			}
			for header, value := range want {
				if got := response.Header().Get(header); got != value {
					t.Errorf("%s = %q, want %q", header, got, value)
				}
			}
			if got := response.Header().Get("Strict-Transport-Security"); got != "" {
				t.Errorf("HSTS present by default: %q", got)
			}
		})
	}
}

func TestHSTSWhenConfigured(t *testing.T) {
	ta := newTestApp(t, testAppOptions{hstsMaxAge: 31536000, hstsSubdomains: true})
	want := "max-age=31536000; includeSubDomains"

	if got := ta.get("/health", "").Header().Get("Strict-Transport-Security"); got != want {
		t.Errorf("HSTS = %q, want %q", got, want)
	}
	forbidden := ta.do(withHeader(newRequest(http.MethodPost, "/auth/login", "", ""), "Sec-Fetch-Site", "cross-site"))
	wantStatus(t, forbidden, http.StatusForbidden)
	if got := forbidden.Header().Get("Strict-Transport-Security"); got != want {
		t.Errorf("HSTS on the CSRF 403 = %q, want %q", got, want)
	}

	withoutSubdomains := newTestApp(t, testAppOptions{hstsMaxAge: 60})
	if got := withoutSubdomains.get("/health", "").Header().Get("Strict-Transport-Security"); got != "max-age=60" {
		t.Errorf("HSTS = %q, want max-age=60", got)
	}
}
