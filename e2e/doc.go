// Package e2e drives the LiftLog server in a real browser through Gherkin
// scenarios.
//
// Run from this directory with `go test -count=1 ./...`. The count flag is
// needed because Go's test cache cannot see a rebuilt server binary. The
// server is E2E_BINARY, else ../bin/liftlog (built if missing). Set
// E2E_FEATURES to a comma-separated list of feature paths to run a subset,
// and E2E_CHROME to pin the browser binary.
package e2e
