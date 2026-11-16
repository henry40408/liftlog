package main

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestParseDotEnv(t *testing.T) {
	input := "\xef\xbb\xbf" + `# comment

A=1
export B="two words"
C='three'
D=four # trailing
E=
F=a=b
G="quoted" # comment
H='it' #c
I="a # b"
J="x"y
`
	got, err := parseDotEnv(strings.NewReader(input))
	if err != nil {
		t.Fatal(err)
	}
	want := []dotEnvEntry{
		{"A", "1"}, {"B", "two words"}, {"C", "three"}, {"D", "four"}, {"E", ""}, {"F", "a=b"},
		{"G", "quoted"}, {"H", "it"}, {"I", "a # b"}, {"J", `"x"y`},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

func TestParseDotEnvRejectsALineWithoutEquals(t *testing.T) {
	_, err := parseDotEnv(strings.NewReader("A=1\nnonsense\n"))
	if err == nil || !strings.Contains(err.Error(), "line 2") {
		t.Fatalf("err = %v", err)
	}
}

func TestLoadDotEnvKeepsVariablesAlreadySet(t *testing.T) {
	path := filepath.Join(t.TempDir(), ".env")
	if err := os.WriteFile(path, []byte("DOTENV_TEST_SET=file\nDOTENV_TEST_NEW=file\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("DOTENV_TEST_SET", "real")
	t.Setenv("DOTENV_TEST_NEW", "") // registers the cleanup
	_ = os.Unsetenv("DOTENV_TEST_NEW")

	if err := loadDotEnv(path); err != nil {
		t.Fatal(err)
	}
	if got := os.Getenv("DOTENV_TEST_SET"); got != "real" {
		t.Errorf("a set variable became %q", got)
	}
	if got := os.Getenv("DOTENV_TEST_NEW"); got != "file" {
		t.Errorf("an unset variable became %q", got)
	}
}

func TestLoadDotEnvIgnoresAMissingFile(t *testing.T) {
	if err := loadDotEnv(filepath.Join(t.TempDir(), "absent")); err != nil {
		t.Fatal(err)
	}
}
