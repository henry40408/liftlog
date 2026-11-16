package auth

import (
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"strings"
	"sync"
	"unicode"
)

// tokenAlphabet is Crockford base32: no I, L, O or U, so a token read off a
// terminal is hard to mistype.
const tokenAlphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"

const tokenGroupLen = 5

// GenerateSetupToken returns a fresh token, XXXXX-XXXXX (50 bits).
//
// Short on purpose: it is typed by hand, guesses are online only and
// rate-limited per IP, and it dies with the first account.
func GenerateSetupToken() string {
	raw := make([]byte, tokenGroupLen*2)
	// rand.Read never fails since Go 1.24.
	_, _ = rand.Read(raw)
	symbols := make([]byte, len(raw))
	for i, b := range raw {
		// 32 symbols: the low 5 bits index the alphabet with no modulo bias.
		symbols[i] = tokenAlphabet[b&0x1f]
	}
	return string(symbols[:tokenGroupLen]) + "-" + string(symbols[tokenGroupLen:])
}

// normalizeToken ignores the dash (only there for readability), whitespace and
// case.
func normalizeToken(token string) string {
	var b strings.Builder
	for _, r := range token {
		if r == '-' || unicode.IsSpace(r) {
			continue
		}
		b.WriteString(strings.ToUpper(string(r)))
	}
	return b.String()
}

// digestsMatch compares digests, so neither the token's length nor its
// matching prefix leaks through timing.
func digestsMatch(expected, submitted string) bool {
	want := sha256.Sum256([]byte(normalizeToken(expected)))
	got := sha256.Sum256([]byte(normalizeToken(submitted)))
	return subtle.ConstantTimeCompare(want[:], got[:]) == 1
}

// SetupToken gates first-run account creation. Without it, whoever reaches a
// fresh install first becomes the admin. It lives in memory and dies with the
// first account. The zero value holds no token.
type SetupToken struct {
	mu      sync.Mutex
	current string
	active  bool
}

// NewSetupToken issues a token only while the install has no users.
func NewSetupToken(userCount int64) *SetupToken {
	if userCount > 0 {
		return &SetupToken{}
	}
	return NewSetupTokenWithValue(GenerateSetupToken())
}

// NewSetupTokenWithValue holds a fixed token, so tests do not depend on the
// random one.
func NewSetupTokenWithValue(token string) *SetupToken {
	return &SetupToken{current: token, active: true}
}

// Current is the token to show the operator; ok is false when none is active.
func (t *SetupToken) Current() (token string, ok bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.current, t.active
}

// Verify is false when no token is active, so a finished install never matches.
func (t *SetupToken) Verify(submitted string) bool {
	expected, ok := t.Current()
	if !ok {
		return false
	}
	return digestsMatch(expected, submitted)
}

func (t *SetupToken) Clear() {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.current, t.active = "", false
}
