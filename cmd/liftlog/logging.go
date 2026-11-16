package main

import (
	"fmt"
	"io"
	"log/slog"
)

var logFormats = []string{"full", "json"}

// parseLogFormat rejects the Rust build's compact and pretty layouts, which
// slog does not have, rather than silently ignoring them.
func parseLogFormat(text string) (string, error) {
	for _, format := range logFormats {
		if text == format {
			return text, nil
		}
	}
	return "", fmt.Errorf("unknown log format %q (want full or json)", text)
}

func parseLogLevel(text string) (slog.Level, error) {
	switch text {
	case "debug":
		return slog.LevelDebug, nil
	case "info":
		return slog.LevelInfo, nil
	case "warn":
		return slog.LevelWarn, nil
	case "error":
		return slog.LevelError, nil
	}
	return 0, fmt.Errorf("unknown log level %q (want debug, info, warn or error)", text)
}

// newLogger writes to w, which is stdout: stderr is reserved for the setup
// token. Audit events carry their target=liftlog::audit attribute themselves.
func newLogger(w io.Writer, format string, level slog.Level) *slog.Logger {
	handlerOptions := &slog.HandlerOptions{Level: level}
	if format == "json" {
		return slog.New(slog.NewJSONHandler(w, handlerOptions))
	}
	return slog.New(slog.NewTextHandler(w, handlerOptions))
}
