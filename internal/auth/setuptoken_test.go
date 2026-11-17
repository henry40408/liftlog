package auth

import (
	"strings"
	"testing"
)

func TestGeneratedTokenHasTwoGroupsFromTheAlphabet(t *testing.T) {
	token := GenerateSetupToken()
	first, second, ok := strings.Cut(token, "-")
	if !ok {
		t.Fatalf("no dash separator in %q", token)
	}
	for _, group := range []string{first, second} {
		if len(group) != tokenGroupLen {
			t.Fatalf("group %q has the wrong length", group)
		}
		if strings.Trim(group, tokenAlphabet) != "" {
			t.Fatalf("got %q", token)
		}
	}
}

func TestGeneratedTokensDiffer(t *testing.T) {
	first, second := GenerateSetupToken(), GenerateSetupToken()
	if first == second {
		t.Fatal("two generated tokens are equal")
	}
}

func TestVerifyAcceptsTheExactToken(t *testing.T) {
	if !NewSetupTokenWithValue("ABCDE-FGHJK").Verify("ABCDE-FGHJK") {
		t.Fatal("exact token refused")
	}
}

func TestVerifyIgnoresDashCaseAndWhitespace(t *testing.T) {
	token := NewSetupTokenWithValue("ABCDE-FGHJK")
	for _, submitted := range []string{"abcdefghjk", "  abcde - fghjk \n"} {
		if !token.Verify(submitted) {
			t.Errorf("%q refused", submitted)
		}
	}
}

func TestVerifyRejectsWrongEmptyAndTruncatedTokens(t *testing.T) {
	token := NewSetupTokenWithValue("ABCDE-FGHJK")
	for _, submitted := range []string{"ABCDE-FGHJM", "", "ABCDE"} {
		if token.Verify(submitted) {
			t.Errorf("%q accepted", submitted)
		}
	}
}

func TestNoTokenIsIssuedWhenUsersExist(t *testing.T) {
	token := NewSetupToken(1)
	if _, ok := token.Current(); ok {
		t.Fatal("a token is active")
	}
	if token.Verify("") || token.Verify("ABCDE-FGHJK") {
		t.Fatal("an inactive token verified")
	}
}

func TestTokenIsIssuedOnAnEmptyInstall(t *testing.T) {
	if _, ok := NewSetupToken(0).Current(); !ok {
		t.Fatal("no token on an empty install")
	}
}

func TestClearInvalidatesTheToken(t *testing.T) {
	token := NewSetupTokenWithValue("ABCDE-FGHJK")
	token.Clear()
	if token.Verify("ABCDE-FGHJK") {
		t.Fatal("a cleared token verified")
	}
}
