package harness

import "fmt"

// sessionsTable is picked by header: /settings has several .data-table.
const sessionsTable = "//table[contains(@class,'data-table')][.//th[normalize-space(.)='Device']]"

// SettingsPage is /settings: the password form and the active-sessions table.
type SettingsPage struct{ browser *Browser }

// Settings returns the settings page object.
func (b *Browser) Settings() SettingsPage { return SettingsPage{browser: b} }

// Goto navigates to /settings.
func (p SettingsPage) Goto() error { return p.browser.Goto("/settings") }

// PasswordChange is what the change-password form is filled with.
type PasswordChange struct {
	Current string
	New     string
	Confirm string
}

// ChangePassword fills and submits the change-password form with client-side
// validation off, so short passwords reach the server's check.
func (p SettingsPage) ChangePassword(change PasswordChange) error {
	if err := p.Goto(); err != nil {
		return err
	}
	if err := p.browser.DisableValidation(`form[action="/settings/password"]`); err != nil {
		return err
	}
	for _, field := range [][2]string{
		{"current_password", change.Current},
		{"new_password", change.New},
		{"confirm_password", change.Confirm},
	} {
		if err := p.browser.Fill(field[0], field[1]); err != nil {
			return err
		}
	}
	return p.browser.ClickButton("Change Password")
}

// Success is the success banner's text, with false when none is showing.
func (p SettingsPage) Success() (string, bool, error) {
	return p.browser.TextOf(CSS(".alert-success"))
}

// Error is the error banner's text, with false when none is showing.
func (p SettingsPage) Error() (string, bool, error) {
	return p.browser.TextOf(CSS(".error"))
}

// LogOutOtherDevices ends every session but this one. The POST re-renders
// /settings, so callers wait for the success banner.
func (p SettingsPage) LogOutOtherDevices() error {
	if err := p.Goto(); err != nil {
		return err
	}
	return p.browser.ClickLink("Log out all other devices")
}

// SessionCount is how many rows the active-sessions table has.
func (p SettingsPage) SessionCount() (int, error) {
	return p.browser.Count(XPath(sessionsTable + "//tbody/tr"))
}

// MarksCurrentDevice reports whether the table marks one of its rows as this
// browser's own session.
func (p SettingsPage) MarksCurrentDevice() (bool, error) {
	return p.browser.has(XPath(sessionsTable + "//strong[normalize-space(.)='This device']"))
}

// FirstRowLabels is the data-label of every cell in the first session row:
// the headers td::before prints once the table collapses to cards.
func (p SettingsPage) FirstRowLabels() ([]string, error) {
	cells, err := p.browser.QueryAll(XPath(sessionsTable + "//tbody/tr[1]/td"))
	if err != nil {
		return nil, err
	}
	labels := make([]string, 0, len(cells))
	for _, cell := range cells {
		label, found, err := cell.Attr("data-label")
		if err != nil {
			return nil, err
		}
		if !found {
			return nil, fmt.Errorf("a session cell carries no data-label")
		}
		labels = append(labels, label)
	}
	return labels, nil
}

// FirstRowTimes is the rendered text of both timestamps in the first session
// row, after base.html rewrites them into the browser's zone.
func (p SettingsPage) FirstRowTimes() ([]string, error) {
	return p.browser.TextsOf(XPath(sessionsTable + "//tbody/tr[1]//time"))
}

// UsersPage is /users: the admin-only list and its row actions.
type UsersPage struct{ browser *Browser }

// Users returns the users page object.
func (b *Browser) Users() UsersPage { return UsersPage{browser: b} }

// Goto navigates to /users.
func (p UsersPage) Goto() error { return p.browser.Goto("/users") }

// Create creates a user through the admin form.
func (p UsersPage) Create(username, password string) error {
	if err := p.browser.Goto("/users/new"); err != nil {
		return err
	}
	if err := p.browser.Fill("username", username); err != nil {
		return err
	}
	if err := p.browser.Fill("password", password); err != nil {
		return err
	}
	return p.browser.ClickButton("Create User")
}

// RowsFor is how many rows on the list name that user.
func (p UsersPage) RowsFor(username string) (int, error) {
	return p.browser.Count(XPath(userRowXPath(username)))
}

// RowText is the whole row for that user, as text.
func (p UsersPage) RowText(username string) (string, error) {
	text, found, err := p.browser.TextOf(XPath(userRowXPath(username)))
	if err == nil && !found {
		return "", fmt.Errorf("no row for `%s` on the users page", username)
	}
	return text, err
}

// OpenAction opens a row action's password re-check page, never a
// window.confirm().
func (p UsersPage) OpenAction(username, action string) error {
	if err := p.Goto(); err != nil {
		return err
	}
	row, err := p.browser.Optional(XPath(userRowXPath(username)))
	if err != nil {
		return err
	}
	if row == nil {
		return fmt.Errorf("no row for `%s` on the users page", username)
	}
	return row.ClickLinkIn(action)
}

// DeleteLinksFor is how many delete links that user's row offers: 0 for the
// admin's own row.
func (p UsersPage) DeleteLinksFor(username string) (int, error) {
	return p.browser.Count(XPath(userRowXPath(username) + "//a[normalize-space(.)='Delete']"))
}

// MarksAsYou reports whether that row is marked as the signed-in admin's own.
func (p UsersPage) MarksAsYou(username string) (bool, error) {
	return p.browser.has(XPath(userRowXPath(username) + "//span[normalize-space(.)='(you)']"))
}

// LinksLabelled is how many links with that text the page offers.
func (p UsersPage) LinksLabelled(label string) (int, error) {
	return p.browser.Count(XPath("//a[normalize-space(.)=" + Quote(label) + "]"))
}

// ConfirmActionPage is the re-authentication page a promote or delete opens.
type ConfirmActionPage struct{ browser *Browser }

// ConfirmAction returns the re-authentication page object.
func (b *Browser) ConfirmAction() ConfirmActionPage { return ConfirmActionPage{browser: b} }

// Confirm confirms with a password; button is the full submit label, such as
// "Delete user".
func (p ConfirmActionPage) Confirm(password, button string) error {
	if err := p.browser.Fill("current_password", password); err != nil {
		return err
	}
	return p.browser.ClickButton(button)
}

// Error is the error banner's text, with false when none is showing.
func (p ConfirmActionPage) Error() (string, bool, error) {
	return p.browser.TextOf(CSS(".error"))
}

func userRowXPath(username string) string {
	return "//tr[td[@data-label='Username'][normalize-space(.)=" + Quote(username) + "]]"
}
