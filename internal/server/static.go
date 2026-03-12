package server

import (
	"io"
	"io/fs"
	"net/http"
)

// StaticHandler returns an http.Handler that serves static files from the
// given filesystem with SPA fallback: if the requested path does not match
// a real file, index.html is served instead. This allows client-side routing
// to work for any URL that doesn't match an API route.
func StaticHandler(fsys fs.FS) http.Handler {
	fileServer := http.FileServer(http.FS(fsys))

	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// For the root path, let the file server handle it directly.
		path := r.URL.Path
		if path == "/" {
			fileServer.ServeHTTP(w, r)
			return
		}

		// Strip leading slash for fs.Open.
		fsPath := path[1:]
		f, err := fsys.Open(fsPath)
		if err != nil {
			// File not found — serve index.html for SPA fallback.
			serveIndex(w, fsys)
			return
		}
		f.Close()

		fileServer.ServeHTTP(w, r)
	})
}

// serveIndex reads index.html from the filesystem and writes it to the
// response. This enables SPA client-side routing for paths that don't
// correspond to real static files.
func serveIndex(w http.ResponseWriter, fsys fs.FS) {
	f, err := fsys.Open("index.html")
	if err != nil {
		http.Error(w, "index.html not found", http.StatusNotFound)
		return
	}
	defer f.Close()

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	io.Copy(w, f)
}
