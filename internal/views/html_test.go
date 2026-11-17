package views

import "testing"

func TestEscapeReplacesOnlyAmpersandBracketsAndDoubleQuote(t *testing.T) {
	if got := escape(`a&b<c>d"e'f+g`); got != "a&amp;b&lt;c&gt;d&quot;e'f+g" {
		t.Fatalf("got %q", got)
	}
}

func TestElRendersAttributesChildrenAndEscapes(t *testing.T) {
	got := el("a", []attr{a("href", `/x?a=1&b="2"`), flag("download")}, text("<hi>"), el("b", nil))
	want := node(`<a href="/x?a=1&amp;b=&quot;2&quot;" download>&lt;hi&gt;<b></b></a>`)
	if got != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestVoidHasNoClosingTag(t *testing.T) {
	if got := void("meta", a("charset", "UTF-8")); got != `<meta charset="UTF-8">` {
		t.Errorf("got %s", got)
	}
}

func TestGroupAddsNoWhitespace(t *testing.T) {
	if got := group(text("a"), text("b")); got != "ab" {
		t.Errorf("got %q", got)
	}
}
