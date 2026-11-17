package models

type Exercise struct {
	ID       string
	Name     string
	Category string
	UserID   string
}

type ExerciseCategory struct {
	Name        string
	DisplayName string
}

// Categories is code, not data: changing it is a code change, not a migration.
var Categories = []ExerciseCategory{
	{Name: "chest", DisplayName: "Chest"},
	{Name: "back", DisplayName: "Back"},
	{Name: "legs", DisplayName: "Legs"},
	{Name: "shoulders", DisplayName: "Shoulders"},
	{Name: "arms", DisplayName: "Arms"},
	{Name: "core", DisplayName: "Core"},
}
