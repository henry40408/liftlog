package views

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/henry40408/liftlog/internal/version"
)

func loadLayoutGolden(t *testing.T, name string) string {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join("testdata", name))
	if err != nil {
		t.Fatal(err)
	}
	return string(raw)
}

// The goldens were captured from a Rust build with its own git version.
func withCurrentVersion(golden string) string {
	const captured = "v0.30.1-1-g19c79d8"
	return strings.ReplaceAll(golden, captured, version.Version())
}

func TestPageShellMatchesRustGolden(t *testing.T) {
	golden := withCurrentVersion(loadLayoutGolden(t, "login.golden.html"))
	wantShell, _, _ := strings.Cut(golden, "<main>")

	gotShell, _, _ := strings.Cut(page("Login - LiftLog", node("<main>")), "<main>")
	if gotShell != wantShell {
		t.Errorf("got:\n%s\nwant:\n%s", gotShell, wantShell)
	}
}

func TestPageClosesTheShell(t *testing.T) {
	if got := page("t", node("x")); !strings.HasSuffix(got, `<div class="app">x</div></body></html>`) {
		t.Errorf("got %q", got)
	}
}

func TestPageEscapesTheTitle(t *testing.T) {
	if got := page(`<b>"&`, node("")); !strings.Contains(got, "<title>&lt;b&gt;&quot;&amp;</title>") {
		t.Errorf("title not escaped: %s", got)
	}
}

func navOfLayoutGolden(t *testing.T, name string) string {
	t.Helper()
	golden := loadLayoutGolden(t, name)
	_, rest, ok := strings.Cut(golden, "<nav")
	if !ok {
		t.Fatalf("%s has no nav", name)
	}
	nav, _, _ := strings.Cut(rest, "</nav>")
	return "<nav" + nav + "</nav>"
}

func TestNavForAdminMatchesRustGolden(t *testing.T) {
	want := navOfLayoutGolden(t, "dashboard.golden.html")
	if !strings.Contains(want, "admin-badge") {
		t.Fatal("precondition: the golden's user is an admin")
	}
	if got := string(nav(NavUser{Username: "alice", IsAdmin: true})); got != want {
		t.Errorf("got:\n%s\nwant:\n%s", got, want)
	}
}

func TestNavForPlainUserHasNoAdminBadge(t *testing.T) {
	got := string(nav(NavUser{Username: "bob"}))
	if strings.Contains(got, "admin-badge") {
		t.Errorf("badge shown to a plain user: %s", got)
	}
	if !strings.Contains(got, `<span class="username">bob</span>`) {
		t.Errorf("username missing: %s", got)
	}
}

func TestNavEscapesTheUsername(t *testing.T) {
	got := string(nav(NavUser{Username: `<script>"x"`}))
	if strings.Contains(got, "<script>") || !strings.Contains(got, "&lt;script&gt;&quot;x&quot;") {
		t.Errorf("username not escaped: %s", got)
	}
}
