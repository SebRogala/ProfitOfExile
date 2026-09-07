package server

import (
	"errors"
	"io/fs"
	"log/slog"
	"net/http"
	"path"
)

// spaFallback is the client-side shell SvelteKit writes for routes it does not
// prerender (adapter-static `fallback` in frontend/svelte.config.js). It is
// deliberately not index.html: that file is the prerendered landing page, and
// a fallback of the same name overwrote it with the empty shell at build time.
const spaFallback = "200.html"

// StaticHandler returns an http.Handler that serves static files from the
// given filesystem with SPA fallback: if the requested path does not match
// a real file, the SPA shell (spaFallback) is served instead. This allows
// client-side routing to work for any URL that doesn't match an API route,
// while "/" and any other prerendered page are served as the real files
// they are.
func StaticHandler(fsys fs.FS) http.Handler {
	fileServer := http.FileServer(http.FS(fsys))

	// Read the shell into memory once so SPA fallback responses are atomic
	// (no partial writes) and we fail fast if the file is missing.
	indexHTML, indexErr := fs.ReadFile(fsys, spaFallback)
	if indexErr != nil {
		slog.Warn("SPA shell not found in frontend filesystem; SPA fallback will return 404", "file", spaFallback, "error", indexErr)
	}

	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// For the root path, let the file server handle it directly.
		urlPath := r.URL.Path
		if urlPath == "/" {
			fileServer.ServeHTTP(w, r)
			return
		}

		// Strip leading slash and clean the path for fs.Stat.
		fsPath := path.Clean(urlPath[1:])
		if _, err := fs.Stat(fsys, fsPath); err != nil {
			if !errors.Is(err, fs.ErrNotExist) {
				slog.Error("unexpected error checking static file", "path", fsPath, "error", err)
				http.Error(w, "Internal Server Error", http.StatusInternalServerError)
				return
			}
			// File not found — serve the SPA shell for client-side routing.
			serveIndex(w, indexHTML)
			return
		}

		fileServer.ServeHTTP(w, r)
	})
}

// serveIndex writes the pre-loaded SPA shell to the response.
// This enables SPA client-side routing for paths that don't correspond
// to real static files.
func serveIndex(w http.ResponseWriter, indexHTML []byte) {
	if indexHTML == nil {
		http.Error(w, spaFallback+" not found", http.StatusNotFound)
		return
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-cache")
	w.Write(indexHTML)
}
