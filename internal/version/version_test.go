package version

import "testing"

func TestInjectedVersionWins(t *testing.T) {
	t.Cleanup(func() { version = "" })
	version = "v1.2.3"
	if got := Version(); got != "v1.2.3" {
		t.Fatalf("got %q", got)
	}
}

// A test binary has no module version ("" or "(devel)"); neither may leak out.
func TestAnEmptyOrDevelModuleVersionFallsBackToDev(t *testing.T) {
	t.Cleanup(func() { version = "" })
	version = ""
	if got := Version(); got != "dev" {
		t.Fatalf("got %q, want dev", got)
	}
}
