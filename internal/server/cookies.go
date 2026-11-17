package server

import (
	"net/http"

	"github.com/henry40408/liftlog/internal/session"
)

func (a *App) setSessionCookie(w http.ResponseWriter, token string) {
	http.SetCookie(w, session.NewCookie(token, a.cfg.CookieSecure))
}

func (a *App) clearSessionCookie(w http.ResponseWriter) {
	http.SetCookie(w, session.RemovalCookie(a.cfg.CookieSecure))
}
