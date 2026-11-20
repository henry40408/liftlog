package config

import (
	"net/netip"
	"reflect"
	"strings"
	"testing"
)

func fakeEnv(vars map[string]string) LookupEnv {
	return func(name string) (string, bool) {
		value, ok := vars[name]
		return value, ok
	}
}

func wantErrContaining(t *testing.T, err error, substr string) {
	t.Helper()
	if err == nil || !strings.Contains(err.Error(), substr) {
		t.Fatalf("got error %v, want one containing %q", err, substr)
	}
}

func TestParseBindDefaultsWhenEmpty(t *testing.T) {
	got, err := ParseBind("")
	if err != nil || got != netip.MustParseAddrPort("127.0.0.1:8080") {
		t.Fatalf("got %v, %v", got, err)
	}
}

func TestParseBindAcceptsValidSocketAddr(t *testing.T) {
	for _, raw := range []string{"127.0.0.1:9000", "0.0.0.0:8080", "[::1]:80"} {
		got, err := ParseBind(raw)
		if err != nil || got != netip.MustParseAddrPort(raw) {
			t.Errorf("%q: got %v, %v", raw, got, err)
		}
	}
}

func TestParseBindRejectsInvalid(t *testing.T) {
	_, err := ParseBind("not-an-addr")
	wantErrContaining(t, err, "invalid LIFTLOG_BIND")
	if _, err := ParseBind("127.0.0.1"); err == nil {
		t.Error("a bind without a port must be rejected")
	}
}

func TestFromEnvReadsBind(t *testing.T) {
	cfg, err := FromEnv(fakeEnv(map[string]string{"LIFTLOG_BIND": "127.0.0.1:9137"}))
	if err != nil || cfg.Bind != netip.MustParseAddrPort("127.0.0.1:9137") {
		t.Fatalf("got %v, %v", cfg.Bind, err)
	}
}

func TestFromEnvDefaults(t *testing.T) {
	cfg, err := FromEnv(fakeEnv(nil))
	if err != nil {
		t.Fatal(err)
	}
	if cfg.DatabaseURL != "sqlite:liftlog.sqlite3?mode=rwc" {
		t.Errorf("DatabaseURL = %q", cfg.DatabaseURL)
	}
	if cfg.TrustedProxyHeader != TrustNoHeader || len(cfg.TrustedProxies) != 0 ||
		cfg.CookieSecure || cfg.HSTSMaxAge != 0 || cfg.HSTSIncludeSubdomains {
		t.Errorf("unexpected non-default: %+v", cfg)
	}
}

func TestFromEnvReadsEverySetting(t *testing.T) {
	cfg, err := FromEnv(fakeEnv(map[string]string{
		"DATABASE_URL":                    "sqlite:/data/x.sqlite3",
		"LIFTLOG_TRUSTED_PROXY_HEADER":    "x-real-ip",
		"LIFTLOG_TRUSTED_PROXIES":         "10.0.0.1",
		"LIFTLOG_COOKIE_SECURE":           "true",
		"LIFTLOG_HSTS_MAX_AGE":            "60",
		"LIFTLOG_HSTS_INCLUDE_SUBDOMAINS": "1",
	}))
	if err != nil {
		t.Fatal(err)
	}
	want := Config{
		DatabaseURL:           "sqlite:/data/x.sqlite3",
		Bind:                  netip.MustParseAddrPort("127.0.0.1:8080"),
		TrustedProxyHeader:    TrustXRealIP,
		TrustedProxies:        []netip.Addr{netip.MustParseAddr("10.0.0.1")},
		CookieSecure:          true,
		HSTSMaxAge:            60,
		HSTSIncludeSubdomains: true,
	}
	if !reflect.DeepEqual(cfg, want) {
		t.Errorf("got %+v, want %+v", cfg, want)
	}
}

func TestFromEnvRejectsLegacyVars(t *testing.T) {
	cases := map[string]string{
		"BIND":       "LIFTLOG_BIND",
		"LOG_FORMAT": "LIFTLOG_LOG_FORMAT",
		"RUST_LOG":   "LIFTLOG_LOG_LEVEL",
	}
	for old, replacement := range cases {
		env := fakeEnv(map[string]string{old: "x"})
		_, err := FromEnv(env)
		wantErrContaining(t, err, old)
		wantErrContaining(t, err, replacement)
	}
}

func TestRejectLegacyEnvVarsPassesWhenClean(t *testing.T) {
	if err := RejectLegacyEnvVars(fakeEnv(nil)); err != nil {
		t.Fatal(err)
	}
}

func TestRejectLegacyEnvVarsCountsAnEmptyValue(t *testing.T) {
	err := RejectLegacyEnvVars(fakeEnv(map[string]string{"BIND": ""}))
	wantErrContaining(t, err, "BIND")
}

func TestParseTrustedProxiesDefaultsEmpty(t *testing.T) {
	for _, raw := range []string{"", "   "} {
		got, err := ParseTrustedProxies(raw)
		if err != nil || len(got) != 0 {
			t.Errorf("%q: got %v, %v", raw, got, err)
		}
	}
}

func TestParseTrustedProxiesAcceptsCommaSeparatedIPs(t *testing.T) {
	got, err := ParseTrustedProxies(" 10.0.0.1 , ::1, 192.168.1.1 , ")
	if err != nil {
		t.Fatal(err)
	}
	want := []netip.Addr{
		netip.MustParseAddr("10.0.0.1"),
		netip.MustParseAddr("::1"),
		netip.MustParseAddr("192.168.1.1"),
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestParseTrustedProxiesRejectsGarbage(t *testing.T) {
	_, err := ParseTrustedProxies("10.0.0.1, not-an-ip")
	wantErrContaining(t, err, "invalid LIFTLOG_TRUSTED_PROXIES")
}

func TestParseTrustedProxiesCanonicalizesMappedIPv6(t *testing.T) {
	got, err := ParseTrustedProxies("::ffff:10.0.0.5")
	if err != nil || len(got) != 1 || got[0] != netip.MustParseAddr("10.0.0.5") {
		t.Fatalf("got %v, %v", got, err)
	}
}

func TestParseTrustedProxyHeaderDefaultsToNone(t *testing.T) {
	for _, raw := range []string{"", "   "} {
		got, err := ParseTrustedProxyHeader(raw)
		if err != nil || got != TrustNoHeader {
			t.Errorf("%q: got %v, %v", raw, got, err)
		}
	}
}

func TestParseTrustedProxyHeaderAcceptsKnownValuesCaseInsensitively(t *testing.T) {
	cases := map[string]TrustedProxyHeader{
		"X-Forwarded-For":     TrustXForwardedFor,
		"x-real-ip":           TrustXRealIP,
		"NONE":                TrustNoHeader,
		"  x-forwarded-for  ": TrustXForwardedFor,
	}
	for raw, want := range cases {
		got, err := ParseTrustedProxyHeader(raw)
		if err != nil || got != want {
			t.Errorf("%q: got %v, %v", raw, got, err)
		}
	}
}

func TestParseTrustedProxyHeaderRejectsUnknown(t *testing.T) {
	_, err := ParseTrustedProxyHeader("x-forwarded")
	wantErrContaining(t, err, "invalid LIFTLOG_TRUSTED_PROXY_HEADER")
}

func TestParseBoolEnvIsFalseWhenAbsentOrBlank(t *testing.T) {
	for _, raw := range []string{"", "   "} {
		got, err := ParseBoolEnv("LIFTLOG_COOKIE_SECURE", raw)
		if err != nil || got {
			t.Errorf("%q: got %v, %v", raw, got, err)
		}
	}
}

func TestParseBoolEnvAcceptsTrueFalse10CaseInsensitively(t *testing.T) {
	for _, raw := range []string{"true", "TRUE", "  true  ", "1"} {
		if got, err := ParseBoolEnv("X", raw); err != nil || !got {
			t.Errorf("%q: got %v, %v", raw, got, err)
		}
	}
	for _, raw := range []string{"false", "False", "  false  ", "0"} {
		if got, err := ParseBoolEnv("X", raw); err != nil || got {
			t.Errorf("%q: got %v, %v", raw, got, err)
		}
	}
}

func TestParseBoolEnvRejectsUnrecognisedValue(t *testing.T) {
	_, err := ParseBoolEnv("LIFTLOG_COOKIE_SECURE", "yes")
	wantErrContaining(t, err, "invalid LIFTLOG_COOKIE_SECURE")
}

func TestFromEnvRejectsNonUTF8Value(t *testing.T) {
	env := fakeEnv(map[string]string{"LIFTLOG_COOKIE_SECURE": "fo\x80o"})
	_, err := FromEnv(env)
	wantErrContaining(t, err, "LIFTLOG_COOKIE_SECURE")
}

func TestParseHSTSMaxAgeDefaultsToZero(t *testing.T) {
	for _, raw := range []string{"", "   "} {
		if got, err := ParseHSTSMaxAge(raw); err != nil || got != 0 {
			t.Errorf("%q: got %v, %v", raw, got, err)
		}
	}
}

func TestParseHSTSMaxAgeAcceptsSeconds(t *testing.T) {
	if got, err := ParseHSTSMaxAge("31536000"); err != nil || got != 31_536_000 {
		t.Errorf("got %v, %v", got, err)
	}
	if got, err := ParseHSTSMaxAge(" 0 "); err != nil || got != 0 {
		t.Errorf("got %v, %v", got, err)
	}
}

func TestParseHSTSMaxAgeRejectsGarbage(t *testing.T) {
	for _, raw := range []string{"not-a-number", "-1"} {
		_, err := ParseHSTSMaxAge(raw)
		wantErrContaining(t, err, "invalid LIFTLOG_HSTS_MAX_AGE")
	}
}
