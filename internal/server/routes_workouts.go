package server

import "net/http"

// workoutRoutes registers the workout, log, share and exercise routes.
func (a *App) workoutRoutes(mux *http.ServeMux) {
	a.workoutPageRoutes(mux)
	a.logRoutes(mux)
	a.shareRoutes(mux)
	a.exerciseRoutes(mux)
}

func (a *App) workoutPageRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /workouts", a.workoutsList)
	mux.HandleFunc("POST /workouts", a.createWorkout)
	mux.HandleFunc("GET /workouts/new", a.newWorkoutPage)
	mux.HandleFunc("GET /workouts/{id}", a.showWorkout)
	mux.HandleFunc("POST /workouts/{id}", a.updateWorkout)
	mux.HandleFunc("GET /workouts/{id}/edit", a.editWorkoutPage)
	mux.HandleFunc("GET /workouts/{id}/delete", a.confirmDeleteWorkout)
	mux.HandleFunc("POST /workouts/{id}/delete", a.deleteWorkout)
}

func (a *App) logRoutes(mux *http.ServeMux) {
	mux.HandleFunc("POST /workouts/{id}/logs", a.addLog)
	mux.HandleFunc("GET /workouts/{id}/logs/{log_id}/delete", a.confirmDeleteLog)
	mux.HandleFunc("POST /workouts/{id}/logs/{log_id}/delete", a.deleteLog)
	mux.HandleFunc("GET /workouts/{id}/logs/{log_id}/edit", a.editLogPage)
	mux.HandleFunc("POST /workouts/{id}/logs/{log_id}", a.updateLog)
}

func (a *App) shareRoutes(mux *http.ServeMux) {
	mux.HandleFunc("POST /workouts/{id}/share", a.shareWorkout)
	mux.HandleFunc("GET /workouts/{id}/revoke-share", a.confirmRevokeShare)
	mux.HandleFunc("POST /workouts/{id}/revoke-share", a.revokeShare)
	mux.HandleFunc("GET /shared/{token}", a.viewShared)
}

func (a *App) exerciseRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /exercises", a.exercisesList)
	mux.HandleFunc("POST /exercises", a.createExercise)
	mux.HandleFunc("GET /exercises/new", a.newExercisePage)
	mux.HandleFunc("GET /exercises/{id}/edit", a.editExercisePage)
	mux.HandleFunc("POST /exercises/{id}", a.updateExercise)
	mux.HandleFunc("GET /exercises/{id}/delete", a.confirmDeleteExercise)
	mux.HandleFunc("POST /exercises/{id}/delete", a.deleteExercise)
}
