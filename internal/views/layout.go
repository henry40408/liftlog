package views

import "github.com/henry40408/liftlog/internal/version"

// NavUser is what the navigation bar shows about the signed-in user.
type NavUser struct {
	Username string
	IsAdmin  bool
}

const fontsURL = "https://fonts.bunny.net/css2?family=Chakra+Petch:wght@400;600;700&family=Overpass+Mono:wght@400;500;600;700&display=swap"

// page is the document shell shared by every page. title is the full <title>.
func page(title string, body node) string {
	v := version.Version()
	return string(group(
		node("<!DOCTYPE html>"),
		el("html", []attr{a("lang", "zh-TW")},
			el("head", nil,
				void("meta", a("charset", "UTF-8")),
				void("meta", a("name", "viewport"), a("content", "width=device-width, initial-scale=1.0")),
				void("link", a("rel", "icon"), a("type", "image/svg+xml"), a("href", "/favicon.svg")),
				void("link", a("rel", "apple-touch-icon"), a("sizes", "180x180"), a("href", "/apple-touch-icon.png")),
				el("title", nil, text(title)),
				void("link", a("rel", "preconnect"), a("href", "https://fonts.bunny.net")),
				void("link", a("href", fontsURL), a("rel", "stylesheet")),
				void("link", a("rel", "stylesheet"), a("href", "/assets/app.css?v="+v)),
				// Blocking on purpose: inline page scripts use `window.LiftLog`.
				el("script", []attr{a("src", "/assets/app.js?v="+v)}),
			),
			el("body", nil,
				el("div", []attr{a("class", "app-accent-bar")}),
				el("div", []attr{a("class", "app")}, body),
			),
		),
	))
}

func nav(user NavUser) node {
	return el("nav", []attr{a("class", "nav-bar")},
		el("a", []attr{a("href", "/"), a("class", "nav-logo")}, text("Lift"), el("span", nil, text("Log"))),
		text(" "),
		void("input", a("type", "checkbox"), a("id", "nav-toggle"), a("class", "nav-toggle-checkbox")),
		text(" "),
		el("label", []attr{a("for", "nav-toggle"), a("class", "nav-toggle-label")},
			text(" "), el("span", []attr{a("class", "nav-toggle-icon")}), text(" ")),
		el("ul", []attr{a("class", "nav-links")},
			navLink("/", "Dashboard"),
			navLink("/workouts", "Workouts"),
			navLink("/exercises", "Exercises"),
			navLink("/stats", "Stats"),
			navLink("/settings", "Settings"),
			navLink("/users", "Users"),
		),
		el("div", []attr{a("class", "nav-user")},
			navUsername(user),
			el("form", []attr{a("action", "/auth/logout"), a("method", "post"), a("style", "display:inline;")},
				el("button", []attr{a("type", "submit"), a("class", "sign-out-btn")}, text("Sign Out")),
			),
		),
	)
}

func navLink(href, label string) node {
	return el("li", nil, el("a", []attr{a("href", href)}, text(label)))
}

func navUsername(user NavUser) node {
	if !user.IsAdmin {
		return el("span", []attr{a("class", "username")}, text(user.Username))
	}
	return el("span", []attr{a("class", "username")},
		text(user.Username), text(" "), el("span", []attr{a("class", "admin-badge")}, text("Admin")))
}
