package config

import (
	"fmt"
	"strings"
)

// legacyEnvVar pairs a retired environment variable with its replacement.
type legacyEnvVar struct{ old, replacement string }

// legacyEnvVars lists the retired variables. Update it whenever an environment
// variable is added, renamed or removed: a silently ignored one is worse than
// a startup failure.
var legacyEnvVars = []legacyEnvVar{
	{"BIND", "LIFTLOG_BIND"},
	{"LOG_FORMAT", "LIFTLOG_LOG_FORMAT"},
	// A global RUST_LOG would otherwise be ignored without a word.
	{"RUST_LOG", "LIFTLOG_LOG_LEVEL"},
}

// RejectLegacyEnvVars fails when a retired variable is set, even empty.
func RejectLegacyEnvVars(lookup LookupEnv) error {
	var found []string
	for _, legacy := range legacyEnvVars {
		if _, ok := lookup(legacy.old); ok {
			found = append(found, fmt.Sprintf("%s (renamed to %s)", legacy.old, legacy.replacement))
		}
	}
	if len(found) == 0 {
		return nil
	}
	return fmt.Errorf("refusing to start: removed environment variable(s) still set: %s. "+
		"Rename them in your deployment to the LIFTLOG_-prefixed names", strings.Join(found, ", "))
}
