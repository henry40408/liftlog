package chart

import (
	"math"
	"strconv"
	"strings"
)

// Display formats f like Rust's `{}` for f64: the shortest digits that
// round-trip, never in exponent form (100 stays "100").
func Display(f float64) string {
	switch {
	case math.IsInf(f, 1):
		return "inf"
	case math.IsInf(f, -1):
		return "-inf"
	case math.IsNaN(f):
		return "NaN"
	}
	return strconv.FormatFloat(f, 'f', -1, 64)
}

// jsonFloat formats f like serde_json (ryu): shortest round-trip digits, always
// with a fractional part ("100.0"), exponent form only for very large or very
// small magnitudes.
func jsonFloat(f float64) string {
	if math.IsInf(f, 0) || math.IsNaN(f) {
		return "null"
	}
	if f == 0 {
		if math.Signbit(f) {
			return "-0.0"
		}
		return "0.0"
	}
	sign := ""
	if f < 0 {
		sign, f = "-", -f
	}
	mantissa, exp := shortestDigits(f)
	return sign + layoutDigits(mantissa, exp)
}

// shortestDigits returns the decimal digits of f without a decimal point and
// the exponent e such that f = 0.DIGITS * 10^e.
func shortestDigits(f float64) (string, int) {
	s := strconv.FormatFloat(f, 'e', -1, 64)
	mantissa, expPart, _ := strings.Cut(s, "e")
	exp, _ := strconv.Atoi(expPart)
	return strings.Replace(mantissa, ".", "", 1), exp + 1
}

// layoutDigits places the decimal point the way ryu does: plain notation while
// the value is in [1e-5, 1e16), otherwise `d.ddde<exp>`.
func layoutDigits(digits string, exp int) string {
	n := len(digits)
	switch {
	case exp > 0 && exp <= 16 && n <= exp:
		return digits + strings.Repeat("0", exp-n) + ".0"
	case exp > 0 && exp <= 16:
		return digits[:exp] + "." + digits[exp:]
	case exp > -5 && exp <= 0:
		return "0." + strings.Repeat("0", -exp) + digits
	}
	sci := digits[:1]
	if n > 1 {
		sci += "." + digits[1:]
	}
	return sci + "e" + strconv.Itoa(exp-1)
}
