package db

import (
	"regexp"
	"testing"
)

var uuidV4 = regexp.MustCompile(`^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$`)

func TestNewIDIsAVersion4UUID(t *testing.T) {
	for range 100 {
		if id := NewID(); !uuidV4.MatchString(id) {
			t.Fatalf("not a v4 UUID: %q", id)
		}
	}
}

func TestNewIDIsUnique(t *testing.T) {
	seen := map[string]bool{}
	for range 1000 {
		id := NewID()
		if seen[id] {
			t.Fatalf("duplicate id %q", id)
		}
		seen[id] = true
	}
}
