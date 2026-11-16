package server

import "net/http"

// Handler is the whole app, outermost layer first: HSTS and baseline headers,
// the CSRF guard, the session layer, then the routes. The order matters: the
// headers must reach even a CSRF 403, and the guard must run before any
// session lookup touches the database.
func (a *App) Handler() http.Handler {
	mux := http.NewServeMux()
	a.registerRoutes(mux)
	return a.securityHeaders(a.csrfGuard(a.sessionMiddleware(mux)))
}

// registerRoutes lists every route of the app. Each group has one owner:
//
//   - staticRoutes, authRoutes, userRoutes: this package's handlers_*.go.
//   - workoutRoutes (routes_workouts.go): /workouts*, /shared/{token} and
//     /exercises*.
//   - statsRoutes (routes_stats.go): the dashboard "/", /stats* and /settings*.
//
// A later phase adds its handlers_*.go file and fills in its own routes_*.go;
// neither this file nor the middleware needs to change.
func (a *App) registerRoutes(mux *http.ServeMux) {
	a.staticRoutes(mux)
	a.authRoutes(mux)
	a.userRoutes(mux)
	a.workoutRoutes(mux)
	a.statsRoutes(mux)
}
