package main

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"log/slog"
)

// usageError marks a bad command line or environment value; main exits 2.
type usageError struct{ err error }

func (e usageError) Error() string { return e.err.Error() }
func (e usageError) Unwrap() error { return e.err }

// options are the process-level settings. The server's own settings come from
// config.FromEnv.
type options struct {
	logFormat   string
	logLevel    slog.Level
	showVersion bool
}

// parseOptions applies precedence flag > environment > default: the
// environment is read first and a flag parsed afterwards overrides it. An
// empty variable counts as unset.
func parseOptions(args []string, lookupEnv func(string) (string, bool), errw io.Writer) (options, error) {
	format := envOrDefault(lookupEnv, "LIFTLOG_LOG_FORMAT", defaultLogFormat)
	level := envOrDefault(lookupEnv, "LIFTLOG_LOG_LEVEL", defaultLogLevel)
	var showVersion bool
	flags := flag.NewFlagSet("liftlog", flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	flags.StringVar(&format, "log-format", format, "log format: full or json (env LIFTLOG_LOG_FORMAT)")
	flags.StringVar(&level, "log-level", level, "log level: debug, info, warn or error (env LIFTLOG_LOG_LEVEL)")
	flags.BoolVar(&showVersion, "version", false, "print the version and exit")
	if err := flags.Parse(args); err != nil {
		if errors.Is(err, flag.ErrHelp) {
			_, _ = fmt.Fprintln(errw, "Usage: liftlog [flags]")
			flags.SetOutput(errw)
			flags.PrintDefaults()
		}
		return options{}, usageError{err}
	}
	if flags.NArg() > 0 {
		return options{}, usageError{fmt.Errorf("unexpected argument %q", flags.Arg(0))}
	}
	return buildOptions(format, level, showVersion)
}

func envOrDefault(lookupEnv func(string) (string, bool), name, fallback string) string {
	if value, ok := lookupEnv(name); ok && value != "" {
		return value
	}
	return fallback
}

func buildOptions(format, level string, showVersion bool) (options, error) {
	parsedFormat, err := parseLogFormat(format)
	if err != nil {
		return options{}, usageError{err}
	}
	parsedLevel, err := parseLogLevel(level)
	if err != nil {
		return options{}, usageError{err}
	}
	return options{logFormat: parsedFormat, logLevel: parsedLevel, showVersion: showVersion}, nil
}
