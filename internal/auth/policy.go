package auth

import (
	"fmt"
	"unicode/utf8"

	"github.com/henry40408/liftlog/internal/zxcvbn"
)

// MinPasswordLen is 12 rather than NIST's 15-without-MFA: the zxcvbn floor
// does the blocklist work, and together they reject more than 15 alone would
// (123456789012345 is 15 characters). Enforced server-side; the form's
// minlength is only a convenience.
const MinPasswordLen = 12

// MaxPasswordLen bounds Argon2 input against denial of service while fitting
// passphrases (OWASP: at least 64). Over-long passwords are rejected, never
// truncated.
const MaxPasswordLen = 128

// minPasswordScore is the zxcvbn score (0-4). 3 is "safely unguessable"
// against an offline attack on a slow hash; 4 mostly turns away real users.
const minPasswordScore = 3

// PasswordPolicyError is the user-facing rejection message, or "" if the
// password is acceptable. label names the field, since the settings form has
// three password inputs.
//
//  1. Length, in characters (not bytes, so non-Latin scripts are not
//     penalised). Checked first so over-long input never reaches zxcvbn.
//  2. zxcvbn score of at least 3: this blocks common passwords, not breached
//     ones (there is no breach lookup).
//
// userInputs (the username) are penalised as attacker-known. Only zxcvbn's
// prose feedback is shown, never its guess or crack-time estimates.
func PasswordPolicyError(password, label string, userInputs []string) string {
	length := utf8.RuneCountInString(password)
	if length < MinPasswordLen {
		return fmt.Sprintf("%s must be at least %d characters", label, MinPasswordLen)
	}
	if length > MaxPasswordLen {
		return fmt.Sprintf("%s must be at most %d characters", label, MaxPasswordLen)
	}

	result := zxcvbn.PasswordStrength(password, userInputs)
	if result.Score >= minPasswordScore {
		return ""
	}
	feedback := feedbackFor(result.Sequence)
	warning := feedback.Warning
	if warning == "" {
		warning = label + " is too easy to guess."
	}
	if len(feedback.Suggestions) == 0 {
		return warning
	}
	return warning + " " + feedback.Suggestions[0]
}
