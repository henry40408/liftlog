package views

import "time"

// SessionRow is one entry of the active-session list.
type SessionRow struct {
	Token         string
	CreatedAt     time.Time
	LastTouchedAt time.Time
}

// SettingsView is the settings page. Timezone is empty until one is saved,
// Error and Success are empty when there is nothing to show, and
// CurrentSessionToken marks the row for "This device".
type SettingsView struct {
	User                NavUser
	Timezone            string
	TimezoneNames       []string
	GitVersion          string
	Error               string
	Success             string
	Sessions            []SessionRow
	CurrentSessionToken string
}

// timezonePrefillJS fills in the browser's zone only when none is saved yet.
// It is written byte for byte as the Rust page had it.
const timezonePrefillJS = `
        /* Prefill with the browser's zone only when none is saved yet. */
        (function () {
            var input = document.getElementById('timezone-input');
            if (!input || input.value) return;
            try {
                input.value = Intl.DateTimeFormat().resolvedOptions().timeZone || '';
            } catch (e) { /* leave empty */ }
        })();
    `

func Settings(view SettingsView) string {
	return page("Settings - LiftLog", group(
		nav(view.User),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text("Settings"))),
			successAlert(view.Success),
			errorBox(view.Error),
			timezoneSection(view),
			passwordSection(),
			sessionsSection(view),
			el("h2", nil, text("Application Info")),
			el("table", []attr{a("class", "data-table")},
				el("tbody", nil, el("tr", nil,
					el("td", []attr{a("style", "color: var(--text-muted); width: 150px;")}, text("Version")),
					el("td", nil, el("code", nil, text(view.GitVersion))),
				)),
			),
		),
	))
}

func successAlert(message string) node {
	if message == "" {
		return ""
	}
	return el("div", []attr{a("class", "alert alert-success"), a("style", "margin-bottom: var(--sp-5);")}, text(message))
}

func timezoneSection(view SettingsView) node {
	options := make([]node, 0, len(view.TimezoneNames))
	for _, name := range view.TimezoneNames {
		options = append(options, el("option", []attr{a("value", name)}))
	}
	return group(
		el("h2", []attr{a("id", "timezone")}, text("Timezone")),
		el("p", []attr{a("class", "muted")}, text(`Workout dates and the "This Week" / "This Month" stats follow this timezone.`)),
		el("form", []attr{a("method", "post"), a("action", "/settings/timezone")},
			formGroup(
				el("label", []attr{a("for", "timezone-input")}, text("Timezone (IANA name)")),
				text(" "),
				void("input", a("type", "text"), a("id", "timezone-input"), a("name", "timezone"),
					a("list", "timezone-names"), flag("required"), a("autocomplete", "off"),
					a("value", view.Timezone), a("placeholder", "Asia/Taipei")),
				el("datalist", []attr{a("id", "timezone-names")}, options...),
			),
			el("button", []attr{a("type", "submit")}, text("Save Timezone")),
		),
		el("script", nil, node(timezonePrefillJS)),
	)
}

func passwordSection() node {
	return group(
		el("h2", nil, text("Change Password")),
		el("form", []attr{a("method", "post"), a("action", "/settings/password")},
			labelledInput("Current Password", "current_password", void("input",
				a("type", "password"), a("id", "current_password"), a("name", "current_password"),
				flag("required"), a("autocomplete", "current-password"))),
			labelledInput("New Password", "new_password", changedPasswordInput("new_password")),
			labelledInput("Confirm New Password", "confirm_password", changedPasswordInput("confirm_password")),
			el("button", []attr{a("type", "submit")}, text("Change Password")),
		),
	)
}

func changedPasswordInput(id string) node {
	return void("input", a("type", "password"), a("id", id), a("name", id),
		flag("required"), a("minlength", "12"), a("maxlength", "128"), a("autocomplete", "new-password"))
}

func sessionsSection(view SettingsView) node {
	rows := make([]node, 0, len(view.Sessions))
	for _, s := range view.Sessions {
		rows = append(rows, sessionRow(s, s.Token == view.CurrentSessionToken))
	}
	return group(
		el("h2", nil, text("Active Sessions")),
		el("table", []attr{a("class", "data-table")},
			el("thead", nil, el("tr", nil,
				el("th", nil, text("Device")), el("th", nil, text("Last active")), el("th", nil, text("Signed in")))),
			el("tbody", nil, rows...),
		),
		el("p", []attr{a("style", "margin-top: var(--sp-4);")},
			el("a", []attr{a("href", "/settings/logout-others"), a("class", "btn btn-primary"),
				a("data-confirm", "Log out of all other devices? This device stays signed in.")},
				text("Log out all other devices"))),
	)
}

func sessionRow(s SessionRow, current bool) node {
	device := text("Other device")
	if current {
		device = el("strong", nil, text("This device"))
	}
	return el("tr", nil,
		el("td", []attr{a("data-label", "Device")}, device),
		el("td", []attr{a("data-label", "Last active")}, utcTime(s.LastTouchedAt)),
		el("td", []attr{a("data-label", "Signed in")}, utcTime(s.CreatedAt)),
	)
}
