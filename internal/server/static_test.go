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

	body, _ := io.ReadAll(w.Body)
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

	body, _ := io.ReadAll(w.Body)
	if !strings.Contains(string(body), "#1a1a2e") {
		t.Errorf("GET /assets/style.css body = %q, want it to contain %q", string(body), "#1a1a2e")
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

	body, _ := io.ReadAll(w.Body)
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

			body, _ := io.ReadAll(w.Body)
			if !strings.Contains(string(body), "ProfitOfExile") {
				t.Errorf("GET %s body = %q, want SPA fallback with %q", path, string(body), "ProfitOfExile")
			}
		})
	}
}
