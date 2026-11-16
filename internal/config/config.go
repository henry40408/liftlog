// Package config reads the server settings from environment variables.
package config

import (
	"fmt"
	"net/netip"
	"strconv"
	"strings"
	"unicode/utf8"
)

// TrustedProxyHeader says which proxy-supplied header, if any, may be trusted
// to carry the real client IP.
type TrustedProxyHeader int

const (
	// TrustNoHeader always uses the TCP peer address.
	TrustNoHeader TrustedProxyHeader = iota
	TrustXForwardedFor
	TrustXRealIP
)

type Config struct {
	DatabaseURL           string
	Bind                  netip.AddrPort
	TrustedProxyHeader    TrustedProxyHeader
	TrustedProxies        []netip.Addr
	CookieSecure          bool
	HSTSMaxAge            uint64
	HSTSIncludeSubdomains bool
}

// LookupEnv has the shape of os.LookupEnv, so tests can inject a fake.
type LookupEnv func(name string) (string, bool)

const defaultDatabaseURL = "sqlite:liftlog.sqlite3?mode=rwc"

var defaultBind = netip.AddrPortFrom(netip.AddrFrom4([4]byte{127, 0, 0, 1}), 8080)

// settingNames are the variables FromEnv reads.
var settingNames = []string{
	"DATABASE_URL",
	"LIFTLOG_BIND",
	"LIFTLOG_TRUSTED_PROXY_HEADER",
	"LIFTLOG_TRUSTED_PROXIES",
	"LIFTLOG_COOKIE_SECURE",
	"LIFTLOG_HSTS_MAX_AGE",
	"LIFTLOG_HSTS_INCLUDE_SUBDOMAINS",
}

// FromEnv builds the Config; any invalid or retired variable is an error.
func FromEnv(lookup LookupEnv) (Config, error) {
	if err := RejectLegacyEnvVars(lookup); err != nil {
		return Config{}, err
	}
	if err := requireUTF8(lookup); err != nil {
		return Config{}, err
	}
	get := func(name string) string {
		value, _ := lookup(name)
		return value
	}

	cfg := Config{DatabaseURL: get("DATABASE_URL")}
	if cfg.DatabaseURL == "" {
		cfg.DatabaseURL = defaultDatabaseURL
	}
	var err error
	if cfg.Bind, err = ParseBind(get("LIFTLOG_BIND")); err != nil {
		return Config{}, err
	}
	if cfg.TrustedProxyHeader, err = ParseTrustedProxyHeader(get("LIFTLOG_TRUSTED_PROXY_HEADER")); err != nil {
		return Config{}, err
	}
	if cfg.TrustedProxies, err = ParseTrustedProxies(get("LIFTLOG_TRUSTED_PROXIES")); err != nil {
		return Config{}, err
	}
	if cfg.CookieSecure, err = ParseBoolEnv("LIFTLOG_COOKIE_SECURE", get("LIFTLOG_COOKIE_SECURE"), false); err != nil {
		return Config{}, err
	}
	if cfg.HSTSMaxAge, err = ParseHSTSMaxAge(get("LIFTLOG_HSTS_MAX_AGE")); err != nil {
		return Config{}, err
	}
	includeSubdomains := get("LIFTLOG_HSTS_INCLUDE_SUBDOMAINS")
	if cfg.HSTSIncludeSubdomains, err = ParseBoolEnv("LIFTLOG_HSTS_INCLUDE_SUBDOMAINS", includeSubdomains, false); err != nil {
		return Config{}, err
	}
	return cfg, nil
}

// requireUTF8 fails on a non-UTF-8 value instead of using it as is (and, say,
// falling back to `Secure` off).
func requireUTF8(lookup LookupEnv) error {
	for _, name := range settingNames {
		if value, _ := lookup(name); !utf8.ValidString(value) {
			return fmt.Errorf("environment variable %s is not valid UTF-8", name)
		}
	}
	return nil
}

// ParseBind defaults to loopback 127.0.0.1:8080 when unset or empty; the
// container image sets 0.0.0.0:8080.
func ParseBind(raw string) (netip.AddrPort, error) {
	if raw == "" {
		return defaultBind, nil
	}
	addr, err := netip.ParseAddrPort(raw)
	if err != nil {
		return netip.AddrPort{}, fmt.Errorf("invalid LIFTLOG_BIND '%s': %w", raw, err)
	}
	return addr, nil
}

// ParseTrustedProxies reads comma-separated bare IPs (no CIDR), skipping empty
// segments. They are unmapped so `::ffff:10.0.0.5` matches a plain-IPv4 peer.
func ParseTrustedProxies(raw string) ([]netip.Addr, error) {
	var proxies []netip.Addr
	for _, segment := range strings.Split(raw, ",") {
		segment = strings.TrimSpace(segment)
		if segment == "" {
			continue
		}
		addr, err := netip.ParseAddr(segment)
		if err != nil {
			return nil, fmt.Errorf("invalid LIFTLOG_TRUSTED_PROXIES entry '%s': %w", segment, err)
		}
		proxies = append(proxies, addr.Unmap())
	}
	return proxies, nil
}

// ParseTrustedProxyHeader accepts none / x-forwarded-for / x-real-ip,
// case-insensitively; unset means none. Trusting a header must be opt-in:
// liftlog cannot tell whether the proxy overwrote it or passed a client's
// forgery through.
func ParseTrustedProxyHeader(raw string) (TrustedProxyHeader, error) {
	switch strings.ToLower(strings.TrimSpace(raw)) {
	case "", "none":
		return TrustNoHeader, nil
	case "x-forwarded-for":
		return TrustXForwardedFor, nil
	case "x-real-ip":
		return TrustXRealIP, nil
	}
	return TrustNoHeader, fmt.Errorf(
		"invalid LIFTLOG_TRUSTED_PROXY_HEADER '%s': expected one of none, x-forwarded-for, x-real-ip", raw)
}

// ParseBoolEnv accepts true/false/1/0, case-insensitively; unset or blank
// yields def. Anything else errors, so `COOKIE_SECURE=yes` cannot silently
// mean off.
func ParseBoolEnv(name, raw string, def bool) (bool, error) {
	switch strings.ToLower(strings.TrimSpace(raw)) {
	case "":
		return def, nil
	case "true", "1":
		return true, nil
	case "false", "0":
		return false, nil
	}
	return false, fmt.Errorf("invalid %s '%s': expected one of true, false, 1, 0", name, raw)
}

// ParseHSTSMaxAge reads seconds; unset, blank or 0 sends no HSTS. Off by
// default: liftlog never terminates TLS, and a cached wrong HSTS promise
// cannot be withdrawn.
func ParseHSTSMaxAge(raw string) (uint64, error) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return 0, nil
	}
	seconds, err := strconv.ParseUint(trimmed, 10, 64)
	if err != nil {
		return 0, fmt.Errorf("invalid LIFTLOG_HSTS_MAX_AGE '%s': %w", raw, err)
	}
	return seconds, nil
}
