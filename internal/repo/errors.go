// Package repo holds the database access of every entity.
package repo

import "errors"

// ErrNotFound means no row matched; handlers map it to a 404.
var ErrNotFound = errors.New("not found")

// ErrExerciseInUse means a delete hit ON DELETE RESTRICT; handlers map it to
// a 400 and show the message.
//
//nolint:staticcheck // ST1005: the message is shown to users verbatim
var ErrExerciseInUse = errors.New("This exercise is used by existing workout logs and cannot be deleted")
