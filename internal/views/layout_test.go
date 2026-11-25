package views

import (
	"strings"
	"testing"
)

func TestPageShellMatchesRustGolden(t *testing.T) {
	shell, _, _ := strings.Cut(page("Login - LiftLog", node("<main>")), "<main>")
	assertPageGolden(t, "login-shell", shell)
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

func TestNavForAdminMatchesRustGolden(t *testing.T) {
	assertPageGolden(t, "dashboard-nav", string(nav(NavUser{Username: "alice", IsAdmin: true})))
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
