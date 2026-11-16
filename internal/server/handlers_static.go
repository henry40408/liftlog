package server

import (
	"encoding/json"
	"net/http"
	"strconv"

	"github.com/henry40408/liftlog/internal/assets"
	"github.com/henry40408/liftlog/internal/version"
)

// staticAsset serves a build-time constant. The pages request the CSS and JS
// with ?v=<version>, so a new build gets a new URL and the day of caching is
// safe.
func staticAsset(contentType string, body []byte) http.HandlerFunc {
	return func(w http.ResponseWriter, _ *http.Request) {
		header := w.Header()
		header.Set("Content-Type", contentType)
		header.Set("Cache-Control", assets.CacheControl)
		header.Set("Content-Length", strconv.Itoa(len(body)))
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(body)
	}
}

type healthResponse struct {
	Status     string `json:"status"`
	GitVersion string `json:"git_version"`
}

func healthCheck(w http.ResponseWriter, _ *http.Request) {
	body, err := json.Marshal(healthResponse{Status: "ok", GitVersion: version.Version()})
	if err != nil {
		writeText(w, http.StatusInternalServerError, "Internal error")
		return
	}
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(body)
}

func (a *App) staticRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /health", healthCheck)
	mux.HandleFunc("GET /assets/app.css", staticAsset("text/css; charset=utf-8", []byte(assets.AppCSS)))
	mux.HandleFunc("GET /assets/app.js", staticAsset("text/javascript; charset=utf-8", []byte(assets.AppJS)))
	mux.HandleFunc("GET /favicon.svg", staticAsset("image/svg+xml", []byte(assets.FaviconSVG)))
	mux.HandleFunc("GET /apple-touch-icon.png", staticAsset("image/png", assets.AppleTouchIconPNG))
}
