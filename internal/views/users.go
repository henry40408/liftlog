package views

import "github.com/henry40408/liftlog/internal/models"

// NewUser renders the admin form that creates a user.
func NewUser(user NavUser, errMsg string) string {
	return page("New User - LiftLog", group(
		nav(user),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text("Add New User"))),
			errorBox(errMsg),
			el("form", []attr{a("method", "post"), a("action", "/users/new")},
				labelledInput("Username", "username", void("input",
					a("type", "text"), a("id", "username"), a("name", "username"),
					flag("required"), flag("autofocus"))),
				labelledInput("Password (min 8 characters)", "password", newPasswordInput("password")),
				el("button", []attr{a("type", "submit")}, text("Create User")),
			),
			backLink("/users", "Back to Users"),
		),
	))
}

// ConfirmActionView is the promote/delete-user confirmation, which also
// re-checks the admin's password.
type ConfirmActionView struct {
	User        NavUser
	ActionLabel string
	Consequence string
	FormAction  string
	Error       string
}

func ConfirmAction(view ConfirmActionView) string {
	return page(view.ActionLabel+" - LiftLog", group(
		nav(view.User),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text(view.ActionLabel))),
			errorBox(view.Error),
			el("p", []attr{a("class", "mb-4")}, text(view.Consequence)),
			el("form", []attr{a("method", "post"), a("action", view.FormAction)},
				labelledInput("Confirm your password to continue", "current_password", void("input",
					a("type", "password"), a("id", "current_password"), a("name", "current_password"),
					flag("required"), a("autocomplete", "current-password"), flag("autofocus"))),
				el("button", []attr{a("type", "submit"), a("class", "btn btn-danger")}, text(view.ActionLabel)),
				text(" "),
				el("a", []attr{a("href", "/users"), a("class", "btn btn-ghost")}, text("Cancel")),
			),
		),
	))
}

// UsersListView is the users page. ViewerID is the signed-in user's id, which
// marks their own row "(you)".
type UsersListView struct {
	User     NavUser
	ViewerID string
	Users    []models.UserListItem
}

func UsersList(view UsersListView) string {
	return page("Users - LiftLog", group(
		nav(view.User),
		el("main", nil,
			el("div", []attr{a("class", "page-header")}, el("h1", nil, text("Users"))),
			addUserLink(view.User),
			usersTable(view),
		),
	))
}

func addUserLink(viewer NavUser) node {
	if !viewer.IsAdmin {
		return ""
	}
	return el("p", []attr{a("class", "mb-4")},
		el("a", []attr{a("href", "/users/new"), a("class", "btn btn-primary")}, text("+ Add New User")))
}

func usersTable(view UsersListView) node {
	headers := []node{el("th", nil, text("Username")), el("th", nil, text("Role")), el("th", nil, text("Created"))}
	if view.User.IsAdmin {
		headers = append(headers, el("th", nil, text("Actions")))
	}
	rows := make([]node, 0, len(view.Users))
	for _, u := range view.Users {
		rows = append(rows, userRow(view, u))
	}
	return el("table", []attr{a("class", "data-table")},
		el("thead", nil, el("tr", nil, headers...)),
		el("tbody", nil, rows...),
	)
}

func userRow(view UsersListView, u models.UserListItem) node {
	cells := []node{
		el("td", []attr{a("data-label", "Username"), a("style", "color: var(--text-primary);")}, text(u.Username)),
		el("td", []attr{a("data-label", "Role")}, roleLabel(u.Role)),
		el("td", []attr{a("data-label", "Created")}, utcTime(u.CreatedAt)),
	}
	if view.User.IsAdmin {
		cells = append(cells, el("td", []attr{a("data-label", "Actions")}, userActions(view.ViewerID, u)))
	}
	return el("tr", nil, cells...)
}

func roleLabel(role models.UserRole) node {
	if !role.IsAdmin() {
		return text(string(role))
	}
	return el("span", []attr{a("style", "color: var(--gold);")}, text(string(role)))
}

// userActions has deliberately no data-confirm: those pages re-check the
// admin's password.
func userActions(viewerID string, u models.UserListItem) node {
	if u.ID == viewerID {
		return el("span", []attr{a("class", "muted")}, text("(you)"))
	}
	deleteLink := el("a", []attr{a("href", "/users/"+u.ID+"/delete"), a("class", "btn btn-danger btn-sm")}, text("Delete"))
	if u.Role.IsAdmin() {
		return el("div", []attr{a("class", "actions")}, deleteLink)
	}
	promoteLink := el("a", []attr{a("href", "/users/"+u.ID+"/promote"), a("class", "btn btn-ghost btn-sm")}, text("Promote"))
	return el("div", []attr{a("class", "actions")}, promoteLink, text(" "), deleteLink)
}
