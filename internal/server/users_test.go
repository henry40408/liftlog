package server

import (
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/henry40408/liftlog/internal/models"
)

func confirmForm(password string) url.Values {
	return url.Values{"current_password": {password}}
}

func TestEveryUserMayListUsers(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	member := ta.signInUser()
	wantStatus(t, ta.get("/users", admin.cookie), http.StatusOK)
	wantStatus(t, ta.get("/users", member.cookie), http.StatusOK)
	wantRedirect(t, ta.get("/users", ""), "/auth/login")
}

func TestUsersPageOffersPromoteOnlyForNonAdmins(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	member := ta.createUser("member", strongPassword, models.RoleUser)

	page := ta.get("/users", admin.cookie).text()

	if !strings.Contains(page, "/users/"+member.ID+"/promote") {
		t.Error("no promote link for a regular user")
	}
	if strings.Contains(page, "/users/"+admin.user.ID+"/promote") {
		t.Error("promote link for an admin")
	}
}

func TestNewUserIsAdminOnly(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	member := ta.signInUser()

	wantStatus(t, ta.get("/users/new", admin.cookie), http.StatusOK)
	forbidden := ta.get("/users/new", member.cookie)
	wantStatus(t, forbidden, http.StatusForbidden)
	if forbidden.text() != "Admin access required" {
		t.Errorf("403 body = %q, want exactly the message with no newline", forbidden.text())
	}
	wantRedirect(t, ta.get("/users/new", ""), "/auth/login")

	form := url.Values{"username": {"fresh"}, "password": {strongPassword}}
	wantStatus(t, ta.postForm("/users/new", form, member.cookie), http.StatusForbidden)
	wantRedirect(t, ta.postForm("/users/new", form, admin.cookie), "/users")
	created, err := ta.app.users.FindByUsername(t.Context(), "fresh")
	if err != nil || created.Role != models.RoleUser {
		t.Fatalf("created user = %+v, %v; want a regular user", created, err)
	}
}

func TestNonAdminsAreRefusedBeforeTheBodyIsParsed(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	member := ta.signInUser()
	target := ta.createUser("target", strongPassword, models.RoleUser)

	for _, action := range []string{"delete", "promote"} {
		request := newRequest(http.MethodPost, "/users/"+target.ID+"/"+action, "", member.cookie)
		request.Header.Del("Content-Type")
		wantStatus(t, ta.do(request), http.StatusForbidden)
	}
}

func TestAdminDeletesAUser(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	target := ta.signIn("target", models.RoleUser)

	wantRedirect(t, ta.postForm("/users/"+target.user.ID+"/delete", confirmForm(strongPassword), admin.cookie), "/users")

	if _, err := ta.app.users.FindByID(t.Context(), target.user.ID); err == nil {
		t.Fatal("the user survived")
	}
	if count := ta.count("SELECT COUNT(*) FROM sessions WHERE user_id = ?", target.user.ID); count != 0 {
		t.Fatalf("%d sessions survived", count)
	}
	event := bulkDestroyEvent(t, ta)
	if event["reason"] != "admin_user_delete" || event["count"] != float64(1) {
		t.Fatalf("event = %v", event)
	}
}

func TestAdminCannotDeleteThemselves(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()

	post := ta.postForm("/users/"+admin.user.ID+"/delete", confirmForm(strongPassword), admin.cookie)
	wantStatus(t, post, http.StatusBadRequest)
	wantContains(t, post, "Cannot delete your own account")
	wantStatus(t, ta.get("/users/"+admin.user.ID+"/delete", admin.cookie), http.StatusBadRequest)
	if _, err := ta.app.users.FindByID(t.Context(), admin.user.ID); err != nil {
		t.Fatal("the admin deleted themselves")
	}
}

func TestDeleteWithAWrongPasswordChangesNothing(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	target := ta.createUser("target", strongPassword, models.RoleUser)

	response := ta.postForm("/users/"+target.ID+"/delete", confirmForm("wrong-password"), admin.cookie)

	wantStatus(t, response, http.StatusOK)
	wantContains(t, response, "Password is incorrect")
	if _, err := ta.app.users.FindByID(t.Context(), target.ID); err != nil {
		t.Fatal("the user was deleted")
	}
	if len(ta.auditEventsNamed("auth.reauth.failed")) != 1 {
		t.Error("the failed re-authentication was not audited")
	}
}

func TestPromoteMakesAnAdminAndSignsThemOut(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	target := ta.signIn("target", models.RoleUser)

	wantRedirect(t, ta.postForm("/users/"+target.user.ID+"/promote", confirmForm(strongPassword), admin.cookie), "/users")

	promoted, err := ta.app.users.FindByID(t.Context(), target.user.ID)
	if err != nil || promoted.Role != models.RoleAdmin {
		t.Fatalf("user = %+v, %v; want an admin", promoted, err)
	}
	if count := ta.count("SELECT COUNT(*) FROM sessions WHERE user_id = ?", target.user.ID); count != 0 {
		t.Errorf("the promoted user kept %d sessions", count)
	}
	if count := ta.count("SELECT COUNT(*) FROM sessions WHERE user_id = ?", admin.user.ID); count != 1 {
		t.Errorf("the promoting admin has %d sessions, want 1", count)
	}
	if bulkDestroyEvent(t, ta)["reason"] != "role_change" {
		t.Error("the audit event lacks reason role_change")
	}
}

func TestPromoteWithAWrongPasswordChangesNothing(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	target := ta.createUser("target", strongPassword, models.RoleUser)

	response := ta.postForm("/users/"+target.ID+"/promote", confirmForm("wrong-password"), admin.cookie)

	wantStatus(t, response, http.StatusOK)
	wantContains(t, response, "Password is incorrect")
	if unchanged, _ := ta.app.users.FindByID(t.Context(), target.ID); unchanged.Role != models.RoleUser {
		t.Fatal("the role changed")
	}
}

func TestPromotingAMissingUserLogsNothing(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()

	wantRedirect(t, ta.postForm("/users/missing/promote", confirmForm(strongPassword), admin.cookie), "/users")
	if events := ta.auditEventsNamed("session.destroyed"); len(events) != 0 {
		t.Fatalf("a missing target logged %v", events)
	}
}

func TestConfirmPages(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	member := ta.signInUser()
	target := ta.createUser("target", strongPassword, models.RoleUser)

	deletePage := ta.get("/users/"+target.ID+"/delete", admin.cookie)
	wantStatus(t, deletePage, http.StatusOK)
	for _, fragment := range []string{"target", "cannot be undone", `name="current_password"`} {
		wantContains(t, deletePage, fragment)
	}
	promotePage := ta.get("/users/"+target.ID+"/promote", admin.cookie)
	wantStatus(t, promotePage, http.StatusOK)
	wantContains(t, promotePage, "target")
	wantContains(t, promotePage, "administrative access")

	wantStatus(t, ta.get("/users/"+target.ID+"/delete", member.cookie), http.StatusForbidden)
	wantStatus(t, ta.get("/users/"+target.ID+"/promote", member.cookie), http.StatusForbidden)
	wantStatus(t, ta.get("/users/missing/delete", admin.cookie), http.StatusNotFound)
	wantStatus(t, ta.get("/users/missing/promote", admin.cookie), http.StatusNotFound)
	// GET is inert.
	if _, err := ta.app.users.FindByID(t.Context(), target.ID); err != nil {
		t.Fatal("a GET changed the user")
	}
}

func TestReauthIsThrottledPerUser(t *testing.T) {
	ta := newTestApp(t, testAppOptions{sensitiveLimit: 2})
	admin := ta.signInAdmin()
	target := ta.createUser("target", strongPassword, models.RoleUser)
	attempt := func() response {
		return ta.postForm("/users/"+target.ID+"/promote", confirmForm("wrong-password"), admin.cookie)
	}

	wantStatus(t, attempt(), http.StatusOK)
	wantStatus(t, attempt(), http.StatusOK)
	throttled := attempt()
	wantStatus(t, throttled, http.StatusTooManyRequests)
	wantContains(t, throttled, "Too many attempts")
	if len(ta.auditEventsNamed("auth.reauth.throttled")) != 1 {
		t.Error("the throttle was not audited")
	}
}

func TestAVerifiedPasswordRefundsItsAttempt(t *testing.T) {
	ta := newTestApp(t, testAppOptions{sensitiveLimit: 1})
	admin := ta.signInAdmin()
	for _, name := range []string{"one", "two", "three"} {
		target := ta.createUser(name, strongPassword, models.RoleUser)
		wantRedirect(t, ta.postForm("/users/"+target.ID+"/delete", confirmForm(strongPassword), admin.cookie), "/users")
	}
}

func TestDeleteFailureLeavesSessionsAndUserIntact(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	target := ta.signIn("target", models.RoleUser)
	ta.mustExec(`CREATE TRIGGER block_user_delete BEFORE DELETE ON users
		BEGIN SELECT RAISE(ABORT, 'blocked'); END`)

	response := ta.postForm("/users/"+target.user.ID+"/delete", confirmForm(strongPassword), admin.cookie)

	wantStatus(t, response, http.StatusInternalServerError)
	if response.text() != "Database error" {
		t.Errorf("body = %q, want the generic message", response.text())
	}
	if count := ta.count("SELECT COUNT(*) FROM sessions WHERE user_id = ?", target.user.ID); count != 1 {
		t.Errorf("sessions = %d, want 1", count)
	}
	if count := ta.count("SELECT COUNT(*) FROM users WHERE id = ?", target.user.ID); count != 1 {
		t.Error("the user was deleted")
	}
}

func TestDeleteRemovesSessionsEvenWithoutTheCascade(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	target := ta.signIn("target", models.RoleUser)
	// The in-memory pool has one connection, so this reaches the handler's.
	ta.mustExec("PRAGMA foreign_keys = OFF")

	wantRedirect(t, ta.postForm("/users/"+target.user.ID+"/delete", confirmForm(strongPassword), admin.cookie), "/users")

	if count := ta.count("SELECT COUNT(*) FROM sessions WHERE user_id = ?", target.user.ID); count != 0 {
		t.Fatalf("%d orphan sessions survived", count)
	}
}

func TestDeletingAUserCascadesToTheirSessions(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	target := ta.signIn("target", models.RoleUser)

	ta.mustExec("DELETE FROM users WHERE id = ?", target.user.ID)

	if count := ta.count("SELECT COUNT(*) FROM sessions WHERE user_id = ?", target.user.ID); count != 0 {
		t.Fatalf("%d sessions survived the cascade", count)
	}
}

// bulkDestroyEvent is the one session.destroyed event that carries a count;
// single-session logouts carry a session_fp instead.
func bulkDestroyEvent(t *testing.T, ta *testApp) map[string]any {
	t.Helper()
	var bulk []map[string]any
	for _, event := range ta.auditEventsNamed("session.destroyed") {
		if _, counted := event["count"]; counted {
			bulk = append(bulk, event)
		}
	}
	if len(bulk) != 1 {
		t.Fatalf("want one bulk session.destroyed event, got %d in:\n%s", len(bulk), ta.logs.String())
	}
	return bulk[0]
}
