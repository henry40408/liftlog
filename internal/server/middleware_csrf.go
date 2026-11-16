package server

import "net/http"

// csrfGuard rejects cross-site state-changing requests with an empty 403.
// Sec-Fetch-Site decides when present (only same-origin and none pass); else
// the Origin authority, port included, must equal Host. Safe methods and
// header-less clients pass. With SameSite=Lax cookies this is the whole CSRF
// defence, so keep the port check: cookies ignore ports.
func (a *App) csrfGuard(next http.Handler) http.Handler {
	protection := http.NewCrossOriginProtection()
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := protection.Check(r); err != nil {
			a.audit.CSRFRejected(a.requestInfo(r), csrfReason(r), r.Method, r.Header.Get("Origin"))
			w.WriteHeader(http.StatusForbidden)
			return
		}
		next.ServeHTTP(w, r)
	})
}

// csrfReason tells operators whether the browser declared the request
// cross-site, or the Origin/Host comparison failed (suspect the proxy first).
func csrfReason(r *http.Request) string {
	if r.Header.Get("Sec-Fetch-Site") != "" {
		return "sec_fetch_site"
	}
	return "origin_fallback"
}
