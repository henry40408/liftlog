package server

import (
	"errors"
	"net/http"
)

// appError is a failure with the HTTP status and public message to send. The
// cause is logged, never sent to the client.
type appError struct {
	status  int
	message string
	cause   error
}

func (e *appError) Error() string {
	if e.cause != nil {
		return e.message + ": " + e.cause.Error()
	}
	return e.message
}

func (e *appError) Unwrap() error { return e.cause }

// notFound is a 404 that shows message.
func notFound(message string) error {
	return &appError{status: http.StatusNotFound, message: message}
}

// badRequest is a 400 that shows message.
func badRequest(message string) error {
	return &appError{status: http.StatusBadRequest, message: message}
}

// internalError is a 500 that hides cause behind "Internal error".
func internalError(cause error) error {
	return &appError{status: http.StatusInternalServerError, message: "Internal error", cause: cause}
}

// databaseError is a 500 that hides cause behind "Database error".
func databaseError(cause error) error {
	return &appError{status: http.StatusInternalServerError, message: "Database error", cause: cause}
}

// formError is the response axum's Form extractor gave for a bad body.
func formError(status int, message string) error {
	return &appError{status: status, message: message}
}

// fail writes err as a plain-text response. An error that is not an appError
// is a repository failure, so it is a 500 "Database error".
func (a *App) fail(w http.ResponseWriter, r *http.Request, err error) {
	var failure *appError
	if !errors.As(err, &failure) {
		failure = databaseError(err).(*appError)
	}
	if failure.status >= http.StatusInternalServerError {
		a.logger.Error("request failed", "method", r.Method, "path", r.URL.Path, "err", failure.Error())
	}
	writeText(w, failure.status, failure.message)
}

// writeText sends message as plain text without a trailing newline, like the
// Rust server did.
func writeText(w http.ResponseWriter, status int, message string) {
	header := w.Header()
	header.Set("Content-Type", "text/plain; charset=utf-8")
	header.Set("X-Content-Type-Options", "nosniff")
	w.WriteHeader(status)
	_, _ = w.Write([]byte(message))
}

// writeHTML sends a rendered view.
func writeHTML(w http.ResponseWriter, status int, body string) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.WriteHeader(status)
	_, _ = w.Write([]byte(body))
}

// redirect is the success answer of every POST: 303 See Other.
func redirect(w http.ResponseWriter, location string) {
	w.Header().Set("Location", location)
	w.WriteHeader(http.StatusSeeOther)
}
