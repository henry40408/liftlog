// Package harness drives the LiftLog server and a browser for the e2e suite.
package harness

import (
	"bufio"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"sync"
	"time"
)

// Password is what every seeded account gets. It must pass the server's
// password policy, with margin so a stricter threshold does not break seeding.
const Password = "barbell-club-2026"

// AdminUsername is the first account, created by /auth/setup and therefore
// the admin.
const AdminUsername = "lifter"

// BinaryEnv names the variable that pins the server binary.
const BinaryEnv = "E2E_BINARY"

const (
	startupTimeout = time.Minute
	tokenTimeout   = 5 * time.Second
	setupTokenLine = "Setup token: "
)

// Server is a running LiftLog server on a throwaway database. One server
// serves every scenario; the database is never reused, so @bootstrap sees no
// users.
type Server struct {
	// BaseURL is where the server listens, such as http://127.0.0.1:41234.
	BaseURL string
	// SetupToken is the first-run token the server printed on stderr.
	SetupToken string

	cmd *exec.Cmd
	// exited receives the result of cmd.Wait once the child is gone.
	exited chan error
	// seeding serialises account creation: concurrent scenarios hitting
	// /auth/setup together would race on the first user.
	seeding sync.Mutex
}

// Start starts the server on a free port against a fresh database.
func Start() (*Server, error) {
	binary, err := ensureBinary()
	if err != nil {
		return nil, err
	}
	database, err := freshDatabase()
	if err != nil {
		return nil, err
	}
	port, err := freePort()
	if err != nil {
		return nil, err
	}

	cmd := exec.Command(binary)
	cmd.Dir = repoRoot()
	cmd.Env = append(withoutEnv(os.Environ(), "RUST_LOG", "LIFTLOG_LOG_LEVEL"),
		"DATABASE_URL=sqlite:"+database+"?mode=rwc",
		fmt.Sprintf("LIFTLOG_BIND=127.0.0.1:%d", port),
		// Quiet child output; RUST_LOG is a retired variable the Go binary rejects.
		"LIFTLOG_LOG_LEVEL=warn",
	)
	// Passed through, so a refusal to start shows in the test output.
	cmd.Stdout = os.Stdout
	// Piped to read the setup token; forwardStderr echoes the rest.
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return nil, err
	}
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("spawning the liftlog server at %s: %w", binary, err)
	}

	server := &Server{
		BaseURL: fmt.Sprintf("http://127.0.0.1:%d", port),
		cmd:     cmd,
		exited:  make(chan error, 1),
	}
	tokens := make(chan string, 1)
	drained := make(chan struct{})
	go func() {
		forwardStderr(stderr, tokens)
		close(drained)
	}()
	// Wait must not run before the pipe is drained.
	go func() {
		<-drained
		server.exited <- cmd.Wait()
	}()

	if err := server.waitUntilHealthy(); err != nil {
		server.Stop()
		return nil, err
	}
	select {
	case server.SetupToken = <-tokens:
	case <-time.After(tokenTimeout):
		server.Stop()
		return nil, fmt.Errorf("the server printed no %q line on stderr", strings.TrimSpace(setupTokenLine))
	}
	return server, nil
}

// Stop kills the server.
func (s *Server) Stop() {
	if s.cmd == nil {
		return
	}
	_ = s.cmd.Process.Kill()
	select {
	case <-s.exited:
	case <-time.After(10 * time.Second):
	}
	s.cmd = nil
}

// URL joins a path onto the server's base URL.
func (s *Server) URL(path string) string {
	return s.BaseURL + path
}

// forwardStderr echoes the server's stderr and reports the setup token once.
func forwardStderr(stderr io.Reader, tokens chan<- string) {
	scanner := bufio.NewScanner(stderr)
	scanner.Buffer(make([]byte, 0, 64*1024), 4*1024*1024)
	// A line over the limit stops the scanner; the copy below keeps the pipe
	// draining so the child never blocks on a full stderr.
	defer func() { _, _ = io.Copy(os.Stderr, stderr) }()
	for scanner.Scan() {
		line := scanner.Text()
		fmt.Fprintln(os.Stderr, line)
		token, found := strings.CutPrefix(line, setupTokenLine)
		if !found {
			continue
		}
		select {
		case tokens <- strings.TrimSpace(token):
		default:
		}
	}
}

// waitUntilHealthy polls /health until the server answers. It gives up as
// soon as the child exits, so a startup failure shows at once.
func (s *Server) waitUntilHealthy() error {
	deadline := time.After(startupTimeout)
	for {
		select {
		case err := <-s.exited:
			s.cmd = nil
			return fmt.Errorf("the liftlog server exited before it was healthy: %v", err)
		case <-deadline:
			return fmt.Errorf("the liftlog server was not healthy at %s within %v", s.BaseURL, startupTimeout)
		case <-time.After(100 * time.Millisecond):
			if s.healthy() {
				return nil
			}
		}
	}
}

func (s *Server) healthy() bool {
	response, err := http.Get(s.URL("/health"))
	if err != nil {
		return false
	}
	_ = response.Body.Close()
	return response.StatusCode == http.StatusOK
}

// ensureBinary returns the server binary: E2E_BINARY, else bin/liftlog,
// built first when missing.
//
// An existing binary is never rebuilt, so rebuild by hand after changing
// server code. CI builds it beforehand.
func ensureBinary() (string, error) {
	if pinned := os.Getenv(BinaryEnv); pinned != "" {
		// cmd.Dir is the repo root, so a relative path must be resolved first.
		absolute, err := filepath.Abs(pinned)
		if err != nil || !isFile(absolute) {
			return "", fmt.Errorf("%s=%s is not a file", BinaryEnv, pinned)
		}
		return absolute, nil
	}

	binary := filepath.Join(repoRoot(), "bin", "liftlog")
	if isFile(binary) {
		return binary, nil
	}

	fmt.Fprintf(os.Stderr, "e2e: %s is missing, building it\n", binary)
	build := exec.Command("go", "build", "-mod=vendor", "-o", "bin/liftlog", "./cmd/liftlog")
	build.Dir = repoRoot()
	build.Stdout = os.Stdout
	build.Stderr = os.Stderr
	if err := build.Run(); err != nil {
		return "", fmt.Errorf("running go build: %w", err)
	}
	return binary, nil
}

func isFile(path string) bool {
	info, err := os.Stat(path)
	return err == nil && info.Mode().IsRegular()
}

// freshDatabase returns the database path with any previous file removed,
// including the -wal and -shm sidecars, whose replay would resurrect old users.
func freshDatabase() (string, error) {
	dir := filepath.Join(e2eRoot(), ".tmp")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	database := filepath.Join(dir, "liftlog-e2e.sqlite3")
	for _, suffix := range []string{"", "-wal", "-shm"} {
		if err := os.Remove(database + suffix); err != nil && !os.IsNotExist(err) {
			return "", err
		}
	}
	return database, nil
}

// freePort asks the OS for a free port. Racy in principle, fine in practice.
func freePort() (int, error) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return 0, fmt.Errorf("asking the OS for a free port: %w", err)
	}
	defer listener.Close()
	return listener.Addr().(*net.TCPAddr).Port, nil
}

// e2eRoot is the e2e module root, two levels above this file. It comes from
// the compile-time path, so the suite works from any working directory.
func e2eRoot() string {
	_, file, _, _ := runtime.Caller(0)
	return filepath.Join(filepath.Dir(file), "..", "..")
}

func repoRoot() string {
	return filepath.Join(e2eRoot(), "..")
}

// withoutEnv drops the named variables from an environment list.
func withoutEnv(environ []string, names ...string) []string {
	kept := make([]string, 0, len(environ))
	for _, entry := range environ {
		name, _, _ := strings.Cut(entry, "=")
		if !slices.Contains(names, name) {
			kept = append(kept, entry)
		}
	}
	return kept
}
