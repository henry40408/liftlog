package server

import (
	"net/http"
	"regexp"
	"strings"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/auth"
	"github.com/henry40408/liftlog/internal/models"
)

var sixteenHex = regexp.MustCompile(`^[0-9a-f]{16}$`)

func TestLoginAuditsASessionFingerprintNotTheToken(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	ta.createUser("lifter", strongPassword, models.RoleUser)

	response := ta.postForm("/auth/login", loginForm("lifter", strongPassword), "")

	event := ta.onlyAuditEvent("session.created")
	if event["reason"] != "login" || event["username"] != "lifter" {
		t.Fatalf("event = %v", event)
	}
	fingerprint, _ := event["session_fp"].(string)
	if !sixteenHex.MatchString(fingerprint) {
		t.Fatalf("session_fp = %q, want 16 hex characters", fingerprint)
	}
	token := strings.TrimPrefix(response.cookieHeader(), "session=")
	if strings.Contains(ta.logs.String(), token) {
		t.Fatal("the raw session token reached the log")
	}
}

func TestFailedLoginAuditsTheUsernameNeverThePassword(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	ta.createUser("lifter", strongPassword, models.RoleUser)

	ta.postForm("/auth/login", loginForm("lifter", "hunter2-is-secret"), "")

	event := ta.onlyAuditEvent("auth.login.failed")
	if event["username"] != "lifter" {
		t.Fatalf("event = %v", event)
	}
	if strings.Contains(ta.logs.String(), "hunter2-is-secret") {
		t.Fatal("the attempted password reached the log")
	}
}

func TestUnknownUserAndWrongPasswordLogTheSameShape(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	ta.createUser("lifter", strongPassword, models.RoleUser)

	ta.postForm("/auth/login", loginForm("lifter", "wrong-password"), "")
	ta.postForm("/auth/login", loginForm("ghost", "wrong-password"), "")

	events := ta.auditEventsNamed("auth.login.failed")
	if len(events) != 2 {
		t.Fatalf("got %d events, want 2", len(events))
	}
	for _, event := range events {
		delete(event, "username")
		delete(event, "time")
	}
	if len(events[0]) != len(events[1]) {
		t.Fatalf("the events differ: %v vs %v", events[0], events[1])
	}
	for key, value := range events[0] {
		if events[1][key] != value {
			t.Errorf("%s differs: %v vs %v", key, value, events[1][key])
		}
	}
}

func TestAuditTruncatesTheUserAgent(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	request := newRequest(http.MethodPost, "/auth/login", loginForm("ghost", "wrong-password").Encode(), "")
	request.Header.Set("User-Agent", strings.Repeat("ä", 400))

	ta.do(request)

	userAgent, _ := ta.onlyAuditEvent("auth.login.failed")["user_agent"].(string)
	if got := len([]rune(userAgent)); got != 256 {
		t.Fatalf("user_agent has %d characters, want 256", got)
	}
}

func TestReauthFailureAuditsTheActorWithoutSecrets(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	target := ta.createUser("target", strongPassword, models.RoleUser)

	ta.postForm("/users/"+target.ID+"/promote", confirmForm("not-the-password-1"), admin.cookie)

	event := ta.onlyAuditEvent("auth.reauth.failed")
	if event["user_id"] != admin.user.ID || event["action"] != "promote_user" {
		t.Fatalf("event = %v", event)
	}
	if fingerprint, _ := event["actor_session_fp"].(string); !sixteenHex.MatchString(fingerprint) {
		t.Errorf("actor_session_fp = %q", fingerprint)
	}
	logs := ta.logs.String()
	if strings.Contains(logs, admin.token) || strings.Contains(logs, "not-the-password-1") {
		t.Fatal("a secret reached the log")
	}
}

func TestSessionRenewalIsAudited(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()
	ta.ageSessionTouch(admin.token, 2)

	ta.get("/", admin.cookie)

	if event := ta.onlyAuditEvent("session.renewed"); event["user_id"] != admin.user.ID {
		t.Fatalf("event = %v", event)
	}
}

func backoffDelays(ta *testApp, username string, attempts int) []float64 {
	for range attempts {
		ta.postForm("/auth/login", loginForm(username, "wrong-password"), "")
	}
	var delays []float64
	for _, event := range ta.auditEventsNamed("auth.login.failed") {
		if event["username"] == username {
			delays = append(delays, event["backoff_ms"].(float64))
		}
	}
	return delays
}

func TestBackoffClimbsPerAccountAndResetsOnSuccess(t *testing.T) {
	schedule := auth.BackoffSchedule{FreeAttempts: 1, Base: 10 * time.Millisecond, Max: 40 * time.Millisecond, Window: time.Minute}
	ta := newTestApp(t, testAppOptions{loginBackoff: &schedule})
	ta.createUser("lifter", strongPassword, models.RoleUser)
	ta.createUser("other", strongPassword, models.RoleUser)

	got := backoffDelays(ta, "lifter", 3)
	if len(got) != 3 || got[0] != 0 || got[1] != 10 || got[2] != 20 {
		t.Fatalf("lifter delays = %v, want [0 10 20]", got)
	}
	if other := backoffDelays(ta, "other", 1); other[0] != 0 {
		t.Fatalf("another account inherited a delay: %v", other)
	}

	ta.postForm("/auth/login", loginForm("lifter", strongPassword), "")
	if after := backoffDelays(ta, "lifter", 1); after[len(after)-1] != 0 {
		t.Fatalf("a successful login did not reset the delay: %v", after)
	}
}
