package auth

import (
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"errors"
	"fmt"
	"runtime"
	"strconv"
	"strings"
	"sync"

	"golang.org/x/crypto/argon2"
)

// Argon2id at m=19 MiB, t=2, p=1 with a 16-byte salt and 32-byte output: the
// parameters of the Rust server's Argon2::default(), so existing hashes verify.
// Verification reads the parameters from the stored string, never from here.
const (
	argon2Version = 19
	memoryKiB     = 19456
	timeCost      = 2
	parallelism   = 1
	saltBytes     = 16
	keyBytes      = 32
)

// Limits on stored costs, so a corrupted or hostile row cannot trigger a huge
// allocation or a very long computation.
const (
	maxMemoryKiB = 1 << 20 // 1 GiB
	maxTimeCost  = 64
	maxThreads   = 16
)

// dummyPassword is never a real credential; its hash only burns time for
// unknown usernames.
const dummyPassword = "liftlog-unknown-user-placeholder"

var errMalformedHash = errors.New("malformed password hash")

// Hasher runs Argon2id with a cap on concurrent computations: each one holds
// about 19 MiB, so unbounded login attempts could exhaust memory. Hold one in
// app state and share it.
type Hasher struct {
	slots chan struct{}

	dummyOnce sync.Once
	dummy     string
}

// NewHasher allows runtime.GOMAXPROCS(0) concurrent hashes.
func NewHasher() *Hasher {
	return &Hasher{slots: make(chan struct{}, runtime.GOMAXPROCS(0))}
}

func (h *Hasher) derive(password, salt []byte, params passwordHash) []byte {
	h.slots <- struct{}{}
	defer func() { <-h.slots }()
	return argon2.IDKey(password, salt, params.time, params.memory, params.threads, uint32(len(params.key)))
}

// Hash returns a PHC string, $argon2id$v=19$m=19456,t=2,p=1$salt$hash, with
// unpadded standard base64.
func (h *Hasher) Hash(password string) (string, error) {
	salt := make([]byte, saltBytes)
	if _, err := rand.Read(salt); err != nil {
		return "", fmt.Errorf("generate salt: %w", err)
	}
	params := passwordHash{memory: memoryKiB, time: timeCost, threads: parallelism, salt: salt, key: make([]byte, keyBytes)}
	params.key = h.derive([]byte(password), salt, params)
	return formatHash(params), nil
}

// Verify checks password against a stored PHC string. The error is for a hash
// that cannot be parsed, not for a wrong password.
func (h *Hasher) Verify(password, encoded string) (bool, error) {
	hash, err := parseHash(encoded)
	if err != nil {
		return false, err
	}
	got := h.derive([]byte(password), hash.salt, hash)
	return subtle.ConstantTimeCompare(got, hash.key) == 1, nil
}

// DummyHash is a valid hash of dummyPassword, for verifying against when the
// username is unknown, so timing does not reveal which usernames exist.
//
// Computed at first use so it tracks Hash's parameters, and so each
// deployment gets its own salt.
func (h *Hasher) DummyHash() string {
	h.dummyOnce.Do(func() {
		hash, err := h.Hash(dummyPassword)
		if err != nil {
			panic(fmt.Sprintf("hash the dummy password: %v", err))
		}
		h.dummy = hash
	})
	return h.dummy
}

type passwordHash struct {
	memory  uint32
	time    uint32
	threads uint8
	salt    []byte
	key     []byte
}

func formatHash(hash passwordHash) string {
	return fmt.Sprintf("$argon2id$v=%d$m=%d,t=%d,p=%d$%s$%s",
		argon2Version, hash.memory, hash.time, hash.threads,
		base64.RawStdEncoding.EncodeToString(hash.salt),
		base64.RawStdEncoding.EncodeToString(hash.key))
}

// parseHash reads $argon2id$v=19$m=..,t=..,p=..$salt$hash.
func parseHash(encoded string) (passwordHash, error) {
	fields := strings.Split(encoded, "$")
	if len(fields) != 6 || fields[0] != "" || fields[1] != "argon2id" || fields[2] != "v="+strconv.Itoa(argon2Version) {
		return passwordHash{}, errMalformedHash
	}
	hash, err := parseCost(fields[3])
	if err != nil {
		return passwordHash{}, err
	}
	hash.salt, err = base64.RawStdEncoding.DecodeString(fields[4])
	if err != nil || len(hash.salt) == 0 {
		return passwordHash{}, errMalformedHash
	}
	hash.key, err = base64.RawStdEncoding.DecodeString(fields[5])
	if err != nil || len(hash.key) == 0 {
		return passwordHash{}, errMalformedHash
	}
	return hash, nil
}

func parseCost(raw string) (passwordHash, error) {
	var hash passwordHash
	var seen int
	for pair := range strings.SplitSeq(raw, ",") {
		name, value, ok := strings.Cut(pair, "=")
		if !ok {
			return passwordHash{}, errMalformedHash
		}
		number, err := strconv.ParseUint(value, 10, 32)
		if err != nil {
			return passwordHash{}, errMalformedHash
		}
		switch name {
		case "m":
			hash.memory = uint32(number)
		case "t":
			hash.time = uint32(number)
		case "p":
			if number > 255 {
				return passwordHash{}, errMalformedHash
			}
			hash.threads = uint8(number)
		default:
			return passwordHash{}, errMalformedHash
		}
		seen++
	}
	// IDKey panics on zero costs; a hash that needs them is malformed anyway.
	if hash.memory > maxMemoryKiB || hash.time > maxTimeCost || hash.threads > maxThreads {
		return passwordHash{}, errMalformedHash
	}
	if seen != 3 || hash.time == 0 || hash.threads == 0 || hash.memory < 8*uint32(hash.threads) {
		return passwordHash{}, errMalformedHash
	}
	return hash, nil
}
