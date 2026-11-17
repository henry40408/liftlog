package auth

import (
	"strings"
	"testing"
)

func TestUsernameAcceptsOrdinaryNames(t *testing.T) {
	for _, name := range []string{"henry", "Lifter 王小明", strings.Repeat("a", MaxUsernameLen)} {
		if got := UsernameError(name); got != "" {
			t.Errorf("%q rejected: %s", name, got)
		}
	}
}

func TestUsernameRejectsEmptyAndBlank(t *testing.T) {
	for _, blank := range []string{"", "   ", "　"} {
		if got := UsernameError(blank); !strings.Contains(got, "required") {
			t.Errorf("%q: got %q", blank, got)
		}
	}
}

func TestUsernameIsJudgedAfterTrimming(t *testing.T) {
	for _, name := range []string{"  henry  ", "  " + strings.Repeat("a", MaxUsernameLen) + "  "} {
		if got := UsernameError(name); got != "" {
			t.Errorf("%q rejected: %s", name, got)
		}
	}
}

func TestUsernameRejectsTooLongCountingCharacters(t *testing.T) {
	if got := UsernameError(strings.Repeat("a", MaxUsernameLen+1)); !strings.Contains(got, "at most 64") {
		t.Errorf("65 chars: got %q", got)
	}
	// 64 multi-byte characters are over 64 bytes but still allowed.
	if got := UsernameError(strings.Repeat("王", MaxUsernameLen)); got != "" {
		t.Errorf("64 CJK chars rejected: %s", got)
	}
}

func TestUsernameRejectsControlCharacters(t *testing.T) {
	for _, name := range []string{"a\nb", "a\tb", "a\x00b", "a\u007fb", "a\u0085b"} {
		if got := UsernameError(name); !strings.Contains(got, "control") {
			t.Errorf("%q: got %q", name, got)
		}
	}
}
