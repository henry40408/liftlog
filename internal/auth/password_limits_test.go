package auth

import "testing"

func TestParseHashAcceptsCostsExactlyAtTheLimits(t *testing.T) {
	for _, cost := range []string{
		"m=1048576,t=2,p=1", "m=19456,t=64,p=1", "m=19456,t=2,p=16", "m=8,t=1,p=1", "m=16,t=1,p=2",
	} {
		if _, err := parseHash("$argon2id$v=19$" + cost + "$c2FsdHNhbHQ$aGFzaA"); err != nil {
			t.Errorf("%s: %v", cost, err)
		}
	}
}

func TestParseHashRejectsMemoryBelowEightKiBPerThread(t *testing.T) {
	if _, err := parseHash("$argon2id$v=19$m=15,t=1,p=2$c2FsdHNhbHQ$aGFzaA"); err == nil {
		t.Error("m=15 with 2 threads must be rejected")
	}
}
