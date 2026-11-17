package server

import "net/http"

// statsRoutes registers the dashboard, /stats* and /settings* routes.
func (a *App) statsRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /{$}", a.dashboard)
	mux.HandleFunc("GET /stats", a.statsIndex)
	mux.HandleFunc("GET /stats/exercise/{id}", a.exerciseStats)
	mux.HandleFunc("GET /stats/prs", a.prsList)
	mux.HandleFunc("GET /settings", a.settingsPage)
	mux.HandleFunc("POST /settings/password", a.changePassword)
	mux.HandleFunc("POST /settings/timezone", a.changeTimezone)
	mux.HandleFunc("GET /settings/logout-others", a.confirmLogoutOthers)
	mux.HandleFunc("POST /settings/logout-others", a.logoutOthers)
}
