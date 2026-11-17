package models

import "time"

// WorkoutSession's Date is a calendar date (UTC midnight); it must not be
// converted to another timezone.
type WorkoutSession struct {
	ID         string
	UserID     string
	Date       time.Time
	Notes      *string
	ShareToken *string
	// ShareExpiresAt nil means the share link (if any) never expires.
	ShareExpiresAt *time.Time
	CreatedAt      time.Time
}

// RepSchemes are the rep-count suggestions of the Add Set form, in display
// order. Suggestions only; the field accepts any positive integer.
var RepSchemes = []int{1, 3, 5, 6, 8, 10, 12, 15, 20}

type WorkoutLog struct {
	ID         string
	SessionID  string
	ExerciseID string
	SetNumber  int
	Reps       int
	Weight     float64
	RPE        *int
	CreatedAt  time.Time
}

type WorkoutLogWithExercise struct {
	ID           string
	SessionID    string
	ExerciseID   string
	ExerciseName string
	SetNumber    int
	Reps         int
	Weight       float64
	RPE          *int
	// IsPR: the set matches the all-time best weight for its exercise.
	IsPR bool
	// IsRecentPR: it matches the best weight within the rolling window.
	IsRecentPR bool
}
