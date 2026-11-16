package auth

import (
	"unicode"
	"unicode/utf8"

	"github.com/henry40408/liftlog/internal/zxcvbn/match"
)

// The Go zxcvbn library reports score and matches only. The warning and
// suggestion texts, and the choice between them, are ported from the Rust
// zxcvbn 3.1.1 feedback module so users see the same prose as before.

const (
	warningStraightRows       = "Straight rows of keys are easy to guess."
	warningShortKeyboard      = "Short keyboard patterns are easy to guess."
	warningRepeatsAaa         = `Repeats like "aaa" are easy to guess.`
	warningRepeatsAbcAbc      = `Repeats like "abcabcabc" are only slightly harder to guess than "abc".`
	warningTop10              = "This is a top-10 common password."
	warningTop100             = "This is a top-100 common password."
	warningCommonPassword     = "This is a very common password."
	warningSimilarToCommon    = "This is similar to a commonly used password."
	warningSequences          = "Sequences like abc or 6543 are easy to guess."
	warningRecentYears        = "Recent years are easy to guess."
	warningWordByItself       = "A word by itself is easy to guess."
	warningDates              = "Dates are often easy to guess."
	warningNamesByThemselves  = "Names and surnames by themselves are easy to guess."
	warningCommonNames        = "Common names and surnames are easy to guess."
	suggestionAddAnotherWord  = "Add another word or two. Uncommon words are better."
	suggestionCapitalization  = "Capitalization doesn't help very much."
	suggestionAllUppercase    = "All-uppercase is almost as easy to guess as all-lowercase."
	suggestionReversedWords   = "Reversed words aren't much harder to guess."
	suggestionSubstitutions   = "Predictable substitutions like '@' instead of 'a' don't help very much."
	suggestionLongerKeyboard  = "Use a longer keyboard pattern with more turns."
	suggestionAvoidRepeats    = "Avoid repeated words and characters."
	suggestionAvoidSequences  = "Avoid sequences."
	suggestionAvoidRecent     = "Avoid recent years."
	suggestionAvoidYears      = "Avoid years that are associated with you."
	suggestionAvoidDates      = "Avoid dates and years that are associated with you."
	suggestionFewWords        = "Use a few words, avoid common phrases."
	suggestionNoNeedForSymbol = "No need for symbols, digits, or uppercase letters."
)

// Feedback helps the user choose a better password. Warning explains what is
// wrong and may be empty.
type Feedback struct {
	Warning     string
	Suggestions []string
}

// feedbackFor is the feedback for a password that scored below the threshold,
// given zxcvbn's best match sequence.
func feedbackFor(sequence []*match.Match) Feedback {
	if len(sequence) == 0 {
		return Feedback{Suggestions: []string{suggestionFewWords, suggestionNoNeedForSymbol}}
	}
	feedback := matchFeedback(longestMatch(sequence), len(sequence) == 1)
	feedback.Suggestions = append([]string{suggestionAddAnotherWord}, feedback.Suggestions...)
	return feedback
}

// longestMatch keeps the last of equally long matches, as Rust's max_by_key
// does.
func longestMatch(sequence []*match.Match) *match.Match {
	longest := sequence[0]
	for _, m := range sequence[1:] {
		if utf8.RuneCountInString(m.Token) >= utf8.RuneCountInString(longest.Token) {
			longest = m
		}
	}
	return longest
}

func matchFeedback(m *match.Match, soleMatch bool) Feedback {
	switch m.Pattern {
	case "dictionary":
		return dictionaryFeedback(m, soleMatch)
	case "spatial":
		warning := warningShortKeyboard
		if m.Turns == 1 {
			warning = warningStraightRows
		}
		return Feedback{Warning: warning, Suggestions: []string{suggestionLongerKeyboard}}
	case "repeat":
		warning := warningRepeatsAbcAbc
		if utf8.RuneCountInString(m.BaseToken) == 1 {
			warning = warningRepeatsAaa
		}
		return Feedback{Warning: warning, Suggestions: []string{suggestionAvoidRepeats}}
	case "sequence":
		return Feedback{Warning: warningSequences, Suggestions: []string{suggestionAvoidSequences}}
	case "regex":
		if m.RegexName != "recent_year" {
			return Feedback{}
		}
		return Feedback{
			Warning:     warningRecentYears,
			Suggestions: []string{suggestionAvoidRecent, suggestionAvoidYears},
		}
	case "date":
		return Feedback{Warning: warningDates, Suggestions: []string{suggestionAvoidDates}}
	}
	return Feedback{}
}

func dictionaryFeedback(m *match.Match, soleMatch bool) Feedback {
	if m.Token == "" {
		return Feedback{}
	}
	return Feedback{
		Warning:     dictionaryWarning(m, soleMatch),
		Suggestions: dictionarySuggestions(m),
	}
}

func dictionaryWarning(m *match.Match, soleMatch bool) string {
	switch m.DictionaryName {
	case "passwords":
		if !soleMatch || m.L33t || m.Reversed {
			return warningSimilarToCommon
		}
		return passwordRankWarning(m.Rank)
	case "english_wikipedia":
		if soleMatch {
			return warningWordByItself
		}
	case "surnames", "female_names", "male_names":
		if soleMatch {
			return warningNamesByThemselves
		}
		return warningCommonNames
	}
	return ""
}

func passwordRankWarning(rank int) string {
	switch {
	case rank <= 10:
		return warningTop10
	case rank <= 100:
		return warningTop100
	}
	return warningCommonPassword
}

func dictionarySuggestions(m *match.Match) []string {
	var suggestions []string
	first, _ := utf8.DecodeRuneInString(m.Token)
	if unicode.IsUpper(first) {
		suggestions = append(suggestions, suggestionCapitalization)
	} else if allUpper(m.Token) {
		suggestions = append(suggestions, suggestionAllUppercase)
	}
	if m.Reversed && utf8.RuneCountInString(m.Token) >= 4 {
		suggestions = append(suggestions, suggestionReversedWords)
	}
	if m.L33t {
		suggestions = append(suggestions, suggestionSubstitutions)
	}
	return suggestions
}

func allUpper(token string) bool {
	for _, r := range token {
		if !unicode.IsUpper(r) {
			return false
		}
	}
	return true
}
