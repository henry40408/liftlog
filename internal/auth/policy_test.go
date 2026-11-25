package auth

import (
	"strings"
	"testing"
)

// strong clears the score gate, so the other gate is what fires.
const strong = "deadlift squats bench"

func TestPolicyRejectsTooShort(t *testing.T) {
	got := PasswordPolicyError(strings.Repeat("a", MinPasswordLen-1), "Password", nil)
	if !strings.Contains(got, "at least 12 characters") {
		t.Fatalf("got %q", got)
	}
}

func TestPolicyRejectsTooLong(t *testing.T) {
	long := strings.Repeat(strong, 20)
	got := PasswordPolicyError(long, "Password", nil)
	if !strings.Contains(got, "at most 128 characters") {
		t.Fatalf("got %q", got)
	}
}

func TestPolicyReportsLengthBeforeStrength(t *testing.T) {
	got := PasswordPolicyError(strings.Repeat("a", MaxPasswordLen+1), "Password", nil)
	if !strings.Contains(got, "at most 128 characters") {
		t.Fatalf("the length ceiling should be reported, not the strength verdict: %q", got)
	}
}

func TestPolicyAcceptsAStrongPassword(t *testing.T) {
	if got := PasswordPolicyError(strong, "Password", nil); got != "" {
		t.Fatalf("got %q", got)
	}
}

func TestPolicyAcceptsTheMinimumLengthWhenStrong(t *testing.T) {
	if got := PasswordPolicyError("gymrat.2026!", "Password", nil); got != "" {
		t.Fatalf("got %q", got)
	}
}

// MyPassword12 passes any composition rule, yet is trivially guessable.
func TestPolicyRejectsALongEnoughButGuessablePassword(t *testing.T) {
	got := PasswordPolicyError("MyPassword12", "Password", nil)
	if got == "" || strings.Contains(got, "characters") {
		t.Fatalf("want a strength message, got %q", got)
	}
}

func TestPolicyPenalisesPasswordsDerivedFromTheUsername(t *testing.T) {
	const derived = "henrylifts.42x"
	if got := PasswordPolicyError(derived, "Password", nil); got != "" {
		t.Fatalf("precondition: this clears the bar when the username is unknown, got %q", got)
	}
	if PasswordPolicyError(derived, "Password", []string{"henrylifts"}) == "" {
		t.Fatal("the same password must be rejected once the username is supplied")
	}
}

// The characters differ, not repeat: zxcvbn would reject a repeat on strength
// and prove nothing about length.
func TestPolicyCountsCharactersNotBytes(t *testing.T) {
	const twelveCJK = "密碼鎖健身房訓練紀錄應用"
	if got := PasswordPolicyError(twelveCJK, "Password", nil); got != "" {
		t.Fatalf("12 characters rejected: %q", got)
	}
	const elevenCJK = "健身訓練紀錄鎖密碼安全"
	if got := PasswordPolicyError(elevenCJK, "Password", nil); !strings.Contains(got, "at least 12 characters") {
		t.Fatalf("11 characters: got %q", got)
	}
}

func TestPolicyUsesTheSuppliedLabelForLengthErrors(t *testing.T) {
	if got := PasswordPolicyError("short", "New password", nil); !strings.HasPrefix(got, "New password") {
		t.Fatalf("got %q", got)
	}
}

func TestPolicySurfacesActionableFeedback(t *testing.T) {
	got := PasswordPolicyError("password1234", "Password", nil)
	if len(got) <= 20 {
		t.Fatalf("expected zxcvbn's prose feedback, got %q", got)
	}
	for _, leaked := range []string{"guesses", "score", "10^", "seconds"} {
		if strings.Contains(strings.ToLower(got), leaked) {
			t.Fatalf("estimate detail leaked into the user-facing message: %q", got)
		}
	}
}

func TestPolicyRejectsInvalidUTF8AsWeak(t *testing.T) {
	if PasswordPolicyError(strings.Repeat("\xff", 20), "Password", nil) == "" {
		t.Fatal("invalid UTF-8 accepted")
	}
}

// knownWordingDivergence lists passwords both libraries reject, with the Go
// warning. "123456789" costs the same as a top-10 password and as a
// sequence. Rust breaks that tie by HashMap and unstable-sort order, so its
// warning is not stable: run to run it flips for "1234567892024!". The Go
// fork keeps the first match found, the dictionary one. The verdict does not
// depend on the tie.
var knownWordingDivergence = map[string]string{
	"1234567892024!": "This is similar to a commonly used password. Add another word or two. Uncommon words are better.",
	"123456789012":   "This is similar to a commonly used password. Add another word or two. Uncommon words are better.",
}

// Rust zxcvbn 3.1.1 scores each of these 0-2. A byte-indexed zxcvbn misses
// the repeats and sequences and scores them 3-4.
func TestPolicyRejectsWeakNonASCIIPasswords(t *testing.T) {
	for _, password := range []string{
		strings.Repeat("密碼", 6),
		strings.Repeat("密碼", 6) + "1",
		"😀😀😀😀😀😀😀😀😀😀😀😀",
		strings.Repeat("é", 12),
		"éééééééééééé1",
		"あいうえおかきくけこさし",
		"абвгдежзийкл",
		"парольпароль",
		// Only the first 100 characters are scored.
		strings.Repeat("é", 127) + "x",
	} {
		if PasswordPolicyError(password, "Password", nil) == "" {
			t.Errorf("%q accepted", password)
		}
	}
}

func TestPolicyAcceptsAStrongRepeatedNonASCIIBlock(t *testing.T) {
	if got := PasswordPolicyError("健身房訓練紀錄應用密碼鎖健身房訓練紀錄應用密碼鎖", "Password", nil); got != "" {
		t.Errorf("strong repeated block rejected: %q", got)
	}
}

// A byte-indexed zxcvbn scored random CJK and Cyrillic as 0. Rust accepts
// all of these.
func TestPolicyAcceptsRandomNonASCIIPasswords(t *testing.T) {
	for _, password := range []string{
		"鋒蔣窄濤蠟慘囑滬駐誼壹溝",
		"肖濱讒揮疆彌兆凳楚粹轍鎢",
		"дтэщшвигпшоп",
		"фмщжгпаюымну",
		"ψζλκθφωνξμβη",
		"ぬぽやゑげぢゆぺぎぶむぉ",
		"ÿçøñßüæœàéîõ",
		"🎃🌵😎🍇🎲🌋😴🍩🎻🌀😬🍕",
		"zé密рψ😀kô藍дλぬ",
	} {
		if got := PasswordPolicyError(password, "Password", nil); got != "" {
			t.Errorf("%q rejected: %q", password, got)
		}
	}
}

// Rust zxcvbn 3.1.1 produced rustPolicyCases; the Go port must give the same
// verdict and the same words for every case.
func TestPolicyMatchesTheRustCorpus(t *testing.T) {
	// A fixed count, so a case dropped from the table cannot go unnoticed.
	const wantCases = 506
	if len(rustPolicyCases) != wantCases {
		t.Fatalf("%d cases, want %d", len(rustPolicyCases), wantCases)
	}
	for _, tc := range rustPolicyCases {
		got := PasswordPolicyError(tc.password, "Password", []string{tc.username})
		if divergent, ok := knownWordingDivergence[tc.password]; ok && got == divergent {
			continue
		}
		wantAccepted := tc.message == accepted
		if (got == "") != wantAccepted {
			t.Errorf("%q (user %q): accepted = %v, Rust said %v; message %q",
				tc.password, tc.username, got == "", wantAccepted, got)
			continue
		}
		if got != tc.message {
			t.Errorf("%q (user %q):\n got  %q\n want %q", tc.password, tc.username, got, tc.message)
		}
	}
}
