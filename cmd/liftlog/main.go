// Command liftlog serves the LiftLog workout journal over HTTP.
package main

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"os"

	"github.com/henry40408/liftlog/internal/config"
	"github.com/henry40408/liftlog/internal/version"
)

const (
	exitFailure = 1
	exitUsage   = 2

	dotEnvPath       = ".env"
	defaultLogFormat = "full"
	defaultLogLevel  = "info"
)

// process is everything run needs from the outside world.
type process struct {
	args      []string
	lookupEnv func(string) (string, bool)
	stdout    io.Writer
	stderr    io.Writer
}

func main() {
	// The .env file feeds the environment before anything reads it.
	if err := loadDotEnv(dotEnvPath); err != nil {
		os.Exit(reportFatal(os.Stderr, err))
	}
	os.Exit(run(process{
		args:      os.Args[1:],
		lookupEnv: os.LookupEnv,
		stdout:    os.Stdout,
		stderr:    os.Stderr,
	}))
}

// run returns the exit code. Logs go to stdout; stderr holds errors and the
// setup token.
func run(p process) int {
	// Before flags: a retired variable must stop startup however the command
	// line looks.
	if err := config.RejectLegacyEnvVars(p.lookupEnv); err != nil {
		return reportFatal(p.stderr, err)
	}
	opts, err := parseOptions(p.args, p.lookupEnv, p.stderr)
	if errors.Is(err, flag.ErrHelp) {
		return 0
	}
	if err != nil {
		return reportFatal(p.stderr, err)
	}
	if opts.showVersion {
		_, _ = fmt.Fprintf(p.stdout, "liftlog %s\n", version.Version())
		return 0
	}
	cfg, err := config.FromEnv(p.lookupEnv)
	if err != nil {
		return reportFatal(p.stderr, err)
	}
	logger := newLogger(p.stdout, opts.logFormat, opts.logLevel)
	if err := serve(serveParams{cfg: cfg, logger: logger, stderr: p.stderr}); err != nil {
		return reportFatal(p.stderr, err)
	}
	return 0
}

func reportFatal(stderr io.Writer, err error) int {
	_, _ = fmt.Fprintf(stderr, "Error: %v\n", err)
	var usage usageError
	if errors.As(err, &usage) {
		return exitUsage
	}
	return exitFailure
}
