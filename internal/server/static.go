package server

import (
	"errors"
	"io/fs"
	"log/slog"
	"net/http"
	"path"
)

// StaticHandler returns an http.Handler that serves static files from the
// given filesystem with SPA fallback: if the requested path does not match
// a real file, index.html is served instead. This allows client-side routing
// to work for any URL that doesn't match an API route.
func StaticHandler(fsys fs.FS) http.Handler {
	fileServer := http.FileServer(http.FS(fsys))

	// Read index.html into memory once so SPA fallback responses are atomic
	// (no partial writes) and we fail fast if the file is missing.
	indexHTML, indexErr := fs.ReadFile(fsys, "index.html")
	if indexErr != nil {
		slog.Warn("index.html not found in frontend filesystem; SPA fallback will return 404", "error", indexErr)
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
			// File not found — serve index.html for SPA fallback.
			serveIndex(w, indexHTML)
			return
		}

		fileServer.ServeHTTP(w, r)
	})
}

// serveIndex writes the pre-loaded index.html content to the response.
// This enables SPA client-side routing for paths that don't correspond
// to real static files.
func serveIndex(w http.ResponseWriter, indexHTML []byte) {
	if indexHTML == nil {
		http.Error(w, "index.html not found", http.StatusNotFound)
		return
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-cache")
	w.Write(indexHTML)
}
