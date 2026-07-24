package gemicon

import (
	"bytes"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"sync/atomic"
	"testing"

	"github.com/go-chi/chi/v5"
)

// fakePNG is a minimal byte payload standing in for an icon image. The leading
// bytes are the real PNG magic number.
var fakePNG = []byte("\x89PNG\r\n\x1a\nfake-gem-icon-bytes")

// stubUpstream is an httptest server that serves fakePNG and counts how many
// times it was hit, so tests can prove the disk cache prevents further fetches.
type stubUpstream struct {
	server *httptest.Server
	hits   int64
}

func newStubUpstream() *stubUpstream {
	s := &stubUpstream{}
	s.server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt64(&s.hits, 1)
		w.Header().Set("Content-Type", "image/png")
		_, _ = w.Write(fakePNG)
	}))
	return s
}

func (s *stubUpstream) hitCount() int64 { return atomic.LoadInt64(&s.hits) }
func (s *stubUpstream) close()          { s.server.Close() }

// serve routes a GET for the given percent-encoded name through the handler.
func serve(c *Cache, encodedName string) *httptest.ResponseRecorder {
	router := chi.NewRouter()
	router.Get("/api/gem-icon/{name}", c.Handler())
	req := httptest.NewRequest(http.MethodGet, "/api/gem-icon/"+encodedName, nil)
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)
	return w
}

func TestHandler_KnownGemServesImageBytesWithImmutableCacheControl(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	c := newCache(map[string]string{"Absolution": up.server.URL}, up.server.Client(), t.TempDir())

	w := serve(c, "Absolution")

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusOK)
	}
	if !bytes.Equal(w.Body.Bytes(), fakePNG) {
		t.Errorf("body = %q, want the upstream image bytes %q", w.Body.Bytes(), fakePNG)
	}
	if got := w.Header().Get("Content-Type"); got != "image/png" {
		t.Errorf("Content-Type = %q, want image/png", got)
	}
	if got := w.Header().Get("Cache-Control"); got != cacheControl {
		t.Errorf("Cache-Control = %q, want %q", got, cacheControl)
	}
}

func TestHandler_FirstRequestWritesIconToCacheDir(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	dir := t.TempDir()
	c := newCache(map[string]string{"Absolution": up.server.URL}, up.server.Client(), dir)

	if code := serve(c, "Absolution").Code; code != http.StatusOK {
		t.Fatalf("status = %d, want 200", code)
	}

	onDisk, err := os.ReadFile(filepath.Join(dir, "Absolution.png"))
	if err != nil {
		t.Fatalf("expected the icon persisted to the cache dir: %v", err)
	}
	if !bytes.Equal(onDisk, fakePNG) {
		t.Errorf("cached file = %q, want the fetched bytes %q", onDisk, fakePNG)
	}
}

func TestHandler_SpacedGemNameIsDecodedBeforeLookup(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	c := newCache(map[string]string{"Added Chaos Damage Support": up.server.URL}, up.server.Client(), t.TempDir())

	// Client sends the name percent-encoded, as encodeURIComponent would.
	w := serve(c, "Added%20Chaos%20Damage%20Support")

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d (name must be decoded before the map lookup)", w.Code, http.StatusOK)
	}
	if !bytes.Equal(w.Body.Bytes(), fakePNG) {
		t.Errorf("body = %q, want the upstream image bytes", w.Body.Bytes())
	}
}

func TestHandler_UnknownGemReturns404WithoutFetchingOrWriting(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	dir := t.TempDir()
	c := newCache(map[string]string{"Absolution": up.server.URL}, up.server.Client(), dir)

	w := serve(c, "Nonexistent%20Gem")

	if w.Code != http.StatusNotFound {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusNotFound)
	}
	if up.hitCount() != 0 {
		t.Errorf("upstream hit %d times for an unknown gem, want 0", up.hitCount())
	}
	entries, _ := os.ReadDir(dir)
	if len(entries) != 0 {
		t.Errorf("cache dir has %d files after an unknown-gem request, want 0", len(entries))
	}
}

func TestHandler_SecondRequestServedFromDiskNotUpstream(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	c := newCache(map[string]string{"Absolution": up.server.URL}, up.server.Client(), t.TempDir())

	if code := serve(c, "Absolution").Code; code != http.StatusOK {
		t.Fatalf("first request status = %d, want 200", code)
	}

	second := serve(c, "Absolution")
	if second.Code != http.StatusOK {
		t.Fatalf("second request status = %d, want 200", second.Code)
	}
	if !bytes.Equal(second.Body.Bytes(), fakePNG) {
		t.Errorf("second request body = %q, want the cached image bytes", second.Body.Bytes())
	}
	if up.hitCount() != 1 {
		t.Errorf("upstream hit %d times across two requests, want 1", up.hitCount())
	}
}

// A fresh Cache pointed at the same directory simulates a server restart: the
// persistent disk copy must be served with no upstream fetch at all.
func TestHandler_FreshInstanceServesPersistedIconWithoutUpstream(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	dir := t.TempDir()

	// First instance populates the disk cache (1 upstream hit).
	warm := newCache(map[string]string{"Absolution": up.server.URL}, up.server.Client(), dir)
	if code := serve(warm, "Absolution").Code; code != http.StatusOK {
		t.Fatalf("warm request status = %d, want 200", code)
	}

	// Restart: brand-new instance, same directory, empty in-memory state.
	restarted := newCache(map[string]string{"Absolution": up.server.URL}, up.server.Client(), dir)
	w := serve(restarted, "Absolution")

	if w.Code != http.StatusOK {
		t.Fatalf("post-restart status = %d, want 200", w.Code)
	}
	if !bytes.Equal(w.Body.Bytes(), fakePNG) {
		t.Errorf("post-restart body = %q, want the persisted bytes", w.Body.Bytes())
	}
	if up.hitCount() != 1 {
		t.Errorf("upstream hit %d times total, want 1 (restart must serve from disk)", up.hitCount())
	}
}

func TestHandler_UpstreamErrorReturns502(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer upstream.Close()
	c := newCache(map[string]string{"Absolution": upstream.URL}, upstream.Client(), t.TempDir())

	if code := serve(c, "Absolution").Code; code != http.StatusBadGateway {
		t.Fatalf("status = %d, want %d for an upstream failure", code, http.StatusBadGateway)
	}
}

func TestHandler_UpstreamFailureIsNotCachedSoRetrySucceeds(t *testing.T) {
	var fail atomic.Bool
	fail.Store(true)
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if fail.Load() {
			w.WriteHeader(http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "image/png")
		_, _ = w.Write(fakePNG)
	}))
	defer upstream.Close()
	dir := t.TempDir()
	c := newCache(map[string]string{"Absolution": upstream.URL}, upstream.Client(), dir)

	if code := serve(c, "Absolution").Code; code != http.StatusBadGateway {
		t.Fatalf("first (failing) request status = %d, want %d", code, http.StatusBadGateway)
	}
	if _, err := os.Stat(filepath.Join(dir, "Absolution.png")); !os.IsNotExist(err) {
		t.Fatalf("a failed fetch must not write a cache file, stat err = %v", err)
	}

	// Upstream recovers; the earlier failure must not have been cached.
	fail.Store(false)
	retry := serve(c, "Absolution")
	if retry.Code != http.StatusOK {
		t.Fatalf("retry status = %d, want 200 (a failed fetch must not be cached)", retry.Code)
	}
	if !bytes.Equal(retry.Body.Bytes(), fakePNG) {
		t.Errorf("retry body = %q, want the recovered image bytes", retry.Body.Bytes())
	}
}

func TestNew_ParsesEmbeddedURLMapAndCreatesCacheDir(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "nested", "icons")
	c, err := New(dir)
	if err != nil {
		t.Fatalf("New() error = %v, want nil", err)
	}
	if got := c.urls["Absolution"]; got == "" {
		t.Error("embedded map missing a URL for a known gem \"Absolution\"")
	}
	if info, err := os.Stat(dir); err != nil || !info.IsDir() {
		t.Errorf("New() must create the cache dir; stat err = %v", err)
	}
}
