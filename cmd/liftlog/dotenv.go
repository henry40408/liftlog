package main

import (
	"bufio"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"strings"
)

// loadDotEnv reads path and sets each variable the process does not already
// have, so a real environment always beats the file. A missing file is fine.
func loadDotEnv(path string) error {
	file, err := os.Open(path)
	if errors.Is(err, fs.ErrNotExist) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("open %s: %w", path, err)
	}
	defer func() { _ = file.Close() }()
	entries, err := parseDotEnv(file)
	if err != nil {
		return fmt.Errorf("parse %s: %w", path, err)
	}
	for _, entry := range entries {
		if _, set := os.LookupEnv(entry.name); set {
			continue
		}
		if err := os.Setenv(entry.name, entry.value); err != nil {
			return err
		}
	}
	return nil
}

type dotEnvEntry struct{ name, value string }

// parseDotEnv accepts `NAME=value` lines, an optional `export ` prefix, blank
// lines, `#` comments, and single or double quotes around the value.
func parseDotEnv(r io.Reader) ([]dotEnvEntry, error) {
	var entries []dotEnvEntry
	scanner := bufio.NewScanner(r)
	for lineNumber := 1; scanner.Scan(); lineNumber++ {
		text := scanner.Text()
		if lineNumber == 1 {
			text = strings.TrimPrefix(text, "\xef\xbb\xbf")
		}
		line := strings.TrimSpace(text)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		name, value, found := strings.Cut(strings.TrimPrefix(line, "export "), "=")
		name = strings.TrimSpace(name)
		if !found || name == "" {
			return nil, fmt.Errorf("line %d: want NAME=value", lineNumber)
		}
		entries = append(entries, dotEnvEntry{name: name, value: dotEnvValue(value)})
	}
	return entries, scanner.Err()
}

// dotEnvValue supports a bare value with an optional ` #` comment, or a single
// or double quoted value with an optional trailing `#` comment; no escapes.
func dotEnvValue(raw string) string {
	value := strings.TrimSpace(raw)
	if quoted, ok := unquoteDotEnv(value); ok {
		return quoted
	}
	if before, _, found := strings.Cut(value, " #"); found {
		return strings.TrimSpace(before)
	}
	return value
}

// unquoteDotEnv returns the text between the opening quote and the next
// matching quote, provided only blank or a `#` comment follows it.
func unquoteDotEnv(value string) (string, bool) {
	if value == "" || (value[0] != '"' && value[0] != '\'') {
		return "", false
	}
	end := strings.IndexByte(value[1:], value[0])
	if end < 0 {
		return "", false
	}
	rest := strings.TrimSpace(value[end+2:])
	if rest != "" && !strings.HasPrefix(rest, "#") {
		return "", false
	}
	return value[1 : end+1], true
}
