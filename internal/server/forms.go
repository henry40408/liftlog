package server

import (
	"errors"
	"fmt"
	"math"
	"mime"
	"net/http"
	"net/url"
	"strconv"
)

// maxFormBytes matches axum's default body limit.
const maxFormBytes = 2 << 20

// formValues is a parsed urlencoded body. Missing and empty fields differ the
// way they did under axum: required() fails on a missing one only.
type formValues struct {
	values url.Values
}

// readForm parses an urlencoded POST body with axum's Form semantics: 415
// without the right content type, 413 over 2 MiB, 422 for a malformed body.
// Call it after the auth checks, so an unauthorised caller never gets a form
// error.
func readForm(w http.ResponseWriter, r *http.Request) (formValues, error) {
	mediaType, _, _ := mime.ParseMediaType(r.Header.Get("Content-Type"))
	if mediaType != "application/x-www-form-urlencoded" {
		return formValues{}, formError(http.StatusUnsupportedMediaType,
			"Form requests must have `Content-Type: application/x-www-form-urlencoded`")
	}
	r.Body = http.MaxBytesReader(w, r.Body, maxFormBytes)
	if err := r.ParseForm(); err != nil {
		var tooLarge *http.MaxBytesError
		if errors.As(err, &tooLarge) {
			return formValues{}, formError(http.StatusRequestEntityTooLarge, "length limit exceeded")
		}
		return formValues{}, formError(http.StatusUnprocessableEntity,
			"Failed to deserialize form body: "+err.Error())
	}
	// Duplicate keys take the first value; the Rust extractor rejected them with 422.
	return formValues{values: r.PostForm}, nil
}

// required is a field the form struct had no default for.
func (f formValues) required(name string) (string, error) {
	if !f.values.Has(name) {
		return "", formError(http.StatusUnprocessableEntity,
			fmt.Sprintf("Failed to deserialize form body: missing field `%s`", name))
	}
	return f.values.Get(name), nil
}

// optional is a field with a default of "".
func (f formValues) optional(name string) string {
	return f.values.Get(name)
}

// parseFiniteFloat parses a decimal number and rejects inf and NaN.
// Rust's f64 parser accepted them, but the progress chart cannot draw such a
// value and the stats arithmetic would poison every aggregate, so weight
// fields refuse them up front.
func parseFiniteFloat(text string) (float64, error) {
	value, err := strconv.ParseFloat(text, 64)
	if err != nil {
		return 0, err
	}
	if math.IsInf(value, 0) || math.IsNaN(value) {
		return 0, fmt.Errorf("%q is not a finite number", text)
	}
	return value, nil
}
