package assets

import (
	"bytes"
	"os"
	"strings"
	"testing"
)

func TestEmbeddedFilesAreNotEmpty(t *testing.T) {
	for name, size := range map[string]int{
		"AppCSS":            len(AppCSS),
		"AppJS":             len(AppJS),
		"FaviconSVG":        len(FaviconSVG),
		"AppleTouchIconPNG": len(AppleTouchIconPNG),
	} {
		if size == 0 {
			t.Errorf("%s is empty", name)
		}
	}
}

func TestAppleTouchIconIsAPNG(t *testing.T) {
	if !bytes.HasPrefix(AppleTouchIconPNG, []byte("\x89PNG\r\n\x1a\n")) {
		t.Error("apple-touch-icon.png has no PNG signature")
	}
}

func TestFaviconIsAnSVG(t *testing.T) {
	if !strings.Contains(FaviconSVG, "<svg") {
		t.Error("favicon.svg has no <svg element")
	}
}

// The Rust sources stay in assets/ until the rewrite replaces them; the copies
// must not drift meanwhile.
func TestCopiesMatchTheRustAssets(t *testing.T) {
	for name, got := range map[string]string{
		"app.css":     AppCSS,
		"app.js":      AppJS,
		"favicon.svg": FaviconSVG,
	} {
		want, err := os.ReadFile("../../assets/" + name)
		if os.IsNotExist(err) {
			t.Skip("root assets/ already removed")
		}
		if err != nil {
			t.Fatal(err)
		}
		if got != string(want) {
			t.Errorf("%s differs from assets/%s", name, name)
		}
	}
}
