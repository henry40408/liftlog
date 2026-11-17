package server

import (
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/henry40408/liftlog/internal/models"
	"github.com/henry40408/liftlog/internal/version"
)

const (
	settingsOldPassword = "password123"
	settingsNewPassword = "purple-monkey-dishwasher"
)

func settingsPasswordForm(current, next, confirm string) url.Values {
	return url.Values{"current_password": {current}, "new_password": {next}, "confirm_password": {confirm}}
}

func settingsTimezoneForm(name string) url.Values {
	return url.Values{"timezone": {name}}
}

// signInWithPassword is a member whose password is settingsOldPassword.
func (ta *testApp) signInWithPassword(username string) signedIn {
	ta.t.Helper()
	user := ta.createUser(username, settingsOldPassword, models.RoleUser)
	token := ta.createSession(user.ID)
	return signedIn{user: user, token: token, cookie: ta.cookieFor(token)}
}

func (ta *testApp) sessionIsAlive(token string) bool {
	ta.t.Helper()
	return ta.count("SELECT COUNT(*) FROM sessions WHERE token = ?", token) == 1
}

func (ta *testApp) passwordWorks(username, password string) bool {
	ta.t.Helper()
	_, ok, err := ta.app.verifyPassword(newRequest(http.MethodGet, "/", "", ""), username, password)
	if err != nil {
		ta.t.Fatal(err)
	}
	return ok
}

func TestSettingsRoutesRequireSignIn(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	wantRedirect(t, ta.get("/settings", ""), "/auth/login")
	wantRedirect(t, ta.get("/settings/logout-others", ""), "/auth/login")
	wantRedirect(t, ta.postForm("/settings/password", settingsPasswordForm("x", "newpass", "newpass"), ""), "/auth/login")
	wantRedirect(t, ta.postForm("/settings/timezone", settingsTimezoneForm("Asia/Taipei"), ""), "/auth/login")
	wantRedirect(t, ta.postForm("/settings/logout-others", url.Values{}, ""), "/auth/login")
}

func TestSettingsPageRendersWithTheBuildVersionAndNoStore(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInWithPassword("testuser")

	page := ta.get("/settings", user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, "Settings")
	wantContains(t, page, "<code>"+version.Version()+"</code>")
	if got := page.Header().Get("Cache-Control"); got != "no-cache, no-store, must-revalidate" {
		t.Fatalf("Cache-Control = %q", got)
	}
	if got := page.Header().Get("Pragma"); got != "no-cache" {
		t.Fatalf("Pragma = %q", got)
	}
}

func TestSettingsPageListsSessionsAndMarksThisDevice(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	ta.createSession(user.user.ID)

	page := ta.get("/settings", user.cookie)

	for _, fragment := range []string{"Active Sessions", "This device", "Other device"} {
		wantContains(t, page, fragment)
	}
	if strings.Contains(page.text(), user.token) {
		t.Fatal("a raw session token reached the page")
	}
}

func TestSettingsPageReachesLogoutOthersThroughItsConfirmationPage(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()

	page := ta.get("/settings", user.cookie)

	if strings.Contains(page.text(), `onsubmit="return confirm(`) {
		t.Fatal("the settings page depends on confirm() to guard an action")
	}
	wantContains(t, page, `<a href="/settings/logout-others"`)
}

func TestChangePasswordRotatesTheSessionAndEndsTheOthers(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInWithPassword("testuser")
	other := ta.createSession(user.user.ID)

	page := ta.postForm("/settings/password", settingsPasswordForm(settingsOldPassword, settingsNewPassword, settingsNewPassword), user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, "Password changed successfully. All other sessions have been logged out.")
	if !ta.passwordWorks("testuser", settingsNewPassword) || ta.passwordWorks("testuser", settingsOldPassword) {
		t.Fatal("the password was not replaced")
	}
	newToken := strings.TrimPrefix(page.cookieHeader(), "session=")
	if newToken == "" || newToken == user.token {
		t.Fatalf("the token was not rotated: %q", newToken)
	}
	if ta.sessionIsAlive(user.token) || ta.sessionIsAlive(other) {
		t.Fatal("a pre-change session survived")
	}
	if !ta.sessionIsAlive(newToken) {
		t.Fatal("the replacement session is not usable")
	}
	if got := ta.count("SELECT COUNT(*) FROM sessions WHERE user_id = ?", user.user.ID); got != 1 {
		t.Fatalf("%d sessions survive, want 1", got)
	}
}

// Without suppressing the middleware's refresh, a second Set-Cookie for the
// dead old token would reach the browser and sign the user out.
func TestChangePasswordSetsExactlyOneCookieAndItWorks(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInWithPassword("testuser")

	page := ta.postForm("/settings/password", settingsPasswordForm(settingsOldPassword, settingsNewPassword, settingsNewPassword), user.cookie)

	if cookies := page.setCookies(); len(cookies) != 1 {
		t.Fatalf("want exactly one Set-Cookie, got %v", cookies)
	}
	next := ta.get("/settings", page.cookieHeader())
	wantStatus(t, next, http.StatusOK)
	wantContains(t, next, "This device")
}

func TestChangePasswordMarksTheNewSessionAsThisDevice(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInWithPassword("testuser")

	page := ta.postForm("/settings/password", settingsPasswordForm(settingsOldPassword, settingsNewPassword, settingsNewPassword), user.cookie)

	wantContains(t, page, "This device")
	if strings.Contains(page.text(), "Other device") {
		t.Fatal("the rotated session shows as another device")
	}
}

func TestChangePasswordAuditsTheRotation(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInWithPassword("testuser")
	ta.createSession(user.user.ID)

	page := ta.postForm("/settings/password", settingsPasswordForm(settingsOldPassword, settingsNewPassword, settingsNewPassword), user.cookie)

	bulk := bulkDestroyEvent(t, ta)
	// The count includes the rotated-away session.
	if bulk["reason"] != "password_change" || bulk["count"] != float64(2) || bulk["user_id"] != user.user.ID {
		t.Fatalf("bulk event = %v", bulk)
	}
	created := ta.onlyAuditEvent("session.created")
	if created["reason"] != "password_change_rotation" || created["username"] != "testuser" {
		t.Fatalf("created event = %v", created)
	}
	if strings.Contains(ta.logs.String(), strings.TrimPrefix(page.cookieHeader(), "session=")) {
		t.Fatal("the raw new token reached the log")
	}
}

func TestChangePasswordRefusals(t *testing.T) {
	cases := []struct {
		name     string
		form     url.Values
		wantText string
	}{
		{"mismatch", settingsPasswordForm(settingsOldPassword, settingsNewPassword, "amber-tractor-lantern"), "New passwords do not match"},
		{"too short", settingsPasswordForm(settingsOldPassword, "short", "short"), "at least 12 characters"},
		{"too long", settingsPasswordForm(settingsOldPassword, strings.Repeat("a", 129), strings.Repeat("a", 129)), "at most 128 characters"},
		{"guessable", settingsPasswordForm(settingsOldPassword, "MyPassword12", "MyPassword12"), "similar to a commonly used password"},
		{"wrong current", settingsPasswordForm("wrongpass", settingsNewPassword, settingsNewPassword), "Current password is incorrect"},
		{"reused", settingsPasswordForm(settingsNewPassword, settingsNewPassword, settingsNewPassword), "New password must be different from the current password"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			ta := newTestApp(t, testAppOptions{})
			user := ta.signInWithPassword("testuser")
			other := ta.createSession(user.user.ID)
			if tc.name == "reused" {
				ta.mustExec("UPDATE users SET password_hash = ? WHERE id = ?", mustHash(t, ta, settingsNewPassword), user.user.ID)
			}

			page := ta.postForm("/settings/password", tc.form, user.cookie)

			wantStatus(t, page, http.StatusOK)
			wantContains(t, page, tc.wantText)
			if page.sessionCookie() != "" {
				t.Fatalf("a refused change rotated the session: %v", page.setCookies())
			}
			if !ta.sessionIsAlive(user.token) || !ta.sessionIsAlive(other) {
				t.Fatal("a refused change ended a session")
			}
			if len(ta.auditEventsNamed("session.created")) != 0 {
				t.Fatal("a refused change audited a new session")
			}
		})
	}
}

func mustHash(t *testing.T, ta *testApp, password string) string {
	t.Helper()
	hash, err := ta.app.hasher.Hash(password)
	if err != nil {
		t.Fatal(err)
	}
	return hash
}

func TestChangePasswordRejectsAPasswordDerivedFromTheUsername(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInWithPassword("henrylifts")

	page := ta.postForm("/settings/password", settingsPasswordForm(settingsOldPassword, "henrylifts.42x", "henrylifts.42x"), user.cookie)

	wantStatus(t, page, http.StatusOK)
	if ta.passwordWorks("henrylifts", "henrylifts.42x") {
		t.Fatal("a password built from the username was accepted")
	}
}

func TestChangePasswordChecksThePolicyBeforeTheCurrentPassword(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInWithPassword("testuser")

	ta.postForm("/settings/password", settingsPasswordForm("wrongpass", "short", "short"), user.cookie)

	if len(ta.auditEventsNamed("auth.reauth.failed")) != 0 {
		t.Fatal("a rejected new password still cost a password check")
	}
}

func TestFailedPasswordChangeAuditsWithoutLeakingSecrets(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInWithPassword("testuser")

	ta.postForm("/settings/password", settingsPasswordForm("wrongpass", settingsNewPassword, settingsNewPassword), user.cookie)

	event := ta.onlyAuditEvent("auth.reauth.failed")
	if event["user_id"] != user.user.ID || event["action"] != "password_change" {
		t.Fatalf("event = %v", event)
	}
	logs := ta.logs.String()
	if strings.Contains(logs, user.token) || strings.Contains(logs, "wrongpass") {
		t.Fatalf("a secret reached the log:\n%s", logs)
	}
}

func TestChangePasswordIsThrottledAfterTheBudgetIsSpent(t *testing.T) {
	ta := newTestApp(t, testAppOptions{sensitiveLimit: 3})
	user := ta.signInWithPassword("testuser")
	form := settingsPasswordForm("wrongpass", settingsNewPassword, settingsNewPassword)

	for attempt := 1; attempt <= 3; attempt++ {
		wantStatus(t, ta.postForm("/settings/password", form, user.cookie), http.StatusOK)
	}
	throttled := ta.postForm("/settings/password", form, user.cookie)

	wantStatus(t, throttled, http.StatusTooManyRequests)
	wantContains(t, throttled, "Too many password change attempts")
	event := ta.onlyAuditEvent("auth.reauth.throttled")
	if event["action"] != "password_change" {
		t.Fatalf("event = %v", event)
	}
}

func TestASuccessfulPasswordChangeRefundsItsAttempt(t *testing.T) {
	ta := newTestApp(t, testAppOptions{sensitiveLimit: 1})
	user := ta.signInWithPassword("testuser")
	cookie := user.cookie

	passwords := []string{settingsOldPassword, "amber-tractor-lantern", "velvet-harbour-kestrel", "copper-thistle-marmot"}
	for i := 1; i < len(passwords); i++ {
		page := ta.postForm("/settings/password", settingsPasswordForm(passwords[i-1], passwords[i], passwords[i]), cookie)
		wantStatus(t, page, http.StatusOK)
		wantContains(t, page, "Password changed successfully")
		cookie = page.cookieHeader()
	}
	if !ta.passwordWorks("testuser", "copper-thistle-marmot") {
		t.Fatal("the last change was not applied")
	}
}

func TestPasswordChangeThrottleIsPerUser(t *testing.T) {
	ta := newTestApp(t, testAppOptions{sensitiveLimit: 1})
	alice := ta.signInWithPassword("alice")
	bob := ta.signInWithPassword("bob")
	form := settingsPasswordForm("wrongpass", settingsNewPassword, settingsNewPassword)

	wantStatus(t, ta.postForm("/settings/password", form, alice.cookie), http.StatusOK)
	wantStatus(t, ta.postForm("/settings/password", form, alice.cookie), http.StatusTooManyRequests)
	wantStatus(t, ta.postForm("/settings/password", form, bob.cookie), http.StatusOK)
}

func TestChangePasswordReportsMissingFormFields(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInWithPassword("testuser")

	page := ta.postForm("/settings/password", url.Values{"current_password": {"x"}}, user.cookie)

	wantStatus(t, page, http.StatusUnprocessableEntity)
	wantContains(t, page, "missing field `new_password`")
}

func TestChangeTimezoneSavesTheTrimmedName(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()

	page := ta.postForm("/settings/timezone", settingsTimezoneForm("  Asia/Taipei "), user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, "Timezone updated.")
	wantContains(t, page, `value="Asia/Taipei"`)
	stored, err := ta.app.users.FindTimezone(t.Context(), user.user.ID)
	if err != nil || stored == nil || *stored != "Asia/Taipei" {
		t.Fatalf("stored = %v, %v", stored, err)
	}
}

func TestChangeTimezoneRejectsNamesThatAreNotExactlyIANA(t *testing.T) {
	for _, name := range []string{"Not/AZone", "asia/taipei", "", "Local", "../etc/passwd"} {
		t.Run(name, func(t *testing.T) {
			ta := newTestApp(t, testAppOptions{})
			user := ta.signInUser()

			page := ta.postForm("/settings/timezone", settingsTimezoneForm(name), user.cookie)

			wantStatus(t, page, http.StatusBadRequest)
			wantContains(t, page, "Unknown timezone. Pick a name such as Asia/Taipei.")
			if stored, _ := ta.app.users.FindTimezone(t.Context(), user.user.ID); stored != nil {
				t.Fatalf("stored %q", *stored)
			}
		})
	}
}

func TestChangeTimezoneRequiresTheField(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()

	page := ta.postForm("/settings/timezone", url.Values{}, user.cookie)

	wantStatus(t, page, http.StatusUnprocessableEntity)
	wantContains(t, page, "missing field `timezone`")
}

func TestLogoutOthersEndsSiblingsOnly(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	sibling := ta.createSession(user.user.ID)
	ta.createSession(user.user.ID)
	stranger := ta.signIn("stranger", models.RoleUser)

	page := ta.postForm("/settings/logout-others", url.Values{}, user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, "Logged out of all other devices.")
	if ta.sessionIsAlive(sibling) || !ta.sessionIsAlive(user.token) || !ta.sessionIsAlive(stranger.token) {
		t.Fatal("logout-others ended the wrong sessions")
	}
	bulk := bulkDestroyEvent(t, ta)
	if bulk["reason"] != "logout_others" || bulk["count"] != float64(2) || bulk["user_id"] != user.user.ID {
		t.Fatalf("bulk event = %v", bulk)
	}
}

func TestLogoutOthersConfirmationPhrasesTheSessionCount(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	const alone = "No other device is signed in, so nothing will be logged out."

	wantContains(t, ta.get("/settings/logout-others", user.cookie), alone)

	ta.createSession(user.user.ID)
	wantContains(t, ta.get("/settings/logout-others", user.cookie), "1 other signed-in device will be logged out. This device stays signed in.")

	ta.createSession(user.user.ID)
	wantContains(t, ta.get("/settings/logout-others", user.cookie), "2 other signed-in devices will be logged out. This device stays signed in.")
}

func TestLogoutOthersConfirmationIsInert(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	sibling := ta.createSession(user.user.ID)

	page := ta.get("/settings/logout-others", user.cookie)

	wantStatus(t, page, http.StatusOK)
	wantContains(t, page, `<form method="post" action="/settings/logout-others">`)
	wantContains(t, page, `href="/settings"`)
	if !ta.sessionIsAlive(sibling) {
		t.Fatal("viewing the confirmation page ended a session")
	}
	if len(ta.auditEventsNamed("session.destroyed")) != 0 {
		t.Fatal("viewing the confirmation page was audited as a destroy")
	}
}

func TestSettingsPostsRejectNonFormBodies(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	user := ta.signInUser()
	request := newRequest(http.MethodPost, "/settings/timezone", `{"timezone":"Asia/Taipei"}`, user.cookie)
	request.Header.Set("Content-Type", "application/json")

	wantStatus(t, ta.do(request), http.StatusUnsupportedMediaType)
}
