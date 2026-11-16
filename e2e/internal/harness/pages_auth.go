package harness

// LoginPage is the login form (/auth/login).
type LoginPage struct{ browser *Browser }

// Login returns the login page object.
func (b *Browser) Login() LoginPage { return LoginPage{browser: b} }

// Goto navigates to /auth/login.
func (p LoginPage) Goto() error { return p.browser.Goto("/auth/login") }

// Login fills the form and submits it, from wherever the browser is.
func (p LoginPage) Login(username, password string) error {
	if err := p.Goto(); err != nil {
		return err
	}
	if err := p.browser.Fill("username", username); err != nil {
		return err
	}
	if err := p.browser.Fill("password", password); err != nil {
		return err
	}
	return p.browser.ClickButton("Login")
}

// IsShowing reports whether the login form is on screen.
func (p LoginPage) IsShowing() (bool, error) {
	return p.browser.has(XPath("//button[normalize-space(.)='Login']"))
}

// Error is the error banner's text, with false when none is showing.
func (p LoginPage) Error() (string, bool, error) {
	return p.browser.TextOf(CSS(".error"))
}

// SetupPage is the first-run account form (/auth/setup).
type SetupPage struct{ browser *Browser }

// Setup returns the setup page object.
func (b *Browser) Setup() SetupPage { return SetupPage{browser: b} }

// Goto navigates to /auth/setup.
func (p SetupPage) Goto() error { return p.browser.Goto("/auth/setup") }

// Submit fills and submits the form with the server's real setup token, and
// with client-side validation off so short passwords reach the server.
func (p SetupPage) Submit(username, password string) error {
	return p.SubmitWithToken(username, password, p.browser.server.SetupToken)
}

// SubmitWithToken is Submit with an explicit setup token.
func (p SetupPage) SubmitWithToken(username, password, token string) error {
	if err := p.Goto(); err != nil {
		return err
	}
	if err := p.browser.DisableValidation("form"); err != nil {
		return err
	}
	for _, field := range [][2]string{{"setup_token", token}, {"username", username}, {"password", password}} {
		if err := p.browser.Fill(field[0], field[1]); err != nil {
			return err
		}
	}
	return p.browser.ClickButton("Create Account")
}

// IsShowing reports whether the setup form is on screen.
func (p SetupPage) IsShowing() (bool, error) {
	return p.browser.has(XPath("//button[normalize-space(.)='Create Account']"))
}

// Error is the error banner's text, with false when none is showing.
func (p SetupPage) Error() (string, bool, error) {
	return p.browser.TextOf(CSS(".error"))
}

// NavBar is the nav bar, present on every signed-in page.
type NavBar struct{ browser *Browser }

// Nav returns the nav bar object.
func (b *Browser) Nav() NavBar { return NavBar{browser: b} }

// SignOut submits the Sign Out form and waits for the login page; otherwise
// the next navigation cancels the logout and the old session survives.
func (n NavBar) SignOut() error {
	if err := n.browser.ClickNavigating(CSS("button.sign-out-btn")); err != nil {
		return err
	}
	return EventuallyEq("the URL after signing out", "/auth/login", n.browser.Path)
}

// has reports whether anything matches, without waiting.
func (b *Browser) has(by By) (bool, error) {
	count, err := b.Count(by)
	return count > 0, err
}
