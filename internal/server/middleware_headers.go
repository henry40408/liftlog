package server

import (
	"net/http"
	"strconv"
)

// hstsValue is the Strict-Transport-Security value, empty when HSTS is off.
func (a *App) hstsValue() string {
	if a.cfg.HSTSMaxAge == 0 {
		return ""
	}
	value := "max-age=" + strconv.FormatUint(a.cfg.HSTSMaxAge, 10)
	if a.cfg.HSTSIncludeSubdomains {
		value += "; includeSubDomains"
	}
	return value
}

// securityHeaders is the outermost layer, so even a CSRF 403 or a 404 carries
// the headers. The CSP only blocks framing, because the pages use inline
// script and style.
func (a *App) securityHeaders(next http.Handler) http.Handler {
	hsts := a.hstsValue()
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		header := w.Header()
		if hsts != "" {
			header.Set("Strict-Transport-Security", hsts)
		}
		header.Set("Content-Security-Policy", "frame-ancestors 'none'")
		header.Set("X-Frame-Options", "DENY")
		header.Set("X-Content-Type-Options", "nosniff")
		header.Set("Referrer-Policy", "strict-origin-when-cross-origin")
		next.ServeHTTP(w, r)
	})
}
