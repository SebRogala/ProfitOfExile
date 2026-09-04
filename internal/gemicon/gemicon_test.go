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
	"testing/fstest"

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

// newStubUpstreamPerPath is newStubUpstream with the body keyed on the request
// path, so a test that maps one name to two different URLs can tell WHICH of
// them the cache went and fetched. newStubUpstream serves one payload for every
// path, which cannot distinguish "refetched from the new URL" from "served the
// old bytes back".
func newStubUpstreamPerPath() *stubUpstream {
	s := &stubUpstream{}
	s.server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt64(&s.hits, 1)
		w.Header().Set("Content-Type", "image/png")
		_, _ = w.Write(append(append([]byte{}, fakePNG...), r.URL.Path...))
	}))
	return s
}

// bytesForPath is what newStubUpstreamPerPath answers for path.
func bytesForPath(path string) []byte {
	return append(append([]byte{}, fakePNG...), path...)
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

	// The filename itself is pinned by TestFilePath_isSafeNameDashURLHashPNG;
	// here the production builder is used so this test keeps asserting the
	// BYTES on disk rather than re-stating the scheme.
	onDisk, err := os.ReadFile(c.filePath("Absolution", up.server.URL))
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
	if _, err := os.Stat(c.filePath("Absolution", upstream.URL)); !os.IsNotExist(err) {
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

// pinnedIconURL and pinnedShortHash are the shared vector for the
// cache-filename scheme. The same URL, the same 16 hex characters and the same
// full filename are pinned in scripts/download-gem-icons.py's import-time
// _self_check(), because the seeding path is Python and production only ever
// reads what that script wrote — a scheme that drifts between the two languages
// seeds a volume of files the server never looks for, and ADR-012 says
// production cannot recover by fetching.
const (
	pinnedIconURL   = "https://www.poewiki.net/images/c/c6/Absolution_inventory_icon.png"
	pinnedShortHash = "e2b9dfdb1dd1d6a0"
)

// The golden filename. Every part of the scheme is load-bearing and this is the
// only place all of them are asserted at once: the safe name, the "-" joiner
// (safeFileName never emits one, so the last "-" always starts the hash), the
// 16-hex URL hash and the constant ".png". Change any of them and the seeded
// production volume stops matching what the server reads.
func TestFilePath_isSafeNameDashURLHashPNG(t *testing.T) {
	c := newCache(nil, nil, "/icons-cache/gems")

	got := c.filePath("Absolution", pinnedIconURL)

	want := filepath.Join("/icons-cache/gems", "Absolution-"+pinnedShortHash+".png")
	if got != want {
		t.Errorf("filePath = %q, want %q", got, want)
	}
}

// The hash half of the vector on its own, so a change to the algorithm or the
// truncation length fails here and names itself, rather than only failing the
// golden filename above where it reads as a joiner problem.
func TestShortHash_pinnedVector(t *testing.T) {
	got := shortHash(pinnedIconURL)

	if got != pinnedShortHash {
		t.Errorf("shortHash(%q) = %q, want %q — scripts/download-gem-icons.py pins the same value",
			pinnedIconURL, got, pinnedShortHash)
	}
	// The truncation is checked on a URL the golden value does NOT cover.
	// Asserting len(got) would be dead: got either equals the 16-character
	// pinned vector, in which case the length is already settled, or the check
	// above has already failed. A second, unpinned input is what actually
	// guards "16 hex for every URL" rather than "16 hex for this one".
	const otherURL = "https://example.invalid/other.png"
	if n := len(shortHash(otherURL)); n != 16 {
		t.Errorf("shortHash(%q) returned %d hex characters, want 16", otherURL, n)
	}
}

// The bug this task exists for. Before content-addressing, correcting a URL in
// the map changed nothing: the filename came from the name alone, the disk copy
// was returned unconditionally, and the old artwork was served forever with no
// error and no log. The two Caches over one directory are the deploy that
// carries the corrected map onto the existing cache volume.
func TestHandler_urlChangeForTheSameName_fetchesFreshBytes(t *testing.T) {
	up := newStubUpstreamPerPath()
	defer up.close()
	dir := t.TempDir()
	const wrongPath, correctedPath = "/wrong.png", "/corrected.png"

	before := newCache(map[string]string{"Absolution": up.server.URL + wrongPath}, up.server.Client(), dir)
	if w := serve(before, "Absolution"); w.Code != http.StatusOK ||
		!bytes.Equal(w.Body.Bytes(), bytesForPath(wrongPath)) {
		t.Fatalf("warm request status = %d body = %q, want 200 and the wrong-URL bytes", w.Code, w.Body.Bytes())
	}

	// Map corrected, same cache directory — the deploy after a URL fix.
	after := newCache(map[string]string{"Absolution": up.server.URL + correctedPath}, up.server.Client(), dir)
	w := serve(after, "Absolution")

	if w.Code != http.StatusOK {
		t.Fatalf("status after the URL correction = %d, want 200", w.Code)
	}
	if !bytes.Equal(w.Body.Bytes(), bytesForPath(correctedPath)) {
		t.Errorf("served %q, want the corrected URL's bytes %q — the stale file was served instead",
			w.Body.Bytes(), bytesForPath(correctedPath))
	}
	if up.hitCount() != 2 {
		t.Errorf("upstream hit %d times, want 2 — a corrected URL must miss the cache and refetch", up.hitCount())
	}
}

// The name stays in the filename, so the hash is a tiebreaker and never the
// identity. Two names legitimately share a URL (a Vaal alias pointing at the
// base gem's artwork is the case docs/GEM-ICONS.md describes), and each must
// keep its own file: one shared file would make the cache unreadable as a map
// of what is seeded, and pruning or replacing one name's icon would silently
// take the other's with it.
func TestHandler_twoNamesOneURL_writeTwoDistinctFiles(t *testing.T) {
	up := newStubUpstream()
	defer up.close()
	dir := t.TempDir()
	c := newCache(map[string]string{
		"Absolution":      up.server.URL,
		"Vaal Absolution": up.server.URL,
	}, up.server.Client(), dir)

	for _, name := range []string{"Absolution", "Vaal%20Absolution"} {
		if code := serve(c, name).Code; code != http.StatusOK {
			t.Fatalf("GET %s status = %d, want 200", name, code)
		}
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatalf("read cache dir: %v", err)
	}
	if len(entries) != 2 {
		t.Fatalf("cache dir holds %d files, want 2 — the two names collapsed onto one file", len(entries))
	}
	for _, name := range []string{"Absolution", "Vaal Absolution"} {
		if _, err := os.Stat(c.filePath(name, up.server.URL)); err != nil {
			t.Errorf("no cache file for %q: %v", name, err)
		}
	}
	if up.hitCount() != 2 {
		t.Errorf("upstream hit %d times, want 2 — each name owns its own cache file", up.hitCount())
	}
}

// urlFile is a category map as it appears on disk, for the fstest fixtures
// below. The loader only ever unmarshals the bytes, so the value shape is what
// matters, not the key names.
func urlFile(entries string) *fstest.MapFile {
	return &fstest.MapFile{Data: []byte(entries)}
}

// The point of discovering the files instead of naming them: a category the
// loader has never heard of — "alva" here — is picked up with no code change,
// and every file's entries reach the merged map.
func TestLoadURLMap_discoversEveryJSONFileInTheDirectory(t *testing.T) {
	fsys := fstest.MapFS{
		"urls/gems.json":  urlFile(`{"Absolution": "https://example.invalid/abs.png"}`),
		"urls/items.json": urlFile(`{"Gift to the Goddess": "https://example.invalid/gift.png"}`),
		"urls/alva.json":  urlFile(`{"Chamber of Sins": "https://example.invalid/sins.png"}`),
	}

	urls, err := loadURLMap(fsys, "urls")
	if err != nil {
		t.Fatalf("loadURLMap() error = %v, want nil", err)
	}

	want := map[string]string{
		"Absolution":          "https://example.invalid/abs.png",
		"Gift to the Goddess": "https://example.invalid/gift.png",
		"Chamber of Sins":     "https://example.invalid/sins.png",
	}
	if len(urls) != len(want) {
		t.Fatalf("merged map holds %d entries, want %d — a category file was not read", len(urls), len(want))
	}
	for key, wantURL := range want {
		if got := urls[key]; got != wantURL {
			t.Errorf("merged map[%q] = %q, want %q", key, got, wantURL)
		}
	}
}

// A silent last-writer-wins merge is the one real hazard of a flat runtime map:
// the loser's name would serve the winner's artwork with nothing to show for
// it. Both file names and the key are in the message because that is what makes
// the failure actionable — which of the two files to edit is the whole question.
func TestLoadURLMap_duplicateKeyAcrossFiles_failsNamingBothFilesAndTheKey(t *testing.T) {
	fsys := fstest.MapFS{
		"urls/a.json": urlFile(`{"Shared Name": "https://example.invalid/first.png"}`),
		"urls/b.json": urlFile(`{"Shared Name": "https://example.invalid/second.png"}`),
	}

	urls, err := loadURLMap(fsys, "urls")

	if err == nil {
		t.Fatalf("loadURLMap() error = nil, want a duplicate-key rejection (map = %v)", urls)
	}
	if urls != nil {
		t.Errorf("loadURLMap returned a map alongside the error: %v", urls)
	}
	for _, want := range []string{"Shared Name", "urls/a.json", "urls/b.json"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("error %q does not name %q", err, want)
		}
	}
}

func TestLoadURLMap_malformedFile_failsNamingTheFile(t *testing.T) {
	fsys := fstest.MapFS{
		"urls/gems.json":   urlFile(`{"Absolution": "https://example.invalid/abs.png"}`),
		"urls/broken.json": urlFile(`{"Absolution": `),
	}

	urls, err := loadURLMap(fsys, "urls")

	if err == nil {
		t.Fatalf("loadURLMap() error = nil, want a parse rejection (map = %v)", urls)
	}
	if urls != nil {
		t.Errorf("loadURLMap returned a map alongside the error: %v", urls)
	}
	if !strings.Contains(err.Error(), "urls/broken.json") {
		t.Errorf("error %q does not name the file that failed to parse", err)
	}
}

// Boundary: a directory with no category map in it. The Cache this would build
// answers 404 for every name, so it is a broken build and has to fail at
// construction rather than at the first render.
func TestLoadURLMap_noJSONFilesMatched_fails(t *testing.T) {
	fsys := fstest.MapFS{"urls/README.md": urlFile("not a map")}

	urls, err := loadURLMap(fsys, "urls")

	if err == nil {
		t.Fatalf("loadURLMap() error = nil, want a rejection (map = %v)", urls)
	}
	if urls != nil {
		t.Errorf("loadURLMap returned a map alongside the error: %v", urls)
	}
	if !strings.Contains(err.Error(), "no url map files matched") {
		t.Errorf("error %q does not report the no-files-matched cause", err)
	}
}

// Boundary: every matched file parses but none of them carries an entry — e.g.
// a category file was created and left `{}`. This is distinct from no files
// matching at all: the glob succeeded, so the failure has to name the emptier
// cause or it reads as the wrong boundary.
func TestLoadURLMap_matchedFilesHoldNoEntries_fails(t *testing.T) {
	fsys := fstest.MapFS{"urls/empty.json": urlFile(`{}`)}

	urls, err := loadURLMap(fsys, "urls")

	if err == nil {
		t.Fatalf("loadURLMap() error = nil, want a rejection (map = %v)", urls)
	}
	if urls != nil {
		t.Errorf("loadURLMap returned a map alongside the error: %v", urls)
	}
	if !strings.Contains(err.Error(), "hold no entries") {
		t.Errorf("error %q does not report the zero-entries cause", err)
	}
}

// The glob is *.json, not *: a note or a scratch file sitting beside the
// category maps must not be handed to the JSON parser.
func TestLoadURLMap_ignoresNonJSONFiles(t *testing.T) {
	fsys := fstest.MapFS{
		"urls/gems.json": urlFile(`{"Absolution": "https://example.invalid/abs.png"}`),
		"urls/README.md": urlFile("Category files live here; one per category."),
	}

	urls, err := loadURLMap(fsys, "urls")
	if err != nil {
		t.Fatalf("loadURLMap() error = %v, want nil — a non-JSON neighbour was read", err)
	}
	if len(urls) != 1 {
		t.Errorf("merged map holds %d entries, want 1", len(urls))
	}
}

// The embedded categories, end to end: every entry of every file reaches the
// map New builds, which is the property the split had to preserve.
func TestNew_embeddedMap_holdsEveryCategorysEntries(t *testing.T) {
	c, err := New(t.TempDir())
	if err != nil {
		t.Fatalf("New() error = %v, want nil", err)
	}

	// The merged size the single pre-split file held. A category file dropped
	// from the embed, or read and discarded, lands here.
	if got, want := len(c.urls), 765; got != want {
		t.Errorf("embedded map holds %d entries, want %d", got, want)
	}
	if got := c.urls["Absolution"]; got == "" {
		t.Error("embedded map missing a URL for a known gem \"Absolution\" (gems.json)")
	}
	if got := c.urls["Gift to the Goddess"]; got == "" {
		t.Error("embedded map missing a URL for a known offering \"Gift to the Goddess\" (items.json)")
	}
}

func TestNew_createsTheCacheDirectory(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "nested", "icons")

	if _, err := New(dir); err != nil {
		t.Fatalf("New() error = %v, want nil", err)
	}

	if info, err := os.Stat(dir); err != nil || !info.IsDir() {
		t.Errorf("New() must create the cache dir; stat err = %v", err)
	}
}

// The two lab offerings are items the GEM endpoint has to answer for, because
// MarketOverview routes offering names through it. Which file they live in is
// the categorisation this split exists for, and nothing about the merged map
// would reveal it moving back.
func TestEmbeddedCategories_theLabOfferingsAreInItemsAndNotInGems(t *testing.T) {
	offerings := []string{"Gift to the Goddess", "Dedication to the Goddess"}

	items := readEmbeddedCategory(t, "urls/items.json")
	gems := readEmbeddedCategory(t, "urls/gems.json")

	for _, name := range offerings {
		if _, ok := items[name]; !ok {
			t.Errorf("urls/items.json is missing %q", name)
		}
		if _, ok := gems[name]; ok {
			t.Errorf("urls/gems.json carries %q, which is an item, not a gem", name)
		}
	}
}

func readEmbeddedCategory(t *testing.T, path string) map[string]string {
	t.Helper()
	raw, err := urlFiles.ReadFile(path)
	if err != nil {
		t.Fatalf("read embedded %s: %v", path, err)
	}
	var m map[string]string
	if err := json.Unmarshal(raw, &m); err != nil {
		t.Fatalf("parse embedded %s: %v", path, err)
	}
	return m
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

	if err := os.Remove(c.filePath("Absolution", up.server.URL)); err != nil {
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
// serve one set's artwork under the other's name, but only when they also share
// a source URL. So an empty dir is an unconfigured caller, not a request for a
// default.
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
