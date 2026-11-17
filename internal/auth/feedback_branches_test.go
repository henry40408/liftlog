package auth

import (
	"testing"

	"github.com/henry40408/liftlog/internal/zxcvbn/match"
)

func TestPasswordRankWarningBuckets(t *testing.T) {
	cases := []struct {
		rank int
		want string
	}{
		{1, warningTop10},
		{10, warningTop10},
		{11, warningTop100},
		{100, warningTop100},
		{101, warningCommonPassword},
	}
	for _, tc := range cases {
		if got := passwordRankWarning(tc.rank); got != tc.want {
			t.Errorf("rank %d: %q, want %q", tc.rank, got, tc.want)
		}
	}
}

func TestDictionaryFeedbackByDictionary(t *testing.T) {
	cases := []struct {
		name string
		m    match.Match
		sole bool
		want string
	}{
		{"empty token has no feedback", match.Match{DictionaryName: "passwords"}, true, ""},
		{"sole top password uses its rank", match.Match{Token: "password", DictionaryName: "passwords", Rank: 2}, true, warningTop10},
		{"sole common password", match.Match{Token: "football", DictionaryName: "passwords", Rank: 400}, true, warningCommonPassword},
		{"a password inside a longer one is merely similar", match.Match{Token: "password", DictionaryName: "passwords", Rank: 2}, false, warningSimilarToCommon},
		{"l33t password is similar", match.Match{Token: "p4ssword", DictionaryName: "passwords", Rank: 2, L33t: true}, true, warningSimilarToCommon},
		{"reversed password is similar", match.Match{Token: "drowssap", DictionaryName: "passwords", Rank: 2, Reversed: true}, true, warningSimilarToCommon},
		{"sole english word", match.Match{Token: "giraffe", DictionaryName: "english_wikipedia"}, true, warningWordByItself},
		{"english word among others has no warning", match.Match{Token: "giraffe", DictionaryName: "english_wikipedia"}, false, ""},
		{"sole name", match.Match{Token: "henry", DictionaryName: "male_names"}, true, warningNamesByThemselves},
		{"name among others", match.Match{Token: "henry", DictionaryName: "surnames"}, false, warningCommonNames},
		{"unknown dictionary has no warning", match.Match{Token: "word", DictionaryName: "other"}, true, ""},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := dictionaryFeedback(&tc.m, tc.sole).Warning; got != tc.want {
				t.Errorf("warning = %q, want %q", got, tc.want)
			}
		})
	}
}

func TestDictionarySuggestionsFollowTheTokenShape(t *testing.T) {
	cases := []struct {
		name string
		m    match.Match
		want []string
	}{
		{"capitalised", match.Match{Token: "Giraffe"}, []string{suggestionCapitalization}},
		{"all upper gets the capitalization hint first", match.Match{Token: "GIRAFFE"}, []string{suggestionCapitalization}},
		{"lower case needs none", match.Match{Token: "giraffe"}, nil},
		{"reversed long word", match.Match{Token: "effarig", Reversed: true}, []string{suggestionReversedWords}},
		{"reversed word of exactly four letters", match.Match{Token: "abcd", Reversed: true}, []string{suggestionReversedWords}},
		{"reversed short word is not worth a hint", match.Match{Token: "abc", Reversed: true}, nil},
		{"l33t", match.Match{Token: "g1raffe", L33t: true}, []string{suggestionSubstitutions}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := dictionarySuggestions(&tc.m)
			if len(got) != len(tc.want) {
				t.Fatalf("suggestions = %v, want %v", got, tc.want)
			}
			for i := range got {
				if got[i] != tc.want[i] {
					t.Errorf("suggestions = %v, want %v", got, tc.want)
				}
			}
		})
	}
}
