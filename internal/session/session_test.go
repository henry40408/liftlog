package session

import (
	"net/http"
	"net/http/httptest"
	"regexp"
	"strings"
	"testing"
	"time"
)

var now = time.Date(2026, 6, 1, 12, 0, 0, 0, time.UTC)

func TestNewCookieSetsSecureWhenEnabled(t *testing.T) {
	if !NewCookie("tok", true).Secure {
		t.Error("Secure must be set")
	}
}

func TestNewCookieOmitsSecureWhenDisabled(t *testing.T) {
	if strings.Contains(NewCookie("tok", false).String(), "Secure") {
		t.Error("Secure must be omitted")
	}
}

func TestNewCookieAttributes(t *testing.T) {
	got := NewCookie("tok", false).String()
	want := "session=tok; Path=/; Max-Age=604800; HttpOnly; SameSite=Lax"
	if got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestRemovalCookieMatchesCreateAttributes(t *testing.T) {
	for _, secure := range []bool{true, false} {
		created := NewCookie("tok", secure)
		removed := RemovalCookie(secure)

		if removed.Secure != created.Secure || removed.HttpOnly != created.HttpOnly ||
			removed.Path != created.Path || removed.SameSite != created.SameSite ||
			removed.Name != created.Name {
			t.Errorf("secure=%v: created %+v, removed %+v", secure, created, removed)
		}
		if !strings.Contains(removed.String(), "Max-Age=0") {
			t.Errorf("secure=%v: removal must expire the cookie: %s", secure, removed)
		}
	}
}

func TestCookieNameUsesHostPrefixOnlyWhenSecure(t *testing.T) {
	if got := CookieName(true); got != "__Host-session" {
		t.Errorf("got %q", got)
	}
	if got := CookieName(false); got != "session" {
		t.Errorf("got %q", got)
	}
}

func TestSecureCookieSatisfiesHostPrefixRequirements(t *testing.T) {
	cookie := NewCookie("tok", true)
	if !strings.HasPrefix(cookie.Name, "__Host-") || !cookie.Secure || cookie.Path != "/" || cookie.Domain != "" {
		t.Errorf("not a valid __Host- cookie: %+v", cookie)
	}
}

func TestTokenFromRequestReadsOnlyTheMatchingName(t *testing.T) {
	r := httptest.NewRequest(http.MethodGet, "/", nil)
	r.AddCookie(&http.Cookie{Name: "session", Value: "plain"})

	if token, ok := TokenFromRequest(r, false); !ok || token != "plain" {
		t.Errorf("plain: got %q, %v", token, ok)
	}
	if _, ok := TokenFromRequest(r, true); ok {
		t.Error("a bare session cookie must not satisfy a secure deployment")
	}
}

func TestAbsoluteCapIsCreatedAtPlus90Days(t *testing.T) {
	if got, want := AbsoluteCap(now), now.AddDate(0, 0, 90); !got.Equal(want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestComputeTouchActionNothingInsideThrottleWindow(t *testing.T) {
	got := ComputeTouchAction(SessionTimes{
		CreatedAt:     now.Add(-24 * time.Hour),
		LastTouchedAt: now.Add(-5 * time.Minute),
		ExpiresAt:     now.Add(6 * 24 * time.Hour),
	}, now)
	if got.Kind != TouchNothing {
		t.Errorf("got %+v", got)
	}
}

func TestComputeTouchActionThrottleBoundaryIsInclusive(t *testing.T) {
	times := SessionTimes{
		CreatedAt:     now.Add(-24 * time.Hour),
		LastTouchedAt: now.Add(-TouchThrottle),
		ExpiresAt:     now.Add(6 * 24 * time.Hour),
	}
	if got := ComputeTouchAction(times, now); got.Kind != TouchNothing {
		t.Errorf("exactly at the throttle: got %+v", got)
	}
	times.LastTouchedAt = times.LastTouchedAt.Add(-time.Nanosecond)
	if got := ComputeTouchAction(times, now); got.Kind != TouchSlide {
		t.Errorf("just past the throttle: got %+v", got)
	}
}

func TestComputeTouchActionSlidesFullIdleTTLForYoungSession(t *testing.T) {
	got := ComputeTouchAction(SessionTimes{
		CreatedAt:     now.Add(-24 * time.Hour),
		LastTouchedAt: now.Add(-2 * time.Hour),
		ExpiresAt:     now.Add(6 * 24 * time.Hour),
	}, now)
	if got.Kind != TouchSlide || !got.ExpiresAt.Equal(now.Add(IdleTTL)) {
		t.Errorf("got %+v", got)
	}
}

func TestComputeTouchActionClampsToCapNearTheLimit(t *testing.T) {
	createdAt := now.Add(-89 * 24 * time.Hour)
	// Below the cap (now + 1d), so this hits the clamp, not TouchOnly.
	got := ComputeTouchAction(SessionTimes{
		CreatedAt:     createdAt,
		LastTouchedAt: now.Add(-2 * time.Hour),
		ExpiresAt:     now.Add(3 * time.Hour),
	}, now)
	if got.Kind != TouchSlide || !got.ExpiresAt.Equal(AbsoluteCap(createdAt)) {
		t.Errorf("got %+v, want slide to %v", got, AbsoluteCap(createdAt))
	}
}

func TestComputeTouchActionTouchOnlyWhenPinnedAtCap(t *testing.T) {
	createdAt := now.Add(-89 * 24 * time.Hour)
	got := ComputeTouchAction(SessionTimes{
		CreatedAt:     createdAt,
		LastTouchedAt: now.Add(-2 * time.Hour),
		ExpiresAt:     AbsoluteCap(createdAt),
	}, now)
	if got.Kind != TouchOnly {
		t.Errorf("got %+v", got)
	}
}

func TestComputeTouchActionNothingInsideThrottleEvenWhenPinnedAtCap(t *testing.T) {
	createdAt := now.Add(-89 * 24 * time.Hour)
	got := ComputeTouchAction(SessionTimes{
		CreatedAt:     createdAt,
		LastTouchedAt: now.Add(-5 * time.Minute),
		ExpiresAt:     AbsoluteCap(createdAt),
	}, now)
	if got.Kind != TouchNothing {
		t.Errorf("got %+v", got)
	}
}

const sampleToken = "b6b1c1f4-6e1a-4e2a-9c2d-7f1a6d2e3b4c"

func repeated(b byte) []byte { return []byte(strings.Repeat(string([]byte{b}), 32)) }

func TestTokenFingerprintIsDeterministicForSameSaltAndToken(t *testing.T) {
	first := TokenFingerprint(sampleToken, repeated(1))
	second := TokenFingerprint(sampleToken, repeated(1))
	if first != second {
		t.Error("fingerprint must be deterministic")
	}
}

func TestTokenFingerprintDiffersForDifferentSalt(t *testing.T) {
	if TokenFingerprint(sampleToken, repeated(1)) == TokenFingerprint(sampleToken, repeated(2)) {
		t.Error("salt must change the fingerprint")
	}
}

func TestTokenFingerprintDiffersForDifferentToken(t *testing.T) {
	salt := repeated(1)
	if TokenFingerprint(sampleToken, salt) == TokenFingerprint("a1a1a1a1-1111-2222-3333-444455556666", salt) {
		t.Error("token must change the fingerprint")
	}
}

func TestTokenFingerprintNeverContainsTheRawToken(t *testing.T) {
	got := TokenFingerprint(sampleToken, repeated(7))
	if strings.Contains(got, sampleToken) || !regexp.MustCompile(`^[0-9a-f]{16}$`).MatchString(got) {
		t.Errorf("got %q", got)
	}
}
