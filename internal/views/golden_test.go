package views

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"strings"
	"testing"

	"github.com/henry40408/liftlog/internal/version"
)

// capturedVersion is the build version the Rust server stamped into its pages.
const capturedVersion = "v0.30.1-1-g19c79d8"

// fragmentSources names the page each cut-out digest comes from, for the
// recovery hint.
var fragmentSources = map[string]string{
	"login-shell":   "login",
	"dashboard-nav": "dashboard",
}

// normaliseVersion maps the build version in the three places a page shows it
// (the two asset queries and the settings footer) back to capturedVersion.
// Replacing the bare version instead would also rewrite unrelated text when it
// is as short as "dev".
func normaliseVersion(page, current string) string {
	if current == capturedVersion {
		return page
	}
	shown := escape(current)
	return strings.NewReplacer(
		"/assets/app.css?v="+shown+`"`, "/assets/app.css?v="+capturedVersion+`"`,
		"/assets/app.js?v="+shown+`"`, "/assets/app.js?v="+capturedVersion+`"`,
		">Version</td><td><code>"+shown+"</code>", ">Version</td><td><code>"+capturedVersion+"</code>",
	).Replace(page)
}

// normaliseWeightStep maps the weight field's step back to the Rust server's.
// This build deliberately accepts finer weights than it did.
func normaliseWeightStep(page string) string {
	return strings.ReplaceAll(page, `name="weight" step="`+weightStep+`"`, `name="weight" step="0.25"`)
}

// assertPageGolden compares the digest of the page, with its version and weight
// step mapped back to the Rust server's, with the one captured from it. A
// mismatch writes the page to a temporary file; the Rust golden is in git
// history.
func assertPageGolden(t *testing.T, name, got string) {
	t.Helper()
	want, ok := goldenDigests[name]
	if !ok {
		t.Fatalf("no golden digest named %s", name)
	}
	normalised := normaliseWeightStep(normaliseVersion(got, version.Version()))
	sum := sha256.Sum256([]byte(normalised))
	if hex.EncodeToString(sum[:]) == want {
		return
	}
	file, err := os.CreateTemp("", name+".actual.*.html")
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = file.Close() }()
	if _, err := file.WriteString(normalised); err != nil {
		t.Fatal(err)
	}
	source, ok := fragmentSources[name]
	if !ok {
		source = name
	}
	path := "internal/views/testdata/" + source + ".golden.html"
	t.Errorf("%s differs from the Rust golden (%d bytes of normalised output written to %s).\n"+
		"The golden is in git history: git show $(git log --diff-filter=D --format=%%h -1 -- %s)^:%s",
		name, len(normalised), file.Name(), path, path)
}

func TestNormaliseWeightStepRewritesOnlyTheWeightField(t *testing.T) {
	const page = `<input name="weight" step="0.01"><input name="other" step="0.01">`
	const want = `<input name="weight" step="0.25"><input name="other" step="0.01">`
	if got := normaliseWeightStep(page); got != want {
		t.Errorf("got:\n%s\nwant:\n%s", got, want)
	}
}

func TestNormaliseVersionRewritesOnlyTheVersionStamps(t *testing.T) {
	const page = `<link href="/assets/app.css?v=dev"><script src="/assets/app.js?v=dev"></script>` +
		`<p>development <code>dev</code> ?v=dev</p><tr><td>Version</td><td><code>dev</code></td></tr>`
	const want = `<link href="/assets/app.css?v=` + capturedVersion + `"><script src="/assets/app.js?v=` + capturedVersion + `"></script>` +
		`<p>development <code>dev</code> ?v=dev</p><tr><td>Version</td><td><code>` + capturedVersion + `</code></td></tr>`
	if got := normaliseVersion(page, "dev"); got != want {
		t.Errorf("got:\n%s\nwant:\n%s", got, want)
	}
	if got := normaliseVersion(page, capturedVersion); got != page {
		t.Errorf("the captured version must leave the page alone, got:\n%s", got)
	}
}
