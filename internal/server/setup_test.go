package server

import (
	"net/http"
	"net/url"
	"strings"
	"sync"
	"testing"

	"github.com/henry40408/liftlog/internal/models"
)

func TestSetupTokenIsRequired(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	for name, token := range map[string]string{"missing": "", "wrong": "AAAAA-BBBBB"} {
		t.Run(name, func(t *testing.T) {
			response := ta.postForm("/auth/setup", setupForm("owner", strongPassword, token), "")
			wantStatus(t, response, http.StatusForbidden)
			wantContains(t, response, "Invalid setup token")
		})
	}
	if count, _ := ta.app.users.Count(t.Context()); count != 0 {
		t.Fatal("a bad token created a user")
	}
	if len(ta.auditEventsNamed("auth.setup.token_rejected")) != 2 {
		t.Errorf("want 2 token rejections audited")
	}
}

func TestSetupTokenIgnoresCaseDashAndWhitespace(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	wantRedirect(t, ta.postForm("/auth/setup", setupForm("owner", strongPassword, "abcde fghjk"), ""), "/")
}

func TestSetupTokenIsSingleUse(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	wantRedirect(t, ta.postForm("/auth/setup", setupForm("owner", strongPassword, testSetupToken), ""), "/")
	if _, ok := ta.app.setupToken.Current(); ok {
		t.Fatal("the token is still active after setup")
	}
}

func TestSetupWithoutAnActiveTokenAlwaysFails(t *testing.T) {
	ta := newTestApp(t, testAppOptions{noSetupToken: true})
	wantStatus(t, ta.postForm("/auth/setup", setupForm("owner", strongPassword, ""), ""), http.StatusForbidden)
}

func TestSetupThrottleKeepsWrongGuessesCharged(t *testing.T) {
	ta := newTestApp(t, testAppOptions{setupLimit: 2})
	for range 2 {
		wantStatus(t, ta.postForm("/auth/setup", setupForm("owner", strongPassword, "WRONG-TOKEN"), ""), http.StatusForbidden)
	}
	response := ta.postForm("/auth/setup", setupForm("owner", strongPassword, testSetupToken), "")
	wantStatus(t, response, http.StatusTooManyRequests)
	wantContains(t, response, "Too many setup attempts")
	if len(ta.auditEventsNamed("auth.setup.throttled")) != 1 {
		t.Error("throttle was not audited")
	}
}

func TestSetupThrottleRefundsARightTokenWithAWeakPassword(t *testing.T) {
	ta := newTestApp(t, testAppOptions{setupLimit: 2})
	for range 4 {
		wantStatus(t, ta.postForm("/auth/setup", setupForm("owner", "short", testSetupToken), ""), http.StatusOK)
	}
}

func TestSetupChecksExistingUsersBeforeTheToken(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	ta.createUser("lifter", strongPassword, models.RoleUser)
	wantRedirect(t, ta.postForm("/auth/setup", setupForm("owner", strongPassword, "WRONG"), ""), "/auth/login")
}

func TestOnlyOneConcurrentSetupWins(t *testing.T) {
	ta := newTestApp(t, testAppOptions{pool: newFilePool(t)})

	const attempts = 8
	results := make([]response, attempts)
	var group sync.WaitGroup
	for index := range attempts {
		group.Add(1)
		go func() {
			defer group.Done()
			username := "owner" + strings.Repeat("x", index)
			results[index] = ta.postForm("/auth/setup", setupForm(username, strongPassword, testSetupToken), "")
		}()
	}
	group.Wait()

	winners := 0
	for _, response := range results {
		wantStatus(t, response, http.StatusSeeOther)
		switch response.location() {
		case "/":
			winners++
		case "/auth/login":
		default:
			t.Fatalf("unexpected Location %q", response.location())
		}
	}
	if winners != 1 {
		t.Fatalf("%d setups won, want exactly 1", winners)
	}
	if count := ta.count("SELECT COUNT(*) FROM users"); count != 1 {
		t.Fatalf("%d users, want 1", count)
	}
}

func TestSetupStoresTheTrimmedUsername(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	// A raw "+" in a urlencoded body is a space, so this submits "  lifter  ".
	body := "username=++lifter++&password=" + strongPassword + "&setup_token=" + testSetupToken

	wantRedirect(t, ta.do(newRequest(http.MethodPost, "/auth/setup", body, "")), "/")

	if _, err := ta.app.users.FindByUsername(t.Context(), "lifter"); err != nil {
		t.Fatalf("user not stored under the trimmed name: %v", err)
	}
}

func TestUsernameValidation(t *testing.T) {
	tests := []struct {
		name     string
		username string
		want     string
	}{
		{"too long", strings.Repeat("a", 65), "at most 64"},
		{"control characters", "a\nb", "control characters"},
		{"empty after trimming", "   ", "Username is required"},
	}
	for _, test := range tests {
		t.Run("setup "+test.name, func(t *testing.T) {
			ta := newTestApp(t, testAppOptions{})
			response := ta.postForm("/auth/setup", setupForm(test.username, strongPassword, testSetupToken), "")
			wantStatus(t, response, http.StatusOK)
			wantContains(t, response, test.want)
		})
		t.Run("new user "+test.name, func(t *testing.T) {
			ta := newTestApp(t, testAppOptions{})
			admin := ta.signInAdmin()
			response := ta.postForm("/users/new", url.Values{"username": {test.username}, "password": {strongPassword}}, admin.cookie)
			wantStatus(t, response, http.StatusOK)
			wantContains(t, response, test.want)
		})
	}
}

func TestNewUserTrimsTheUsernameAndRejectsDuplicates(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	create := func(username string) response {
		return ta.postForm("/users/new", url.Values{"username": {username}, "password": {strongPassword}}, admin.cookie)
	}

	wantRedirect(t, create("  bob  "), "/users")
	user, err := ta.app.users.FindByUsername(t.Context(), "bob")
	if err != nil || user.Role != models.RoleUser {
		t.Fatalf("created user = %+v, %v; want a regular user named bob", user, err)
	}
	duplicate := create("bob ")
	wantStatus(t, duplicate, http.StatusOK)
	wantContains(t, duplicate, "Username already exists")
}
