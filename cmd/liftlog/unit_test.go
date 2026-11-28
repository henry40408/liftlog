package main

import (
	"bytes"
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"log/slog"
	"net"
	"net/http"
	"net/netip"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/config"
	"github.com/henry40408/liftlog/internal/db"
	"github.com/henry40408/liftlog/internal/repo"
)

// syncBuffer is a log sink that a test may read while the server goroutine
// still writes.
type syncBuffer struct {
	mu  sync.Mutex
	buf bytes.Buffer
}

func (b *syncBuffer) Write(p []byte) (int, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.buf.Write(p)
}

func (b *syncBuffer) String() string {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.buf.String()
}

func captureLogger() (*slog.Logger, *syncBuffer) {
	sink := &syncBuffer{}
	return slog.New(slog.NewTextHandler(sink, &slog.HandlerOptions{Level: slog.LevelDebug})), sink
}

func envFrom(values map[string]string) func(string) (string, bool) {
	return func(name string) (string, bool) {
		value, ok := values[name]
		return value, ok
	}
}

func requireContains(t *testing.T, text string, wants ...string) {
	t.Helper()
	for _, want := range wants {
		if !strings.Contains(text, want) {
			t.Errorf("output lacks %q:\n%s", want, text)
		}
	}
}

func TestProxyHeaderName(t *testing.T) {
	cases := map[config.TrustedProxyHeader]string{
		config.TrustXForwardedFor: "x-forwarded-for",
		config.TrustXRealIP:       "x-real-ip",
	}
	for header, want := range cases {
		if got := proxyHeaderName(header); got != want {
			t.Errorf("proxyHeaderName(%v) = %q, want %q", header, got, want)
		}
	}
}

func TestLogProxyTrust(t *testing.T) {
	proxy := netip.MustParseAddr("10.0.0.7")
	cases := []struct {
		name string
		cfg  config.Config
		want []string
	}{
		{
			name: "no header warns that every client shares a bucket",
			cfg:  config.Config{TrustedProxyHeader: config.TrustNoHeader},
			want: []string{"level=WARN", "LIFTLOG_TRUSTED_PROXY_HEADER is not set"},
		},
		{
			name: "header without proxies trusts loopback only",
			cfg:  config.Config{TrustedProxyHeader: config.TrustXRealIP},
			want: []string{"level=INFO", "loopback peers only", "header=x-real-ip"},
		},
		{
			name: "header with proxies names them",
			cfg:  config.Config{TrustedProxyHeader: config.TrustXForwardedFor, TrustedProxies: []netip.Addr{proxy}},
			want: []string{"level=INFO", "configured proxies", "header=x-forwarded-for", "10.0.0.7"},
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			logger, sink := captureLogger()
			logProxyTrust(tc.cfg, logger)
			requireContains(t, sink.String(), tc.want...)
		})
	}
}

func TestLogCookieAndHSTS(t *testing.T) {
	cases := []struct {
		name   string
		cfg    config.Config
		want   []string
		absent string
	}{
		{
			name:   "insecure cookie warns and HSTS stays quiet",
			cfg:    config.Config{},
			want:   []string{"level=WARN", "LIFTLOG_COOKIE_SECURE is off"},
			absent: "HSTS enabled",
		},
		{
			name:   "secure cookie is reported",
			cfg:    config.Config{CookieSecure: true},
			want:   []string{"level=INFO", "Secure attribute is enabled"},
			absent: "LIFTLOG_COOKIE_SECURE is off",
		},
		{
			name: "HSTS reports its settings",
			cfg:  config.Config{CookieSecure: true, HSTSMaxAge: 3600, HSTSIncludeSubdomains: true},
			want: []string{"HSTS enabled", "max_age=3600", "include_subdomains=true"},
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			logger, sink := captureLogger()
			logCookieAndHSTS(tc.cfg, logger)
			requireContains(t, sink.String(), tc.want...)
			if tc.absent != "" && strings.Contains(sink.String(), tc.absent) {
				t.Errorf("log has %q:\n%s", tc.absent, sink.String())
			}
		})
	}
}

func TestParseLogLevel(t *testing.T) {
	cases := map[string]slog.Level{
		"debug": slog.LevelDebug,
		"info":  slog.LevelInfo,
		"warn":  slog.LevelWarn,
		"error": slog.LevelError,
	}
	for text, want := range cases {
		got, err := parseLogLevel(text)
		if err != nil || got != want {
			t.Errorf("parseLogLevel(%q) = %v, %v; want %v", text, got, err, want)
		}
	}
	for _, text := range []string{"", "trace", "INFO", "warning"} {
		if _, err := parseLogLevel(text); err == nil || !strings.Contains(err.Error(), text) {
			t.Errorf("parseLogLevel(%q) err = %v, want a rejection naming the input", text, err)
		}
	}
}

func TestParseLogFormat(t *testing.T) {
	for _, text := range []string{"full", "json"} {
		if got, err := parseLogFormat(text); err != nil || got != text {
			t.Errorf("parseLogFormat(%q) = %q, %v", text, got, err)
		}
	}
	for _, text := range []string{"", "compact", "pretty", "JSON"} {
		if _, err := parseLogFormat(text); err == nil {
			t.Errorf("parseLogFormat(%q) accepted", text)
		}
	}
}

func TestNewLoggerFormatsAndFiltersByLevel(t *testing.T) {
	var jsonOut, textOut bytes.Buffer
	newLogger(&jsonOut, "json", slog.LevelWarn).Info("hidden")
	newLogger(&jsonOut, "json", slog.LevelWarn).Warn("shown")
	newLogger(&textOut, "full", slog.LevelInfo).Info("plain")

	requireContains(t, jsonOut.String(), `"msg":"shown"`)
	if strings.Contains(jsonOut.String(), "hidden") {
		t.Errorf("an info line passed a warn filter: %s", jsonOut.String())
	}
	requireContains(t, textOut.String(), "msg=plain")
}

func TestEnvOrDefault(t *testing.T) {
	lookup := envFrom(map[string]string{"SET": "value", "EMPTY": ""})
	cases := map[string]string{"SET": "value", "EMPTY": "fallback", "ABSENT": "fallback"}
	for name, want := range cases {
		if got := envOrDefault(lookup, name, "fallback"); got != want {
			t.Errorf("envOrDefault(%q) = %q, want %q", name, got, want)
		}
	}
}

func TestParseOptionsPrecedenceAndErrors(t *testing.T) {
	t.Run("environment feeds the defaults", func(t *testing.T) {
		env := envFrom(map[string]string{"LIFTLOG_LOG_FORMAT": "json", "LIFTLOG_LOG_LEVEL": "error"})
		opts, err := parseOptions(nil, env, io.Discard)
		if err != nil || opts.logFormat != "json" || opts.logLevel != slog.LevelError {
			t.Fatalf("opts = %+v, err = %v", opts, err)
		}
	})
	t.Run("a flag beats the environment", func(t *testing.T) {
		env := envFrom(map[string]string{"LIFTLOG_LOG_LEVEL": "error"})
		opts, err := parseOptions([]string{"--log-level", "debug", "--version"}, env, io.Discard)
		if err != nil || opts.logLevel != slog.LevelDebug || !opts.showVersion {
			t.Fatalf("opts = %+v, err = %v", opts, err)
		}
	})
	t.Run("help prints usage and reports ErrHelp", func(t *testing.T) {
		var usage bytes.Buffer
		_, err := parseOptions([]string{"-h"}, envFrom(nil), &usage)
		if !errors.Is(err, flag.ErrHelp) {
			t.Fatalf("err = %v, want flag.ErrHelp", err)
		}
		requireContains(t, usage.String(), "Usage: liftlog", "-log-format", "-log-level", "-version")
	})
	bad := []struct {
		name string
		args []string
		env  map[string]string
		want string
	}{
		{"unknown flag", []string{"--nope"}, nil, "nope"},
		{"stray argument", []string{"extra"}, nil, `unexpected argument "extra"`},
		{"bad format flag", []string{"--log-format", "pretty"}, nil, "unknown log format"},
		{"bad level from env", nil, map[string]string{"LIFTLOG_LOG_LEVEL": "trace"}, "unknown log level"},
	}
	for _, tc := range bad {
		t.Run(tc.name, func(t *testing.T) {
			_, err := parseOptions(tc.args, envFrom(tc.env), io.Discard)
			var usage usageError
			if !errors.As(err, &usage) || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("err = %v, want a usageError containing %q", err, tc.want)
			}
		})
	}
}

func TestReportFatalPicksTheExitCode(t *testing.T) {
	var stderr bytes.Buffer
	if code := reportFatal(&stderr, errors.New("boom")); code != exitFailure {
		t.Errorf("plain error exit = %d", code)
	}
	if code := reportFatal(&stderr, usageError{errors.New("bad flag")}); code != exitUsage {
		t.Errorf("usage error exit = %d", code)
	}
	if code := reportFatal(&stderr, fmt.Errorf("ctx: %w", usageError{errors.New("wrapped")})); code != exitUsage {
		t.Errorf("wrapped usage error exit = %d", code)
	}
	requireContains(t, stderr.String(), "Error: boom\n", "Error: bad flag\n")
}

func runInProcess(args []string, env map[string]string) (code int, stdout, stderr string) {
	var out, errw bytes.Buffer
	code = run(process{args: args, lookupEnv: envFrom(env), stdout: &out, stderr: &errw})
	return code, out.String(), errw.String()
}

func TestRunFailsBeforeServing(t *testing.T) {
	cases := []struct {
		name string
		args []string
		env  map[string]string
		code int
		want string
	}{
		{"legacy variable", nil, map[string]string{"RUST_LOG": "debug"}, exitFailure, "RUST_LOG (renamed to LIFTLOG_LOG_LEVEL)"},
		{"legacy variable beats a bad flag", []string{"--nope"}, map[string]string{"BIND": "x"}, exitFailure, "BIND (renamed to LIFTLOG_BIND)"},
		{"bad flag", []string{"--nope"}, nil, exitUsage, "Error: "},
		{"bad log format", nil, map[string]string{"LIFTLOG_LOG_FORMAT": "pretty"}, exitUsage, "unknown log format"},
		{"bad bind", nil, map[string]string{"LIFTLOG_BIND": "not-an-address"}, exitFailure, "LIFTLOG_BIND"},
		{"unopenable database", nil, map[string]string{"DATABASE_URL": "sqlite:" + filepath.Join(t.TempDir(), "missing", "x.db")}, exitFailure, "open database"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			code, stdout, stderr := runInProcess(tc.args, tc.env)
			if code != tc.code {
				t.Errorf("exit = %d, want %d (stderr %q)", code, tc.code, stderr)
			}
			requireContains(t, stderr, tc.want)
			if strings.Contains(stdout, "Starting server") {
				t.Errorf("the server started:\n%s", stdout)
			}
		})
	}
}

func TestRunHelpAndVersionExitZero(t *testing.T) {
	code, stdout, stderr := runInProcess([]string{"--version"}, nil)
	if code != 0 || !strings.HasPrefix(stdout, "liftlog ") || stderr != "" {
		t.Errorf("--version: exit %d, stdout %q, stderr %q", code, stdout, stderr)
	}
	code, stdout, stderr = runInProcess([]string{"-h"}, nil)
	if code != 0 || stdout != "" || !strings.Contains(stderr, "Usage: liftlog") {
		t.Errorf("-h: exit %d, stdout %q, stderr %q", code, stdout, stderr)
	}
}

func TestRunServesUntilSIGTERMThenShutsDownCleanly(t *testing.T) {
	env := map[string]string{
		"LIFTLOG_BIND": "127.0.0.1:0",
		"DATABASE_URL": "sqlite:" + filepath.Join(t.TempDir(), "liftlog.sqlite3") + "?mode=rwc",
	}
	var stderr syncBuffer
	stdout := &syncBuffer{}
	exit := make(chan int, 1)
	go func() {
		exit <- run(process{lookupEnv: envFrom(env), stdout: stdout, stderr: &stderr})
	}()

	// serve installs its signal handler before it logs "Starting server", so
	// the signal below can never hit the default action.
	waitUntil(t, "the server to start", func() bool { return strings.Contains(stdout.String(), "Starting server") })
	if err := syscall.Kill(os.Getpid(), syscall.SIGTERM); err != nil {
		t.Fatal(err)
	}
	select {
	case code := <-exit:
		if code != 0 {
			t.Fatalf("exit = %d, stderr %q", code, stderr.String())
		}
	case <-time.After(logWait):
		t.Fatal("run did not return after SIGTERM")
	}
	requireContains(t, stdout.String(), "Received shutdown signal", "Server shut down gracefully")
	requireContains(t, stderr.String(), "Setup token: ")
}

// waitUntil polls a condition; the bound only stops a wedged test.
func waitUntil(t *testing.T, what string, done func() bool) {
	t.Helper()
	deadline := time.Now().Add(logWait)
	for !done() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(5 * time.Millisecond)
	}
}

func TestServeFailsWhenTheAddressIsTaken(t *testing.T) {
	taken, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = taken.Close() })
	logger, _ := captureLogger()
	cfg := config.Config{
		DatabaseURL: "sqlite::memory:",
		Bind:        netip.MustParseAddrPort(taken.Addr().String()),
	}

	err = serve(serveParams{cfg: cfg, logger: logger, stderr: io.Discard})
	if err == nil || !strings.Contains(err.Error(), "address already in use") {
		t.Fatalf("err = %v, want an address-in-use failure", err)
	}
}

func TestOpenDatabaseRejectsAnUnopenablePath(t *testing.T) {
	logger, _ := captureLogger()
	cfg := config.Config{DatabaseURL: "sqlite:" + filepath.Join(t.TempDir(), "missing", "x.db")}
	pool, err := openDatabase(context.Background(), cfg, logger)
	if err == nil || pool != nil {
		t.Fatalf("pool = %v, err = %v; want an error and no pool", pool, err)
	}
}

func TestOpenDatabaseClosesThePoolWhenAMigrationFails(t *testing.T) {
	path := filepath.Join(t.TempDir(), "broken.sqlite3")
	seed, err := db.Open("sqlite:" + path)
	if err != nil {
		t.Fatal(err)
	}
	// A _migrations table without a name column makes reading the old migration
	// records fail.
	mustExec(t, seed, `CREATE TABLE _migrations (id INTEGER)`)
	if err := seed.Close(); err != nil {
		t.Fatal(err)
	}

	logger, _ := captureLogger()
	pool, err := openDatabase(context.Background(), config.Config{DatabaseURL: "sqlite:" + path}, logger)
	if err == nil || pool != nil {
		t.Fatalf("pool = %v, err = %v; want a migration error and no pool", pool, err)
	}
	requireContains(t, err.Error(), "read old migration records")
}

func TestOpenDatabaseMigratesAFreshFile(t *testing.T) {
	logger, sink := captureLogger()
	cfg := config.Config{DatabaseURL: "sqlite:" + filepath.Join(t.TempDir(), "fresh.sqlite3") + "?mode=rwc"}
	pool, err := openDatabase(context.Background(), cfg, logger)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = pool.Close() })
	var users int
	if err := pool.QueryRow(`SELECT COUNT(*) FROM users`).Scan(&users); err != nil {
		t.Fatalf("the schema is missing: %v", err)
	}
	requireContains(t, sink.String(), "Connecting to database", "Migrations completed")
}

func TestAnnounceSetupTokenFailsWhenUsersCannotBeCounted(t *testing.T) {
	pool, err := db.OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	// The pool is closed on purpose, so Count fails.
	if err := pool.Close(); err != nil {
		t.Fatal(err)
	}
	var stderr bytes.Buffer
	logger, _ := captureLogger()
	token, err := announceSetupToken(context.Background(), repo.NewUsers(pool), &stderr, logger)
	if err == nil || token != nil || !strings.Contains(err.Error(), "count users") {
		t.Fatalf("token = %v, err = %v; want a count users error", token, err)
	}
	if stderr.Len() != 0 {
		t.Errorf("stderr = %q, want nothing", stderr.String())
	}
}

func TestShutdownGracefullyLeavesQuietWhenIdle(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	httpServer := &http.Server{Handler: http.NewServeMux(), ReadHeaderTimeout: time.Second}
	served := make(chan error, 1)
	go func() { served <- httpServer.Serve(listener) }()

	logger, sink := captureLogger()
	shutdownGracefully(httpServer, logger, time.Second)

	if err := <-served; !errors.Is(err, http.ErrServerClosed) {
		t.Errorf("Serve returned %v, want ErrServerClosed", err)
	}
	if sink.String() != "" {
		t.Errorf("a clean shutdown logged:\n%s", sink.String())
	}
}

func TestShutdownGracefullyForcesCloseWhenARequestOutlastsTheTimeout(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	started := make(chan struct{})
	release := make(chan struct{})
	httpServer := &http.Server{
		ReadHeaderTimeout: time.Second,
		Handler: http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
			close(started)
			<-release
		}),
	}
	go func() { _ = httpServer.Serve(listener) }()
	clientDone := make(chan struct{})
	go func() {
		defer close(clientDone)
		if response, err := http.Get("http://" + listener.Addr().String()); err == nil {
			_ = response.Body.Close()
		}
	}()
	<-started

	logger, sink := captureLogger()
	shutdownGracefully(httpServer, logger, 20*time.Millisecond)
	close(release)
	<-clientDone

	requireContains(t, sink.String(), "level=ERROR", "graceful shutdown failed, closing connections", "context deadline exceeded")
	if strings.Contains(sink.String(), "failed to close the server") {
		t.Errorf("closing a live server was reported as a failure:\n%s", sink.String())
	}
}

func TestLoadDotEnvReportsUnreadableAndMalformedFiles(t *testing.T) {
	dir := t.TempDir()
	notADirectory := filepath.Join(dir, "plain")
	if err := os.WriteFile(notADirectory, []byte("X=1\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	malformed := filepath.Join(dir, "malformed")
	if err := os.WriteFile(malformed, []byte("LIFTLOG_DOTENV_TEST_OK=1\nno equals sign\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	cases := []struct {
		name string
		path string
		want string
	}{
		{"a path under a regular file", filepath.Join(notADirectory, "child"), "open "},
		{"a directory instead of a file", dir, "parse "},
		{"a malformed line", malformed, "line 2: want NAME=value"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			err := loadDotEnv(tc.path)
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("err = %v, want it to contain %q", err, tc.want)
			}
		})
	}
	if _, set := os.LookupEnv("LIFTLOG_DOTENV_TEST_OK"); set {
		t.Error("a file that failed to parse still set variables")
	}
}

func TestLoadDotEnvRejectsAnUnsettableName(t *testing.T) {
	path := filepath.Join(t.TempDir(), "env")
	if err := os.WriteFile(path, []byte("BAD\x00NAME=1\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := loadDotEnv(path); err == nil {
		t.Fatal("a name with a NUL byte was accepted")
	}
}

func TestParseDotEnvValueForms(t *testing.T) {
	cases := []struct{ line, want string }{
		{`A="unclosed`, `"unclosed`},
		{`A='quoted' trailing`, `'quoted' trailing`},
		{`A="quoted" # note`, "quoted"},
		{`A=bare # note`, "bare"},
		{`A=has#hash`, "has#hash"},
		{`A=`, ""},
		{`A=""`, ""},
	}
	for _, tc := range cases {
		entries, err := parseDotEnv(strings.NewReader(tc.line))
		if err != nil || len(entries) != 1 || entries[0].value != tc.want {
			t.Errorf("%s -> %+v, %v; want value %q", tc.line, entries, err, tc.want)
		}
	}
}
