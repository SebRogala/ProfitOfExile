package server

import (
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"testing/fstest"
)

// testFS returns a MapFS with a mock index.html and a test asset file.
func testFS() fstest.MapFS {
	return fstest.MapFS{
		"index.html": &fstest.MapFile{
			Data: []byte("<html><body>ProfitOfExile</body></html>"),
		},
		"assets/style.css": &fstest.MapFile{
			Data: []byte("body { background: #1a1a2e; }"),
		},
	}
}

func TestStaticHandler_RootServesIndexHTML(t *testing.T) {
	handler := StaticHandler(testFS())

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	w := httptest.NewRecorder()

	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET / status = %d, want %d", w.Code, http.StatusOK)
	}

	body, err := io.ReadAll(w.Body)
	if err != nil {
		t.Fatalf("failed to read response body: %v", err)
	}
	if !strings.Contains(string(body), "ProfitOfExile") {
		t.Errorf("GET / body = %q, want it to contain %q", string(body), "ProfitOfExile")
	}

	contentType := w.Header().Get("Content-Type")
	if !strings.Contains(contentType, "text/html") {
		t.Errorf("GET / Content-Type = %q, want text/html", contentType)
	}
}

func TestStaticHandler_ExistingFileServed(t *testing.T) {
	handler := StaticHandler(testFS())

	req := httptest.NewRequest(http.MethodGet, "/assets/style.css", nil)
	w := httptest.NewRecorder()

	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /assets/style.css status = %d, want %d", w.Code, http.StatusOK)
	}

	body, err := io.ReadAll(w.Body)
	if err != nil {
		t.Fatalf("failed to read response body: %v", err)
	}
	if !strings.Contains(string(body), "#1a1a2e") {
		t.Errorf("GET /assets/style.css body = %q, want it to contain %q", string(body), "#1a1a2e")
	}

	contentType := w.Header().Get("Content-Type")
	if !strings.Contains(contentType, "text/css") {
		t.Errorf("GET /assets/style.css Content-Type = %q, want text/css", contentType)
	}
}

func TestStaticHandler_UnknownPathReturnsSPAFallback(t *testing.T) {
	handler := StaticHandler(testFS())

	req := httptest.NewRequest(http.MethodGet, "/some/unknown/route", nil)
	w := httptest.NewRecorder()

	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /some/unknown/route status = %d, want %d", w.Code, http.StatusOK)
	}

	body, err := io.ReadAll(w.Body)
	if err != nil {
		t.Fatalf("failed to read response body: %v", err)
	}
	if !strings.Contains(string(body), "ProfitOfExile") {
		t.Errorf("SPA fallback body = %q, want it to contain %q", string(body), "ProfitOfExile")
	}

	contentType := w.Header().Get("Content-Type")
	if !strings.Contains(contentType, "text/html") {
		t.Errorf("SPA fallback Content-Type = %q, want text/html", contentType)
	}
}

func TestStaticHandler_SPAFallbackForDeepPaths(t *testing.T) {
	handler := StaticHandler(testFS())

	paths := []string{
		"/strategies",
		"/lab/analysis",
		"/settings/profile/edit",
	}

	for _, path := range paths {
		t.Run(path, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodGet, path, nil)
			w := httptest.NewRecorder()

			handler.ServeHTTP(w, req)

			if w.Code != http.StatusOK {
				t.Fatalf("GET %s status = %d, want %d", path, w.Code, http.StatusOK)
			}

			body, err := io.ReadAll(w.Body)
			if err != nil {
				t.Fatalf("failed to read response body: %v", err)
			}
			if !strings.Contains(string(body), "ProfitOfExile") {
				t.Errorf("GET %s body = %q, want SPA fallback with %q", path, string(body), "ProfitOfExile")
			}
		})
	}
}

func TestStaticHandler_MissingIndexReturns404(t *testing.T) {
	// Filesystem with only an asset, no index.html.
	noIndexFS := fstest.MapFS{
		"assets/style.css": &fstest.MapFile{
			Data: []byte("body { color: red; }"),
		},
	}
	handler := StaticHandler(noIndexFS)

	req := httptest.NewRequest(http.MethodGet, "/some/unknown/route", nil)
	w := httptest.NewRecorder()

	handler.ServeHTTP(w, req)

	if w.Code != http.StatusNotFound {
		t.Fatalf("GET /some/unknown/route status = %d, want %d", w.Code, http.StatusNotFound)
	}

	body, err := io.ReadAll(w.Body)
	if err != nil {
		t.Fatalf("failed to read response body: %v", err)
	}
	if !strings.Contains(string(body), "index.html not found") {
		t.Errorf("body = %q, want it to contain %q", string(body), "index.html not found")
	}
}

func TestStaticHandler_PathTraversalHandledSafely(t *testing.T) {
	handler := StaticHandler(testFS())

	paths := []string{
		"/../etc/passwd",
		"/..%2f..%2fetc/passwd",
		"/..",
	}

	for _, path := range paths {
		t.Run(path, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodGet, path, nil)
			w := httptest.NewRecorder()

			handler.ServeHTTP(w, req)

			// Path traversal attempts must not return 500.
			// They should either 404 or fall back to SPA (200 with index.html).
			if w.Code == http.StatusInternalServerError {
				t.Errorf("GET %s returned 500, expected safe handling (200 or 404)", path)
			}
		})
	}
}
