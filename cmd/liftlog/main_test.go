package main

import (
	"bufio"
	"bytes"
	"context"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/henry40408/liftlog/internal/db"
)

// logWait bounds a wedged binary rather than pacing the tests; generous so a
// loaded machine never reaches it.
const logWait = 30 * time.Second

const strongPassword = "Tr0ub4dor&3-purple-giraffe-lamp-7491"

var liftlogBinary string

// coverDir is where a `-cover` binary writes its profile; empty means the
// binary is built plain. `go test -coverprofile` does not see the exec'd binary.
var coverDir = os.Getenv("LIFTLOG_COVERDIR")

func TestMain(m *testing.M) {
	os.Exit(buildAndRun(m))
}

// buildAndRun builds the binary once, so every test drives the real thing.
func buildAndRun(m *testing.M) int {
	dir, err := os.MkdirTemp("", "liftlog-bin-")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	defer func() { _ = os.RemoveAll(dir) }()
	liftlogBinary = filepath.Join(dir, "liftlog")
	buildArgs := []string{"build", "-mod=vendor"}
	if coverDir != "" {
		if err := os.MkdirAll(coverDir, 0o755); err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 1
		}
		buildArgs = append(buildArgs, "-cover", "-coverpkg=github.com/henry40408/liftlog/...")
	}
	build := exec.Command("go", append(buildArgs, "-o", liftlogBinary, ".")...)
	if out, err := build.CombinedOutput(); err != nil {
		fmt.Fprintf(os.Stderr, "go build failed: %v\n%s", err, out)
		return 1
	}
	return m.Run()
}

// serverEnv is the environment of a server under test, on a fresh database.
func serverEnv(t *testing.T, extra ...string) []string {
	t.Helper()
	env := []string{
		"LIFTLOG_BIND=127.0.0.1:0",
		"DATABASE_URL=sqlite:" + filepath.Join(t.TempDir(), "liftlog.sqlite3") + "?mode=rwc",
	}
	return append(env, extra...)
}

func command(ctx context.Context, env []string, args ...string) *exec.Cmd {
	cmd := exec.CommandContext(ctx, liftlogBinary, args...)
	cmd.Env = env
	cmd.Dir = filepath.Dir(liftlogBinary) // no stray .env is read
	if coverDir != "" {
		cmd.Env = append(append([]string(nil), env...), "GOCOVERDIR="+coverDir)
	}
	return cmd
}

func runToCompletion(t *testing.T, env []string, args ...string) (stdout, stderr string, exitCode int) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), logWait)
	defer cancel()
	cmd := command(ctx, env, args...)
	var out, errw bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, &errw
	err := cmd.Run()
	if err != nil && cmd.ProcessState == nil {
		t.Fatalf("failed to run liftlog: %v", err)
	}
	return out.String(), errw.String(), cmd.ProcessState.ExitCode()
}

func TestVersionFlag(t *testing.T) {
	stdout, stderr, code := runToCompletion(t, nil, "--version")
	if code != 0 || stderr != "" || !strings.HasPrefix(stdout, "liftlog ") {
		t.Fatalf("exit %d, stdout %q, stderr %q", code, stdout, stderr)
	}
}

// runningServer is a liftlog process whose stdout and stderr are read line by line.
type runningServer struct {
	cmd    *exec.Cmd
	stdout chan string
	stderr chan string
	logs   []string
	errs   []string
}

func startServer(t *testing.T, env []string, args ...string) *runningServer {
	t.Helper()
	cmd := command(context.Background(), env, args...)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := cmd.Start(); err != nil {
		t.Fatalf("failed to start liftlog: %v", err)
	}
	t.Cleanup(func() { _ = cmd.Process.Kill(); _ = cmd.Wait() })
	return &runningServer{cmd: cmd, stdout: lines(stdout), stderr: lines(stderr)}
}

func lines(r io.Reader) chan string {
	out := make(chan string, 256)
	go func() {
		defer close(out)
		scanner := bufio.NewScanner(r)
		for scanner.Scan() {
			out <- scanner.Text()
		}
	}()
	return out
}

// waitFor reads stdout lines until one contains marker, recording every line.
func (s *runningServer) waitFor(t *testing.T, marker string) string {
	t.Helper()
	timeout := time.After(logWait)
	for {
		select {
		case line, ok := <-s.stdout:
			if !ok {
				t.Fatalf("stdout closed before %q; logs:\n%s", marker, s.joined())
			}
			s.logs = append(s.logs, line)
			if strings.Contains(line, marker) {
				return line
			}
		case <-timeout:
			t.Fatalf("%q was never logged; logs:\n%s", marker, s.joined())
		}
	}
}

// drain collects the remaining output until both streams close.
func (s *runningServer) drain(t *testing.T) {
	t.Helper()
	timeout := time.After(logWait)
	for s.stdout != nil || s.stderr != nil {
		select {
		case line, ok := <-s.stdout:
			if !ok {
				s.stdout = nil
				break
			}
			s.logs = append(s.logs, line)
		case line, ok := <-s.stderr:
			if !ok {
				s.stderr = nil
				break
			}
			s.errs = append(s.errs, line)
		case <-timeout:
			t.Fatalf("the server never closed its output; logs:\n%s", s.joined())
		}
	}
}

func (s *runningServer) joined() string { return strings.Join(s.logs, "\n") }

func (s *runningServer) wait(t *testing.T) int {
	t.Helper()
	done := make(chan error, 1)
	go func() { done <- s.cmd.Wait() }()
	select {
	case <-done: //nolint:errcheck // only the exit matters, not Wait's error
	case <-time.After(logWait):
		t.Fatalf("the server never exited; logs:\n%s", s.joined())
	}
	return s.cmd.ProcessState.ExitCode()
}

func (s *runningServer) terminate(t *testing.T) {
	t.Helper()
	if err := s.cmd.Process.Signal(syscall.SIGTERM); err != nil {
		t.Fatal(err)
	}
}

// baseURL waits for the listen log line and returns the server's URL.
func (s *runningServer) baseURL(t *testing.T) string {
	t.Helper()
	line := s.waitFor(t, "Starting server")
	match := regexp.MustCompile(`addr=(127\.0\.0\.1:\d+)`).FindStringSubmatch(line)
	if match == nil {
		t.Fatalf("no address in %q", line)
	}
	return "http://" + match[1]
}

// setupToken reads stderr until the setup token line and returns the token.
func (s *runningServer) setupToken(t *testing.T) string {
	t.Helper()
	select {
	case line := <-s.stderr:
		s.errs = append(s.errs, line)
		token, found := strings.CutPrefix(line, "Setup token: ")
		if !found {
			t.Fatalf("stderr line %q does not start with %q", line, "Setup token: ")
		}
		return token
	case <-time.After(logWait):
		t.Fatal("no setup token on stderr")
		return ""
	}
}

func TestStartsAndServesHealth(t *testing.T) {
	server := startServer(t, serverEnv(t))
	base := server.baseURL(t)

	response, err := http.Get(base + "/health")
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = response.Body.Close() }()
	body, _ := io.ReadAll(response.Body)
	if response.StatusCode != http.StatusOK || !strings.Contains(string(body), `"status":"ok"`) {
		t.Fatalf("GET /health = %d %q", response.StatusCode, body)
	}
}

func TestSetupTokenIsPrintedOnStderrOnly(t *testing.T) {
	server := startServer(t, serverEnv(t))
	token := server.setupToken(t)
	if !regexp.MustCompile(`^[0-9A-Z]{5}-[0-9A-Z]{5}$`).MatchString(token) {
		t.Fatalf("token %q is not XXXXX-XXXXX", token)
	}
	server.baseURL(t)
	server.terminate(t)
	server.drain(t)
	server.wait(t)

	if strings.Contains(server.joined(), token) {
		t.Fatalf("the token leaked into the log:\n%s", server.joined())
	}
	if !strings.Contains(server.joined(), "Setup token issued") {
		t.Fatalf("no pointer to stderr in the log:\n%s", server.joined())
	}
	if len(server.errs) != 1 {
		t.Fatalf("stderr = %q, want only the token line", server.errs)
	}
}

func TestNoSetupTokenOnceAUserExists(t *testing.T) {
	env := serverEnv(t)
	first := startServer(t, env)
	token := first.setupToken(t)
	base := first.baseURL(t)
	postSetup(t, base, token)
	first.terminate(t)
	first.drain(t)
	if code := first.wait(t); code != 0 {
		t.Fatalf("first run exited with %d; logs:\n%s", code, first.joined())
	}

	second := startServer(t, env)
	second.waitFor(t, "Starting server")
	second.terminate(t)
	second.drain(t)
	second.wait(t)
	if len(second.errs) != 0 {
		t.Fatalf("stderr = %q, want nothing", second.errs)
	}
}

func postSetup(t *testing.T, base, token string) {
	t.Helper()
	client := &http.Client{CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	form := url.Values{"username": {"owner"}, "password": {strongPassword}, "setup_token": {token}}
	response, err := client.PostForm(base+"/auth/setup", form)
	if err != nil {
		t.Fatal(err)
	}
	_ = response.Body.Close()
	if response.StatusCode != http.StatusSeeOther {
		t.Fatalf("POST /auth/setup = %d, want 303", response.StatusCode)
	}
}

// SIGTERM is what a container runtime sends; the server must drain and exit 0
// rather than die from the signal.
func TestSIGTERMShutsDownGracefully(t *testing.T) {
	server := startServer(t, serverEnv(t))
	server.baseURL(t)
	server.terminate(t)
	server.drain(t)
	if code := server.wait(t); code != 0 {
		t.Fatalf("exited with %d; logs:\n%s", code, server.joined())
	}
	logs := server.joined()
	for _, want := range []string{"Received shutdown signal", "Server shut down gracefully"} {
		if !strings.Contains(logs, want) {
			t.Errorf("logs lack %q:\n%s", want, logs)
		}
	}
}

func TestShutdownCheckpointsTheWAL(t *testing.T) {
	env := serverEnv(t)
	server := startServer(t, env)
	token := server.setupToken(t)
	postSetup(t, server.baseURL(t), token)

	// SQLite checkpoints implicitly when the last connection closes. A second
	// open connection here means only the server's explicit checkpoint can
	// empty the WAL.
	databasePath := strings.TrimSuffix(strings.TrimPrefix(env[1], "DATABASE_URL=sqlite:"), "?mode=rwc")
	observer, err := db.Open(databasePath)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = observer.Close() })
	walPath := databasePath + "-wal"
	if info, err := os.Stat(walPath); err != nil || info.Size() == 0 {
		t.Fatalf("precondition: the WAL should hold the setup write (stat %v, err %v)", info, err)
	}

	server.terminate(t)
	server.drain(t)
	server.wait(t)

	info, err := os.Stat(walPath)
	if err != nil || info.Size() != 0 {
		t.Fatalf("the WAL was not truncated by shutdown (stat %v, err %v)", info, err)
	}
}

func TestLegacyEnvVarsAbortStartup(t *testing.T) {
	for _, name := range []string{"BIND", "LOG_FORMAT", "RUST_LOG"} {
		t.Run(name, func(t *testing.T) {
			stdout, stderr, code := runToCompletion(t, append(serverEnv(t), name+"=x"))
			if code != 1 || stdout != "" || !strings.Contains(stderr, name+" (renamed to LIFTLOG_") {
				t.Fatalf("exit %d, stdout %q, stderr %q", code, stdout, stderr)
			}
		})
	}
}

func TestRustLogNamesTheReplacement(t *testing.T) {
	_, stderr, _ := runToCompletion(t, append(serverEnv(t), "RUST_LOG=debug"))
	if !strings.Contains(stderr, "RUST_LOG (renamed to LIFTLOG_LOG_LEVEL)") {
		t.Fatalf("stderr = %q", stderr)
	}
}

func TestInvalidOptionExitsWithUsageCode(t *testing.T) {
	for _, args := range [][]string{{"--log-format", "pretty"}, {"--log-level", "trace"}, {"extra"}, {"--nope"}} {
		stdout, stderr, code := runToCompletion(t, serverEnv(t), args...)
		if code != 2 || stdout != "" || !strings.HasPrefix(stderr, "Error: ") {
			t.Errorf("%v: exit %d, stdout %q, stderr %q", args, code, stdout, stderr)
		}
	}
}

func TestInvalidConfigurationExitsWithFailure(t *testing.T) {
	_, stderr, code := runToCompletion(t, append(serverEnv(t), "LIFTLOG_BIND=not-an-address"))
	if code != 1 || !strings.Contains(stderr, "LIFTLOG_BIND") {
		t.Fatalf("exit %d, stderr %q", code, stderr)
	}
}

func TestFlagBeatsEnvironmentForLogFormat(t *testing.T) {
	env := append(serverEnv(t), "LIFTLOG_LOG_FORMAT=json")
	server := startServer(t, env, "--log-format", "full")
	line := server.waitFor(t, "Starting server")
	if !strings.HasPrefix(line, "time=") {
		t.Fatalf("want the text layout, got %q", line)
	}
}

func TestEnvironmentSelectsJSONLogs(t *testing.T) {
	server := startServer(t, append(serverEnv(t), "LIFTLOG_LOG_FORMAT=json"))
	line := server.waitFor(t, "Starting server")
	if !strings.HasPrefix(line, "{") {
		t.Fatalf("want JSON, got %q", line)
	}
}

func TestLogLevelHidesInfo(t *testing.T) {
	server := startServer(t, append(serverEnv(t), "LIFTLOG_LOG_LEVEL=warn"))
	server.waitFor(t, "LIFTLOG_COOKIE_SECURE is off")
	for _, line := range server.logs {
		if strings.Contains(line, "level=INFO") {
			t.Fatalf("info line at level warn: %q", line)
		}
	}
}

func TestDotEnvFileFeedsTheEnvironmentButNeverOverridesIt(t *testing.T) {
	dir := t.TempDir()
	dotEnv := "LIFTLOG_LOG_FORMAT=json\nLIFTLOG_LOG_LEVEL=warn\n"
	if err := os.WriteFile(filepath.Join(dir, ".env"), []byte(dotEnv), 0o600); err != nil {
		t.Fatal(err)
	}
	env := append(serverEnv(t), "LIFTLOG_LOG_LEVEL=info")
	cmd := command(context.Background(), env)
	cmd.Dir = dir
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = cmd.Process.Kill(); _ = cmd.Wait() })

	// The file's json format applies; the real environment's info level wins
	// over the file's warn, so the info line is there.
	line, _ := bufio.NewReader(stdout).ReadString('\n')
	if !strings.HasPrefix(line, "{") || !strings.Contains(line, `"level":"INFO"`) {
		t.Fatalf("first log line = %q", line)
	}
}
