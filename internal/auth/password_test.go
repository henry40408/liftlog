package auth

import (
	"runtime"
	"strings"
	"testing"
)

var testHasher = NewHasher()

func TestHashThenVerify(t *testing.T) {
	hash, err := testHasher.Hash("correctpass")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(hash, "$argon2id$v=19$m=19456,t=2,p=1$") {
		t.Fatalf("unexpected PHC string %q", hash)
	}
	for password, want := range map[string]bool{"correctpass": true, "wrongpass": false, "": false} {
		got, err := testHasher.Verify(password, hash)
		if err != nil || got != want {
			t.Errorf("testHasher.Verify(%q) = %v, %v; want %v", password, got, err, want)
		}
	}
}

func TestHashesCarryDistinctSalts(t *testing.T) {
	first, _ := testHasher.Hash("same")
	second, _ := testHasher.Hash("same")
	if first == second {
		t.Fatal("two hashes of one password are identical")
	}
}

func TestVerifyRejectsMalformedHashes(t *testing.T) {
	for _, bad := range []string{
		"", "plaintext", "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ", "$bcrypt$v=19$m=1,t=1,p=1$c2FsdHNhbHQ$aGFzaA",
		"$argon2id$v=18$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaA", "$argon2id$v=19$m=19456,t=0,p=1$c2FsdHNhbHQ$aGFzaA",
		"$argon2id$v=19$m=19456,t=2$c2FsdHNhbHQ$aGFzaA", "$argon2id$v=19$m=19456,t=2,p=0$c2FsdHNhbHQ$aGFzaA",
		"$argon2id$v=19$m=4,t=2,p=1$c2FsdHNhbHQ$aGFzaA",
		"$argon2id$v=19$m=1048577,t=2,p=1$c2FsdHNhbHQ$aGFzaA", "$argon2id$v=19$m=4294967295,t=2,p=1$c2FsdHNhbHQ$aGFzaA",
		"$argon2id$v=19$m=19456,t=65,p=1$c2FsdHNhbHQ$aGFzaA", "$argon2id$v=19$m=19456,t=2,p=17$c2FsdHNhbHQ$aGFzaA", "$argon2id$v=19$m=19456,t=2,p=1$!!$aGFzaA",
	} {
		if _, err := testHasher.Verify("x", bad); err == nil {
			t.Errorf("%q: no error", bad)
		}
	}
}

// rustArgon2Hashes were created by the Rust server (argon2 crate,
// Argon2::default()). The salts are random, so Go must verify them, not
// reproduce them.
var rustArgon2Hashes = []struct{ password, hash string }{
	{`correct horse battery staple`, `$argon2id$v=19$m=19456,t=2,p=1$3Qhfe0zjYC6IPFswNbdFIg$0oq1eeqpHP9w6nTTezSm2I6DqOgKk/PrVkrgpqTldeM`},
	{`x7#Kp2$mQ9vL`, `$argon2id$v=19$m=19456,t=2,p=1$EzIBHtMaJjSngJqWP1nwmw$8KRc26Clk+HCwelySc6ncYNnaVh1ubOsWBSPA9x8eI4`},
	{`密碼密碼密碼密碼密碼密碼`, `$argon2id$v=19$m=19456,t=2,p=1$rowo5UtLA6JHfb1AQ/Lwfw$/Yfqg+iq/wWFR3aJJjgazqIO4iZ1kCGvZ7Sdf7ZQHdk`},
	{`liftlog-unknown-user-placeholder`, `$argon2id$v=19$m=19456,t=2,p=1$txK7z/1hYMwwbO3E/0k/iQ$cTK7v2LXGUtEUhwz/374WjhGLvE2bZKDpwz6pV/NKnQ`},
	{`a`, `$argon2id$v=19$m=19456,t=2,p=1$bJyKI4boZMzgD5Wr/vA20Q$y8wxpH+kpkUD92KYxIPpj2YVL2RgEEVGwFqZuX9JfiU`},
}

// Hashes created by the Rust server must keep verifying, so existing
// databases survive the rewrite.
func TestVerifiesHashesMadeByRust(t *testing.T) {
	for _, tc := range rustArgon2Hashes {
		ok, err := testHasher.Verify(tc.password, tc.hash)
		if err != nil || !ok {
			t.Errorf("%q: Verify = %v, %v", tc.password, ok, err)
		}
		if ok, _ := testHasher.Verify(tc.password+"x", tc.hash); ok {
			t.Errorf("%q: a wrong password verified", tc.password)
		}
	}
}

// The timing defence needs the dummy hash to cost the same as a real one.
func TestDummyHashMatchesARealHashParameterForParameter(t *testing.T) {
	real, err := testHasher.Hash("anything")
	if err != nil {
		t.Fatal(err)
	}
	realParsed, err := parseHash(real)
	if err != nil {
		t.Fatal(err)
	}
	dummyParsed, err := parseHash(testHasher.DummyHash())
	if err != nil {
		t.Fatal(err)
	}
	if dummyParsed.memory != realParsed.memory || dummyParsed.time != realParsed.time ||
		dummyParsed.threads != realParsed.threads || len(dummyParsed.key) != len(realParsed.key) {
		t.Fatalf("dummy %+v differs from real %+v", dummyParsed, realParsed)
	}
}

func TestDummyHashCarriesItsOwnSalt(t *testing.T) {
	parsed, err := parseHash(testHasher.DummyHash())
	if err != nil {
		t.Fatal(err)
	}
	if len(parsed.salt) != saltBytes {
		t.Fatalf("salt length = %d", len(parsed.salt))
	}
	if ok, _ := testHasher.Verify(dummyPassword, testHasher.DummyHash()); !ok {
		t.Fatal("the dummy hash does not verify its own password")
	}
}

func TestHasherCapsConcurrentHashes(t *testing.T) {
	hasher := NewHasher()
	if got, want := cap(hasher.slots), runtime.GOMAXPROCS(0); got != want {
		t.Fatalf("slots = %d, want %d", got, want)
	}
}
