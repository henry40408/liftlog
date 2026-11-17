package server

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"

	"github.com/henry40408/liftlog/internal/models"
)

func TestAppErrorMessageAndCause(t *testing.T) {
	cause := errors.New("disk on fire")
	wrapped := internalError(cause)
	if got, want := wrapped.Error(), "Internal error: disk on fire"; got != want {
		t.Errorf("Error() = %q, want %q", got, want)
	}
	if !errors.Is(wrapped, cause) {
		t.Error("errors.Is cannot reach the cause through Unwrap")
	}

	plain := notFound("Nope")
	if got := plain.Error(); got != "Nope" {
		t.Errorf("Error() without a cause = %q", got)
	}
	if errors.Unwrap(plain) != nil {
		t.Error("an error without a cause unwrapped to something")
	}
}

func TestReadFormRejectsBadBodies(t *testing.T) {
	cases := []struct {
		name        string
		contentType string
		body        string
		status      int
		message     string
	}{
		{"no content type", "", "a=1", http.StatusUnsupportedMediaType, "application/x-www-form-urlencoded"},
		{"json content type", "application/json", `{"a":1}`, http.StatusUnsupportedMediaType, "application/x-www-form-urlencoded"},
		{"oversized body", "application/x-www-form-urlencoded", "a=" + strings.Repeat("x", maxFormBytes), http.StatusRequestEntityTooLarge, "length limit exceeded"},
		{"malformed escape", "application/x-www-form-urlencoded", "a=%zz", http.StatusUnprocessableEntity, "Failed to deserialize form body"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			request := httptest.NewRequest(http.MethodPost, "/", strings.NewReader(tc.body))
			if tc.contentType != "" {
				request.Header.Set("Content-Type", tc.contentType)
			}
			_, err := readForm(httptest.NewRecorder(), request)

			var failure *appError
			if !errors.As(err, &failure) {
				t.Fatalf("err = %v, want an appError", err)
			}
			if failure.status != tc.status || !strings.Contains(failure.message, tc.message) {
				t.Errorf("got %d %q, want %d containing %q", failure.status, failure.message, tc.status, tc.message)
			}
		})
	}
}

func TestReadFormAcceptsAContentTypeWithParameters(t *testing.T) {
	request := httptest.NewRequest(http.MethodPost, "/", strings.NewReader("a=1&a=2&b="))
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")

	form, err := readForm(httptest.NewRecorder(), request)
	if err != nil {
		t.Fatal(err)
	}
	if got := form.optional("a"); got != "1" {
		t.Errorf("a duplicate key gave %q, want the first value", got)
	}
	if _, err := form.required("b"); err != nil {
		t.Errorf("an empty field counts as present, got %v", err)
	}
	if _, err := form.required("missing"); err == nil {
		t.Error("a missing field was accepted")
	}
}

// The session middleware wraps the writer to stamp headers; a handler that
// needs Flush or deadlines must still reach the real writer.
func TestSessionWriterUnwrapsForResponseController(t *testing.T) {
	recorder := httptest.NewRecorder()
	wrapped := &sessionWriter{ResponseWriter: recorder}

	if wrapped.Unwrap() != http.ResponseWriter(recorder) {
		t.Fatal("Unwrap did not return the underlying writer")
	}
	if err := http.NewResponseController(wrapped).Flush(); err != nil {
		t.Fatalf("Flush through the wrapper: %v", err)
	}
	if !recorder.Flushed {
		t.Error("the flush never reached the recorder")
	}
}

// breakTable renames a table, so every query that touches it fails while the
// session and user lookups keep working.
func (ta *testApp) breakTable(name string) {
	ta.t.Helper()
	ta.mustExec("ALTER TABLE " + name + " RENAME TO " + name + "_gone")
}

// failWrites makes every write of the given kind on a table abort.
func (ta *testApp) failWrites(kind, table string) {
	ta.t.Helper()
	ta.mustExec("CREATE TRIGGER fail_" + kind + "_" + table + " BEFORE " + kind + " ON " + table +
		" BEGIN SELECT RAISE(ABORT, 'forced failure'); END")
}

func requireDatabaseError(t *testing.T, got response) {
	t.Helper()
	if got.status() != http.StatusInternalServerError || got.text() != "Database error" {
		t.Fatalf("got %d %q, want 500 \"Database error\"", got.status(), got.text())
	}
}

func TestOverviewPagesReportADatabaseFailureAs500(t *testing.T) {
	cases := []struct {
		name      string
		path      string
		breakName string
	}{
		{"dashboard volume query", "/", "workout_logs"},
		{"stats volume query", "/stats", "workout_logs"},
		{"stats PR query", "/stats", "exercises"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			ta := newTestApp(t, testAppOptions{})
			member := ta.signInUser()
			ta.breakTable(tc.breakName)

			requireDatabaseError(t, ta.get(tc.path, member.cookie))
		})
	}
}

func TestUpdateWorkoutRejectsBadFormsAndSurfacesWriteFailures(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	member := ta.signInUser()
	workout := ta.wkWorkout(member.user.ID, nil)
	path := "/workouts/" + workout.ID

	missing := ta.postForm(path, url.Values{"notes": {"n"}}, member.cookie)
	if missing.status() != http.StatusUnprocessableEntity || !strings.Contains(missing.text(), "missing field `date`") {
		t.Errorf("missing date: %d %q", missing.status(), missing.text())
	}
	badDate := ta.postForm(path, url.Values{"date": {"2024-13-45"}}, member.cookie)
	if badDate.status() != http.StatusUnprocessableEntity || !strings.Contains(badDate.text(), "invalid date") {
		t.Errorf("bad date: %d %q", badDate.status(), badDate.text())
	}

	ta.failWrites("UPDATE", "workout_sessions")
	requireDatabaseError(t, ta.postForm(path, url.Values{"date": {"2024-02-01"}}, member.cookie))
}

func TestNewUserSubmitRejectsIncompleteFormsAndSurfacesWriteFailures(t *testing.T) {
	ta := newTestApp(t, testAppOptions{})
	admin := ta.signInAdmin()

	wrongType := ta.do(withHeader(newRequest(http.MethodPost, "/users/new", `{"username":"x"}`, admin.cookie),
		"Content-Type", "application/json"))
	if wrongType.status() != http.StatusUnsupportedMediaType {
		t.Errorf("json body: %d", wrongType.status())
	}
	noUsername := ta.postForm("/users/new", url.Values{"password": {strongPassword}}, admin.cookie)
	if noUsername.status() != http.StatusUnprocessableEntity || !strings.Contains(noUsername.text(), "missing field `username`") {
		t.Errorf("no username: %d %q", noUsername.status(), noUsername.text())
	}
	noPassword := ta.postForm("/users/new", url.Values{"username": {"fresh"}}, admin.cookie)
	if noPassword.status() != http.StatusUnprocessableEntity || !strings.Contains(noPassword.text(), "missing field `password`") {
		t.Errorf("no password: %d %q", noPassword.status(), noPassword.text())
	}

	ta.failWrites("INSERT", "users")
	requireDatabaseError(t, ta.postForm("/users/new", url.Values{"username": {"fresh"}, "password": {strongPassword}}, admin.cookie))
	if ta.count("SELECT COUNT(*) FROM users WHERE username = 'fresh'") != 0 {
		t.Error("a failed create left a user behind")
	}
}

func TestViewSharedFailureBranches(t *testing.T) {
	share := func(t *testing.T) (*testApp, string) {
		t.Helper()
		ta := newTestApp(t, testAppOptions{})
		owner := ta.signInUser()
		workout := ta.wkWorkout(owner.user.ID, nil)
		ta.mustExec("UPDATE workout_sessions SET share_token = 'public-token' WHERE id = ?", workout.ID)
		return ta, "/shared/public-token"
	}

	t.Run("a failing log query is a database error", func(t *testing.T) {
		ta, path := share(t)
		ta.breakTable("exercises")
		requireDatabaseError(t, ta.get(path, ""))
	})
	t.Run("a vanished owner is a 404", func(t *testing.T) {
		ta, path := share(t)
		// Foreign keys off, so deleting the owner leaves the workout behind.
		ta.mustExec("PRAGMA foreign_keys = OFF")
		ta.mustExec("DELETE FROM users WHERE role = ?", string(models.RoleUser))
		ta.mustExec("PRAGMA foreign_keys = ON")

		got := ta.get(path, "")
		if got.status() != http.StatusNotFound || got.text() != "User not found" {
			t.Errorf("got %d %q, want 404 \"User not found\"", got.status(), got.text())
		}
	})
	t.Run("an unknown token is a 404", func(t *testing.T) {
		ta, _ := share(t)
		got := ta.get("/shared/other-token", "")
		if got.status() != http.StatusNotFound || got.text() != "Shared workout not found" {
			t.Errorf("got %d %q", got.status(), got.text())
		}
	})
}
