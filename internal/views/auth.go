package views

// authLogo is the heading block both signed-out pages start with.
func authLogo(subtitle string) node {
	return group(
		el("div", []attr{a("class", "auth-logo")}, text("Lift"), el("span", nil, text("Log"))),
		el("div", []attr{a("class", "auth-subtitle")}, text(subtitle)),
	)
}

func authPage(title, subtitle, errMsg string, form node) string {
	return page(title, el("main", nil,
		el("div", []attr{a("class", "auth-container")}, authLogo(subtitle), errorBox(errMsg), form),
	))
}

// Login renders the sign-in page; errMsg is empty when there is nothing to show.
func Login(errMsg string) string {
	form := el("form", []attr{a("method", "post"), a("action", "/auth/login")},
		labelledInput("Username", "username", void("input",
			a("type", "text"), a("id", "username"), a("name", "username"),
			flag("required"), flag("autofocus"), a("autocomplete", "username"))),
		labelledInput("Password", "password", void("input",
			a("type", "password"), a("id", "password"), a("name", "password"),
			flag("required"), a("autocomplete", "current-password"))),
		el("button", []attr{a("type", "submit")}, text("Login")),
	)
	return authPage("Login - LiftLog", "Track your iron journey", errMsg, form)
}

const setupTokenHelp = "Find it on the server's stderr (`docker logs` shows it), on the line starting with \"Setup token\". It is shown only until the first account exists."

// Setup renders the first-run page that creates the first admin.
func Setup(errMsg string) string {
	form := el("form", []attr{a("method", "post"), a("action", "/auth/setup")},
		formGroup(
			el("label", []attr{a("for", "setup_token")}, text("Setup token")),
			text(" "),
			void("input", a("type", "text"), a("id", "setup_token"), a("name", "setup_token"),
				flag("required"), flag("autofocus"), a("autocomplete", "off"),
				a("autocapitalize", "characters"), a("spellcheck", "false"), a("placeholder", "XXXXX-XXXXX")),
			text(" "),
			el("small", nil, text(setupTokenHelp)),
		),
		labelledInput("Username", "username", void("input",
			a("type", "text"), a("id", "username"), a("name", "username"), flag("required"))),
		labelledInput("Password (min 8 characters)", "password", newPasswordInput("password")),
		el("button", []attr{a("type", "submit")}, text("Create Account")),
	)
	return authPage("Setup - LiftLog", "Create the first user account to get started", errMsg, form)
}

// newPasswordInput is the password field with the length limits of the policy.
func newPasswordInput(id string) node {
	return void("input", a("type", "password"), a("id", id), a("name", id),
		flag("required"), a("minlength", "12"), a("maxlength", "128"))
}
