// Package assets embeds the hand-written front-end files.
package assets

import "embed"

//go:embed static/app.css static/app.js static/favicon.svg static/apple-touch-icon.png
var files embed.FS

// CacheControl is safe because pages request the CSS and JS with
// ?v=<build version>, so a new build gets a new URL.
const CacheControl = "public, max-age=86400"

var (
	// AppCSS is the stylesheet.
	AppCSS = readText("app.css")
	// AppJS is the script; pages load it blocking because inline page scripts
	// use `window.LiftLog`.
	AppJS = readText("app.js")
	// FaviconSVG is the SVG favicon.
	FaviconSVG = readText("favicon.svg")
	// AppleTouchIconPNG is separate from the favicon because iOS does not
	// accept SVG for the home screen.
	AppleTouchIconPNG = read("apple-touch-icon.png")
)

func read(name string) []byte {
	data, err := files.ReadFile("static/" + name)
	if err != nil {
		panic("assets: embedded file missing: " + err.Error())
	}
	return data
}

func readText(name string) string { return string(read(name)) }
