package auth

import (
	"fmt"
	"strings"
	"unicode"
	"unicode/utf8"
)

const MaxUsernameLen = 64

// UsernameError is the validation message, or "" if username is acceptable.
//
// It judges the trimmed value, which is what gets stored. Length is counted in
// characters, not bytes.
func UsernameError(username string) string {
	username = strings.TrimSpace(username)
	if username == "" {
		return "Username is required"
	}
	if utf8.RuneCountInString(username) > MaxUsernameLen {
		return fmt.Sprintf("Username must be at most %d characters", MaxUsernameLen)
	}
	if strings.ContainsFunc(username, unicode.IsControl) {
		return "Username must not contain control characters"
	}
	return ""
}
