package gemicon

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
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

// serveConditional is serve with an If-None-Match validator attached.
func serveConditional(c *Cache, encodedName, ifNoneMatch string) *httptest.ResponseRecorder {
	router := chi.NewRouter()
	router.Get("/api/gem-icon/{name}", c.Handler())
	req := httptest.NewRequest(http.MethodGet, "/api/gem-icon/"+encodedName, nil)
	req.Header.Set("If-None-Match", ifNoneMatch)
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)
	return w
}

// strongETag is the strong, quoted SHA-256 ETag the handler is expected to
// produce for body, computed independently of the production helper.
func strongETag(body []byte) string {
	sum := sha256.Sum256(body)
	return `"` + hex.EncodeToString(sum[:]) + `"`
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

// The point of the ETag memo is that a conditional request costs no server work.
// This test removes everything the handler could do work *with* — the disk copy
// is deleted and the upstream is shut down — so a 304 is only reachable if the
// handler answered from the memo alone. Restore the pre-memo ordering (hash the
// body, then compare) and this fails with 502, because load() would have to
// refetch from a closed upstream.
func TestHandler_ConditionalRequestAnsweredWithoutReadingOrFetching(t *testing.T) {
	up := newStubUpstream()
	dir := t.TempDir()
	c := newCache(map[string]string{"Absolution": up.server.URL}, up.server.Client(), dir)

	warm := serve(c, "Absolution")
	if warm.Code != http.StatusOK {
		t.Fatalf("warm request status = %d, want 200", warm.Code)
	}
	etag := warm.Header().Get("ETag")
	if etag == "" {
		t.Fatal("warm response carried no ETag to revalidate with")
	}

	if err := os.Remove(filepath.Join(dir, "Absolution.png")); err != nil {
		t.Fatalf("remove disk copy: %v", err)
	}
	up.close()

	w := serveConditional(c, "Absolution", etag)

	if w.Code != http.StatusNotModified {
		t.Fatalf("status = %d, want %d — the conditional request did work it should not have (body: %q)",
			w.Code, http.StatusNotModified, w.Body.String())
	}
	if w.Body.Len() != 0 {
		t.Errorf("304 body = %q, want empty", w.Body.String())
	}
	if up.hitCount() != 1 {
		t.Errorf("upstream hit %d times, want 1 — the 304 path must not fetch", up.hitCount())
	}
}

// The 304 must carry the same caching contract as the 200 it replaces, or a
// client that revalidates loses its Cache-Control and comes back next render.
func TestHandler_NotModifiedCarriesSameETagAndCacheControlAs200(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	c := newCache(map[string]string{"Absolution": up.server.URL}, up.server.Client(), t.TempDir())

	warm := serve(c, "Absolution")
	if warm.Code != http.StatusOK {
		t.Fatalf("warm request status = %d, want 200", warm.Code)
	}

	w := serveConditional(c, "Absolution", warm.Header().Get("ETag"))

	if w.Code != http.StatusNotModified {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusNotModified)
	}
	if got := w.Header().Get("ETag"); got != warm.Header().Get("ETag") {
		t.Errorf("304 ETag = %q, want %q (same validator as the 200)", got, warm.Header().Get("ETag"))
	}
	if got := w.Header().Get("Cache-Control"); got != cacheControl {
		t.Errorf("304 Cache-Control = %q, want %q", got, cacheControl)
	}
	if got := w.Header().Get("Content-Type"); got != "image/png" {
		t.Errorf("304 Content-Type = %q, want image/png", got)
	}
}

// Memoising must not weaken the validator: the ETag served from the memo has to
// remain the strong SHA-256 of the bytes on the wire. Return a constant, a
// truncated hash, or a weak validator from the memo and this fails.
func TestHandler_MemoisedETagIsStillTheStrongHashOfTheServedBytes(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	c := newCache(map[string]string{"Absolution": up.server.URL}, up.server.Client(), t.TempDir())

	want := strongETag(fakePNG)

	first := serve(c, "Absolution")
	if got := first.Header().Get("ETag"); got != want {
		t.Fatalf("first ETag = %q, want %q", got, want)
	}

	// Second 200 is served with the memoised value rather than a fresh hash.
	second := serve(c, "Absolution")
	if second.Code != http.StatusOK {
		t.Fatalf("second status = %d, want 200", second.Code)
	}
	if got := second.Header().Get("ETag"); got != want {
		t.Errorf("memoised ETag = %q, want %q — the memo returned a different validator", got, want)
	}
	if !bytes.Equal(second.Body.Bytes(), fakePNG) {
		t.Errorf("second body = %q, want the icon bytes the ETag describes", second.Body.Bytes())
	}
}

// A client holding a validator that no longer matches must get the bytes, not a
// 304. Short-circuiting on "we have a memo" rather than "the memo matches" would
// leave that client rendering nothing.
func TestHandler_StaleValidatorServesBodyWith200(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	c := newCache(map[string]string{"Absolution": up.server.URL}, up.server.Client(), t.TempDir())

	if code := serve(c, "Absolution").Code; code != http.StatusOK {
		t.Fatalf("warm request status = %d, want 200", code)
	}

	w := serveConditional(c, "Absolution", `"0000000000000000000000000000000000000000000000000000000000000000"`)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 for a non-matching If-None-Match", w.Code)
	}
	if !bytes.Equal(w.Body.Bytes(), fakePNG) {
		t.Errorf("body = %q, want the icon bytes", w.Body.Bytes())
	}
	if got := w.Header().Get("ETag"); got != strongETag(fakePNG) {
		t.Errorf("ETag = %q, want the current strong validator %q", got, strongETag(fakePNG))
	}
}

// A 404 must carry an explicit no-store. Leaving it bare is not neutral: an
// unknown-gem 404 with no freshness information is heuristically cacheable, and
// production showed clients holding onto one across the deploy that added the
// icon — the gem rendered "?" until a hard reload. Delete the header and this
// fails; widen it to any positive TTL and the second assertion fails.
func TestHandler_UnknownGem404IsNotCacheableByClients(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	c := newCache(map[string]string{"Absolution": up.server.URL}, up.server.Client(), t.TempDir())

	w := serve(c, "Nonexistent%20Gem")

	if w.Code != http.StatusNotFound {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusNotFound)
	}
	if got := w.Header().Get("Cache-Control"); got != "no-store" {
		t.Errorf("404 Cache-Control = %q, want %q — a bare 404 is heuristically cacheable", got, "no-store")
	}
	if got := w.Header().Get("Cache-Control"); strings.Contains(got, "max-age") {
		t.Errorf("404 Cache-Control = %q, want no lifetime at all — any TTL outlives the deploy that adds the icon", got)
	}
}

// errorRecords drains buf as newline-delimited slog JSON and returns the records
// logged at ERROR. Decoding beats substring matching here: an assertion on
// rec["url"] fails when the attribute is missing, whereas a substring assertion
// on the whole line also passes when the URL merely appears inside the error
// text, which is where it already was before this attribute existed.
func errorRecords(t *testing.T, buf *bytes.Buffer) []map[string]any {
	t.Helper()
	var out []map[string]any
	for _, line := range strings.Split(strings.TrimSpace(buf.String()), "\n") {
		if line == "" {
			continue
		}
		var rec map[string]any
		if err := json.Unmarshal([]byte(line), &rec); err != nil {
			t.Fatalf("log line is not slog JSON: %v (line: %q)", err, line)
		}
		if rec["level"] == "ERROR" {
			out = append(out, rec)
		}
	}
	return out
}

// captureLogs points the default logger at a buffer for the duration of the
// test. The default logger is the seam because the handler logs through the
// package-level slog, as the disk-write failure path in load already does.
func captureLogs(t *testing.T) *bytes.Buffer {
	t.Helper()
	buf := &bytes.Buffer{}
	prev := slog.Default()
	slog.SetDefault(slog.New(slog.NewJSONHandler(buf, nil)))
	t.Cleanup(func() { slog.SetDefault(prev) })
	return buf
}

// A 502 is the only response on this route that breaks the render, and it is
// invisible to the client as a distinct condition: GemIcon flips to "?" on the
// <img> error event for a 404 and a 502 alike. So the server log is the only
// place the difference can be recovered, and it has to carry enough to act on —
// which gem, which upstream URL, and why. ADR-012 is why the URL matters: a map
// entry deployed ahead of its cache volume 502s forever (poewiki 403s the
// production VPS), and only the URL separates that from a transient blip.
//
// Drop any one of the three attributes and this fails naming the one dropped.
func TestHandler_UpstreamFailureIsLoggedWithGemURLAndCause(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer upstream.Close()
	c := newCache(map[string]string{"Absolution": upstream.URL}, upstream.Client(), t.TempDir())
	logs := captureLogs(t)

	if code := serve(c, "Absolution").Code; code != http.StatusBadGateway {
		t.Fatalf("status = %d, want %d", code, http.StatusBadGateway)
	}

	records := errorRecords(t, logs)
	if len(records) != 1 {
		t.Fatalf("logged %d ERROR records for a 502, want exactly 1 — the failure that breaks the render must not be silent (log: %q)",
			len(records), logs.String())
	}
	rec := records[0]
	if got := rec["gem"]; got != "Absolution" {
		t.Errorf("logged gem = %v, want %q — the log must name which icon failed", got, "Absolution")
	}
	if got := rec["url"]; got != upstream.URL {
		t.Errorf("logged url = %v, want %q — without the resolved URL an unseeded map entry is indistinguishable from an upstream blip", got, upstream.URL)
	}
	// The upstream status is the cause; a log that says "it failed" without it
	// sends the reader back to reproducing the request by hand.
	if got, _ := rec["error"].(string); !strings.Contains(got, "500") {
		t.Errorf("logged error = %q, want it to carry the upstream status 500", got)
	}
}

// The 502 path exists so a later request can retry (nothing is written to disk).
// Caching it would defeat that, so no Cache-Control may leak onto the failure
// response — the reason the success headers are set only after load() returns.
func TestHandler_UpstreamFailure502IsNotCacheable(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer upstream.Close()
	c := newCache(map[string]string{"Absolution": upstream.URL}, upstream.Client(), t.TempDir())

	w := serve(c, "Absolution")

	if w.Code != http.StatusBadGateway {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusBadGateway)
	}
	if got := w.Header().Get("Cache-Control"); got != "" {
		t.Errorf("502 Cache-Control = %q, want no caching directive at all", got)
	}
	if got := w.Header().Get("ETag"); got != "" {
		t.Errorf("502 ETag = %q, want none — there are no bytes to validate", got)
	}
}

// chaosIconKey and divineIconKey are currency-exchange keys: whole feed metadata
// ids, slashes and all. They are what NewWithMap has to serve that the gem map
// never asked for — the route parameter arrives percent-encoded and the map
// lookup happens on the decoded id.
const (
	chaosIconKey  = "Metadata/Items/Currency/CurrencyRerollRare"
	divineIconKey = "Metadata/Items/Currency/CurrencyModValues"
)

// pathBodyUpstream serves the request path as the response body, so a test can
// tell WHICH map entry the handler resolved rather than only that it fetched
// something.
func pathBodyUpstream(t *testing.T) *httptest.Server {
	t.Helper()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "image/png")
		_, _ = w.Write([]byte("image-for" + r.URL.Path))
	}))
	t.Cleanup(server.Close)
	return server
}

// serveWith routes a GET through the handler under the currency-exchange route
// pattern, which is where a key with encoded slashes actually arrives.
func serveWith(c *Cache, pattern, encodedName string) *httptest.ResponseRecorder {
	router := chi.NewRouter()
	router.Get(pattern, c.Handler())
	req := httptest.NewRequest(http.MethodGet, strings.Replace(pattern, "{name}", encodedName, 1), nil)
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)
	return w
}

// NewWithMap is what makes this cache reusable for a second icon set. The map is
// the caller's, so the key that resolves must be the caller's too — with two
// entries in play, serving the wrong one is the failure this catches.
func TestNewWithMap_ServesTheRequestedKeyFromTheSuppliedMap(t *testing.T) {
	upstream := pathBodyUpstream(t)
	c, err := NewWithMap(map[string]string{
		chaosIconKey:  upstream.URL + "/chaos",
		divineIconKey: upstream.URL + "/divine",
	}, t.TempDir())
	if err != nil {
		t.Fatalf("NewWithMap() error = %v, want nil", err)
	}

	w := serveWith(c, "/api/currency-exchange/icon/{name}", url.PathEscape(divineIconKey))

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d (body: %q)", w.Code, http.StatusOK, w.Body.String())
	}
	if got, want := w.Body.String(), "image-for/divine"; got != want {
		t.Errorf("body = %q, want %q — the handler resolved the wrong map entry", got, want)
	}
	if got := w.Header().Get("Content-Type"); got != "image/png" {
		t.Errorf("Content-Type = %q, want image/png", got)
	}
}

// A feed id the asset has no icon for is never in the map, and that request must
// cost nothing: no upstream fetch, no cache file.
func TestNewWithMap_KeyOutsideTheSuppliedMapReturns404WithoutFetching(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	dir := t.TempDir()
	c, err := NewWithMap(map[string]string{chaosIconKey: up.server.URL}, dir)
	if err != nil {
		t.Fatalf("NewWithMap() error = %v, want nil", err)
	}

	w := serveWith(c, "/api/currency-exchange/icon/{name}", url.PathEscape("Metadata/Items/Currency/CurrencyNotInTheAssetYet"))

	if w.Code != http.StatusNotFound {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusNotFound)
	}
	if up.hitCount() != 0 {
		t.Errorf("upstream hit %d times for a key outside the map, want 0", up.hitCount())
	}
	entries, _ := os.ReadDir(dir)
	if len(entries) != 0 {
		t.Errorf("cache dir holds %d files after an unknown-key request, want 0", len(entries))
	}
}

// Each map needs its OWN directory: two maps sharing one would share the
// filename scheme, and any pair of keys reducing to the same safeFileName would
// serve one set's artwork under the other's name. So an empty dir is an
// unconfigured caller, not a request for a default.
func TestNewWithMap_EmptyCacheDirIsRejected(t *testing.T) {
	c, err := NewWithMap(map[string]string{chaosIconKey: "https://example.invalid/icon.png"}, "")

	if err == nil {
		t.Fatalf("NewWithMap(_, \"\") error = nil, want a rejection (cache = %+v)", c)
	}
	if c != nil {
		t.Errorf("NewWithMap returned a cache alongside an error: %+v", c)
	}
}

// New is held to the same rule as NewWithMap since POE-221: this package holds
// no default directory at all, because any default it held would be the GEM
// set's and a second map could silently be handed it. The caller derives one
// sub-directory per map from its configured root. Reintroduce a fallback inside
// New and this fails on the nil error.
func TestNew_EmptyCacheDirIsRejectedRatherThanDefaulted(t *testing.T) {
	c, err := New("")

	if err == nil {
		t.Fatalf("New(\"\") error = nil, want a rejection (cache = %+v)", c)
	}
	if c != nil {
		t.Errorf("New returned a cache alongside an error: %+v", c)
	}
}
