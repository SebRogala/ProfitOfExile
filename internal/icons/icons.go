// Package icons serves Path of Exile artwork to both the web and desktop
// clients through a persistent, on-disk icon cache.
//
// It backs the typed /api/icon/{type}/{name} route and the installed-client
// compatibility alias /api/gem-icon/{name}. Each source map gets its own Cache
// and on-disk sub-directory; the alias uses a merged name→type index.
//
// The correct poewiki image URL is not constructible from a display name or a
// metadata id, so name→URL maps are embedded from urls/*.json. On the first
// request for an icon the upstream image is fetched once and written to a
// persistent on-disk cache; subsequent requests, including after restart, are
// served from disk. Clients additionally cache aggressively via a long immutable
// Cache-Control header.
//
// Only URLs live in the repo (the embedded map). The fetched image bytes are
// never committed — they are copyrighted and heavy — so the cache directory is
// git-ignored. In production the cache directory must be a persistent volume;
// otherwise every redeploy starts with an empty cache and re-fetches.
package icons

import (
	"context"
	"crypto/sha256"
	"embed"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"log/slog"
	"net/http"
	"net/url"
	"os"
	"path"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/go-chi/chi/v5"
)

// urlFiles carries the source name→URL maps, split by category, one file per
// category. NewSets gives each file its own Cache and merges the files only to
// build the compatibility alias's name→type index.
//
// Embedding (rather than reading the directory at runtime) is ADR-012: the icon
// set stays versioned with the binary that serves it, and production — which
// cannot fetch from poewiki at all — never depends on a file being present next
// to the binary.
//
//go:embed urls/*.json
var urlFiles embed.FS

// urlsDir is the directory inside urlFiles the category files live in. Adding a
// category is one new *.json file there and no change here: urlMapFiles
// discovers the files rather than naming them.
const urlsDir = "urls"

// maxImageBytes bounds a single upstream fetch. Inventory icons are a few KB;
// this cap guards against a misbehaving or unexpected upstream response.
const maxImageBytes = 5 << 20 // 5 MiB

// cacheControl instructs browsers and webviews to cache an icon for a year and
// never revalidate. The upstream URL is stable per file name (MediaWiki hashes
// the name, not the bytes), so a cached icon is treated as immutable; this is
// what stops re-downloads on every app open.
const cacheControl = "public, max-age=31536000, immutable"

// notFoundCacheControl stops clients from caching an unknown-icon 404 at all.
//
// This is an explicit "do not keep this" rather than an absence of headers. A
// 404 with no freshness information is heuristically cacheable under RFC 7234,
// so browsers were already caching it on their own terms — measured on
// production, where the 404 for a then-missing gem carried no Cache-Control,
// no Expires and no Last-Modified. The symptom that produced was gems stuck on
// the "?" fallback until a hard reload, for names the server had since started
// answering with 200.
//
// The trade is deliberately the opposite of aggressive caching. Adding an icon
// requires a redeploy (the name→URL map is //go:embed-compiled; see ADR-012),
// but the redeploy restarts the *server*, not the client's HTTP cache — so any
// positive TTL here converts "icon added" into "icon added, invisible to
// existing clients until their copy expires", with no signal that anything is
// wrong. What that would buy back is one map miss per render, and the 404 path
// returns above before touching the disk, the upstream, or the ETag memo. Paying
// the cheapest request the handler serves to keep a silent, hard-to-diagnose
// staleness bug off the table is the right side of that trade.
const notFoundCacheControl = "no-store"

// unsafeFileChars matches any run of characters that must not appear in a cache
// filename. Gem names contain only letters, digits, spaces, apostrophes and
// hyphens, so collapsing these runs to "_" is collision-free for the current
// map and, more importantly, prevents path traversal from a name value.
var unsafeFileChars = regexp.MustCompile(`[^A-Za-z0-9]+`)

// Cache resolves one icon category's keys to poewiki icons and persists fetched
// bytes to disk.
type Cache struct {
	urls   map[string]string
	client *http.Client
	dir    string

	mu    sync.Mutex
	locks map[string]*sync.Mutex

	// etags memoises the strong ETag per icon key so a conditional request can
	// be answered without reading the file or hashing it again. See etagOf.
	etagMu sync.RWMutex
	etags  map[string]string
}

// Sets is the registry of typed icon caches and the compatibility alias index.
// Its maps are immutable after server construction except through Add, which is
// intended for startup wiring before the handlers are registered.
type Sets struct {
	caches   map[string]*Cache
	nameType map[string]string
}

// NewSets builds one Cache per embedded category map at root/<category>. A bad
// category is reported while the other categories continue constructing, so a
// malformed map does not take down every icon surface. The returned Sets is
// non-nil whenever the root and map directory could be inspected; callers can
// register the typed handler for the successfully built categories even when
// err is non-nil.
func NewSets(root string) (*Sets, error) {
	return newSets(root, urlFiles, urlsDir)
}

type categoryMap struct {
	typeName string
	file     string
	urls     map[string]string
}

func newSets(root string, fsys fs.FS, dir string) (*Sets, error) {
	if root == "" {
		return nil, errors.New("icons: cache root is required")
	}

	files, err := urlMapFiles(fsys, dir)
	if err != nil {
		return nil, err
	}

	sets := &Sets{
		caches:   make(map[string]*Cache, len(files)),
		nameType: make(map[string]string),
	}
	var errs []error
	parts := make([]categoryMap, 0, len(files))
	for _, file := range files {
		typeName := strings.TrimSuffix(path.Base(file), path.Ext(file))
		part, err := readURLMap(fsys, file)
		if err != nil {
			slog.Error("icons: icon set disabled", "type", typeName, "map", file, "error", err)
			errs = append(errs, fmt.Errorf("icons: set %s: %w", typeName, err))
			continue
		}
		if len(part) == 0 {
			err = fmt.Errorf("icons: set %s map %s holds no entries", typeName, file)
			slog.Error("icons: icon set disabled", "type", typeName, "map", file, "error", err)
			errs = append(errs, err)
			continue
		}

		cache, err := NewWithMap(part, filepath.Join(root, typeName))
		if err != nil {
			slog.Error("icons: icon set disabled", "type", typeName, "map", file, "error", err)
			errs = append(errs, fmt.Errorf("icons: set %s: %w", typeName, err))
			continue
		}
		sets.caches[typeName] = cache
		parts = append(parts, categoryMap{typeName: typeName, file: file, urls: part})
	}

	merged, mergeErr := mergeCategoryMaps(parts, dir)
	if mergeErr != nil {
		// Return the detail to the router constructor, which logs the aggregate
		// initialization error once alongside the cache root.
		errs = append(errs, mergeErr)
	}
	// mergeCategoryMaps omits names that occur in more than one category. Those
	// are the only entries not eligible for the alias; every other merged name
	// keeps the type of the category file that supplied it.
	for _, part := range parts {
		for name := range part.urls {
			if _, ok := merged[name]; ok {
				sets.nameType[name] = part.typeName
			}
		}
	}

	return sets, errors.Join(errs...)
}

// Add registers a caller-owned map, such as the currency-exchange asset, under
// root/<typeName>. Caller-owned maps are typed-only: the compatibility alias is
// deliberately built from the embedded category files so it remains a gem/item/
// temple compatibility surface and never becomes a second CX route.
func (s *Sets) Add(typeName string, urls map[string]string, root string) error {
	if s == nil {
		return errors.New("icons: cannot add to nil sets")
	}
	if typeName == "" {
		return errors.New("icons: icon type is required")
	}
	if root == "" {
		return errors.New("icons: cache root is required")
	}

	cache, err := NewWithMap(urls, filepath.Join(root, typeName))
	if err != nil {
		return err
	}
	s.caches[typeName] = cache
	return nil
}

// Handler serves GET /api/icon/{type}/{name}. An unknown type is a client bug,
// so it gets the same no-store 404 as an unknown name.
func (s *Sets) Handler() http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		typeName := chi.URLParam(r, "type")
		cache, ok := s.caches[typeName]
		if !ok {
			writeNotFound(w, r)
			return
		}
		cache.serveHTTP(w, r)
	}
}

// AliasHandler serves the installed-desktop compatibility route
// GET /api/gem-icon/{name}. It resolves the name to one category and then uses
// that category's normal Cache handler, so alias and typed responses share all
// cache, ETag and error behaviour.
func (s *Sets) AliasHandler() http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		name := chi.URLParam(r, "name")
		if decoded, err := url.PathUnescape(name); err == nil {
			name = decoded
		}
		typeName, ok := s.nameType[name]
		if !ok {
			writeNotFound(w, r)
			return
		}
		cache, ok := s.caches[typeName]
		if !ok {
			writeNotFound(w, r)
			return
		}
		cache.serveHTTP(w, r)
	}
}

func writeNotFound(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Cache-Control", notFoundCacheControl)
	http.NotFound(w, r)
}

func urlMapFiles(fsys fs.FS, dir string) ([]string, error) {
	files, err := fs.Glob(fsys, dir+"/*.json")
	if err != nil {
		return nil, fmt.Errorf("icons: list url maps in %s: %w", dir, err)
	}
	if len(files) == 0 {
		return nil, fmt.Errorf("icons: no url map files matched %s/*.json", dir)
	}
	return files, nil
}

func readURLMap(fsys fs.FS, file string) (map[string]string, error) {
	raw, err := fs.ReadFile(fsys, file)
	if err != nil {
		return nil, fmt.Errorf("icons: read url map %s: %w", file, err)
	}
	var urls map[string]string
	if err := json.Unmarshal(raw, &urls); err != nil {
		return nil, fmt.Errorf("icons: parse url map %s: %w", file, err)
	}
	return urls, nil
}

// mergeCategoryMaps returns every non-conflicting name and an error for each
// cross-file duplicate. A conflicted name is omitted from the alias index, but
// the other names remain usable; typed caches do not depend on this merge.
func mergeCategoryMaps(parts []categoryMap, dir string) (map[string]string, error) {
	urls := make(map[string]string)
	sourceFile := make(map[string]string)
	conflicts := make(map[string]bool)
	var errs []error
	for _, part := range parts {
		keys := make([]string, 0, len(part.urls))
		for key := range part.urls {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		for _, key := range keys {
			if first, dup := sourceFile[key]; dup {
				if !conflicts[key] {
					delete(urls, key)
					conflicts[key] = true
				}
				errs = append(errs, fmt.Errorf("icons: duplicate icon key %q in %s and %s", key, first, part.file))
				continue
			}
			sourceFile[key] = part.file
			urls[key] = part.urls[key]
		}
	}
	if len(urls) == 0 {
		errs = append(errs, fmt.Errorf("icons: url maps under %s hold no entries", dir))
	}
	return urls, errors.Join(errs...)
}

// NewWithMap builds a Cache over an arbitrary key→upstream-URL map, ensuring
// cacheDir exists.
//
// This is what makes the cache reusable for a second icon set — currency
// exchange items pass exchange.IconURLs() and their own directory (POE-177) —
// rather than the gem map being the only thing it can serve. Everything the
// Cache does is map-agnostic already: the key is whatever the route's {name}
// parameter carries, and safeFileName reduces it to a cache filename.
//
// Each map needs its OWN directory. Two maps sharing one directory would share
// the filename scheme too, so any pair of keys that reduce to the same
// safeFileName — across the two maps, where neither generator can see the
// other's keys — would serve one set's artwork under the other's name, but only
// when they also share a source URL. cacheDir is therefore required: an empty
// one is an unconfigured caller, not a request for a default. Since POE-221
// those directories are sub-directories of one configured cache root, which is
// what lets production mount a single volume without reintroducing the
// collision.
func NewWithMap(urls map[string]string, cacheDir string) (*Cache, error) {
	if cacheDir == "" {
		return nil, errors.New("icons: cache dir is required")
	}
	if err := os.MkdirAll(cacheDir, 0o755); err != nil {
		return nil, fmt.Errorf("icons: create cache dir %q: %w", cacheDir, err)
	}
	return newCache(urls, &http.Client{Timeout: 10 * time.Second}, cacheDir), nil
}

// newCache is the shared constructor used by NewWithMap and by tests that inject
// a stub upstream and a scratch directory.
func newCache(urls map[string]string, client *http.Client, dir string) *Cache {
	return &Cache{
		urls:   urls,
		client: client,
		dir:    dir,
		locks:  make(map[string]*sync.Mutex),
		etags:  make(map[string]string),
	}
}

// Handler serves one category's {name} request. Sets.Handler supplies the typed
// route parameters, while Sets.AliasHandler supplies the compatibility route.
// Unknown names yield 404 (the client renders its "?" fallback); upstream
// failures yield 502 and write nothing to disk, so a later request can retry.
func (c *Cache) Handler() http.HandlerFunc {
	return c.serveHTTP
}

func (c *Cache) serveHTTP(w http.ResponseWriter, r *http.Request) {
	name := chi.URLParam(r, "name")
	// chi routes on the raw (still percent-encoded) path, so a name with
	// spaces arrives as "Added%20Chaos...". Decode before the map lookup.
	if decoded, err := url.PathUnescape(name); err == nil {
		name = decoded
	}

	srcURL, ok := c.urls[name]
	if !ok {
		// Set before NotFound: without an explicit directive a 404 is
		// heuristically cacheable, and a client that cached one keeps
		// rendering "?" after the deploy that adds the icon.
		w.Header().Set("Cache-Control", notFoundCacheControl)
		http.NotFound(w, r)
		return
	}

	// A conditional request whose validator we already know is answered with
	// no disk read and no hashing at all. This is the whole point of the
	// memo: before it, a 304 cost exactly as much as a 200.
	if etag, known := c.etagOf(name); known && r.Header.Get("If-None-Match") == etag {
		writeIconHeaders(w, etag)
		w.WriteHeader(http.StatusNotModified)
		return
	}

	body, err := c.load(r.Context(), name, srcURL)
	if err != nil {
		// This is the only failure on this route that breaks the render, and
		// it is the one the client cannot tell apart from a 404: GemIcon
		// flips to "?" on the <img> error event either way. Nothing else
		// records it — a failed fetch writes no file and caches no marker —
		// so without this line a 502 leaves no trace anywhere.
		//
		// srcURL is logged because the two causes need different fixes and
		// only the URL separates them: a transient upstream blip versus a
		// map entry deployed ahead of its cache volume. ADR-012 makes the
		// second live — poewiki 403s the production VPS, so an unseeded name
		// fails here on every request, forever, until the volume is seeded.
		slog.Error("icons: serve icon failed",
			"gem", name, "url", srcURL, "error", err)
		// Headers are deliberately set only after load succeeds. A 502 must
		// stay uncacheable so a later request can retry (see load).
		http.Error(w, "gem icon unavailable", http.StatusBadGateway)
		return
	}

	// Hash once per gem per process. Subsequent 200s reuse the memo: the
	// bytes still have to be read because they go on the wire, but the
	// SHA-256 does not have to be recomputed.
	etag, known := c.etagOf(name)
	if !known {
		etag = etagFor(body)
		c.rememberETag(name, etag)
	}
	writeIconHeaders(w, etag)
	_, _ = w.Write(body)
}

// writeIconHeaders sets the headers common to a 200 and a 304. Every source is a
// poewiki "*_inventory_icon.png" file and is persisted with a .png extension, so
// the content type is a constant. This avoids having to persist the upstream
// Content-Type across restarts.
func writeIconHeaders(w http.ResponseWriter, etag string) {
	w.Header().Set("Content-Type", "image/png")
	w.Header().Set("Cache-Control", cacheControl)
	w.Header().Set("ETag", etag)
}

// etagOf returns the memoised ETag for name and whether one is known.
//
// Memoising by gem name is safe because the bytes behind a name cannot change
// while the process runs. The name→URL map is embedded in the binary, and
// ADR-012 requires a redeploy — hence a restart, hence an empty memo — for any
// icon addition or replacement. If the disk copy is deleted mid-process, load
// refetches the same mapped URL and gets the same bytes back.
//
// The memo is only ever written for a name that resolved in the embedded map, so
// it cannot grow past that map's size (765 entries, ~100 KB) no matter what a
// client requests. That bound is why this is a name→ETag memo rather than a
// name→bytes cache: caching the bodies too would remove the disk read as well,
// but costs ~6 MB (measured: 8.2 KB mean over 116 cached icons × 765 entries) to
// avoid a page-cache-warm read of bytes that are about to be written to the wire
// anyway. The SHA-256 was the per-request cost worth removing; the read is not.
func (c *Cache) etagOf(name string) (string, bool) {
	c.etagMu.RLock()
	defer c.etagMu.RUnlock()
	etag, ok := c.etags[name]
	return etag, ok
}

// rememberETag stores the computed ETag for name. Concurrent computations for
// the same name produce the same value, so a last-write-wins race is harmless.
func (c *Cache) rememberETag(name, etag string) {
	c.etagMu.Lock()
	defer c.etagMu.Unlock()
	c.etags[name] = etag
}

// load returns the icon bytes for name, reading the persistent disk copy when it
// exists and otherwise fetching from srcURL exactly once and writing it to disk.
//
// srcURL is part of the cache path, not only of the fetch: correcting a URL in
// the map changes the filename, so the old file is not found, the new bytes are
// fetched, and the map alone decides what a name serves (ADR-012 decision 3).
func (c *Cache) load(ctx context.Context, name, srcURL string) ([]byte, error) {
	path := c.filePath(name, srcURL)
	if body, err := os.ReadFile(path); err == nil {
		return body, nil // served from our own disk copy — no upstream hit
	}

	// Serialize concurrent first-fetches for the same gem so poewiki is hit at
	// most once even under a burst of simultaneous requests.
	lock := c.lockFor(name)
	lock.Lock()
	defer lock.Unlock()

	// Another goroutine may have fetched and written it while we waited. The
	// same (name, srcURL) yields the same path, so this is the same file the
	// first read missed.
	if body, err := os.ReadFile(path); err == nil {
		return body, nil
	}

	body, err := c.fetch(ctx, srcURL)
	if err != nil {
		return nil, err
	}

	if err := c.writeFile(path, body); err != nil {
		// A disk-write failure must not break icon delivery: serve the bytes we
		// already have. The next request will attempt the write again.
		slog.Error("icons: persist icon failed; serving without caching",
			"gem", name, "path", path, "error", err)
	}
	return body, nil
}

// fetch downloads the image at srcURL. A non-200 response or transport error is
// returned as an error so the caller can respond 502 and leave nothing cached.
func (c *Cache) fetch(ctx context.Context, srcURL string) ([]byte, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, srcURL, nil)
	if err != nil {
		return nil, err
	}
	resp, err := c.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("icons: upstream %s returned %d", srcURL, resp.StatusCode)
	}
	return io.ReadAll(io.LimitReader(resp.Body, maxImageBytes))
}

// writeFile persists body to path atomically (temp file + rename) so a crash
// mid-write never leaves a truncated icon in the cache.
func (c *Cache) writeFile(path string, body []byte) error {
	tmp, err := os.CreateTemp(c.dir, ".tmp-*")
	if err != nil {
		return err
	}
	tmpName := tmp.Name()
	if _, err := tmp.Write(body); err != nil {
		_ = tmp.Close()
		_ = os.Remove(tmpName)
		return err
	}
	if err := tmp.Close(); err != nil {
		_ = os.Remove(tmpName)
		return err
	}
	return os.Rename(tmpName, path)
}

// filePath returns the on-disk cache path for a gem name fetched from srcURL:
// "<safeFileName(name)>-<shortHash(srcURL)>.png".
//
// The URL is in the filename because the cache has no other invalidation at all
// — load returns the disk copy unconditionally when the file exists, and
// production may not re-fetch (ADR-012: poewiki 403s the VPS). Keying on the
// name alone meant a corrected URL kept serving the old artwork forever, with no
// error and no log. Keying on both makes a URL edit a different file, hence a
// miss, hence a fetch, so the map is the single source of truth for what a name
// serves. It also keeps the name in the filename, so two names sharing one URL
// still get one file each and neither can be mistaken for the other on disk.
//
// The "-" separator is load-bearing. safeFileName emits [A-Za-z0-9_] only
// (unsafeFileChars collapses every other run to "_", then the ends are
// trimmed), so it can neither contain nor end in "-": the LAST "-" in the
// filename always starts the hash, which is what lets an operator — or the
// prune mode of scripts/download-gem-icons.py — split a cache filename back
// into its two parts unambiguously.
//
// ".png" is hardcoded rather than taken from the URL: every source is a poewiki
// "*_inventory_icon.png", writeIconHeaders serves a constant image/png, and a
// future non-PNG icon set has to change both together.
func (c *Cache) filePath(name, srcURL string) string {
	return filepath.Join(c.dir, safeFileName(name)+"-"+shortHash(srcURL)+".png")
}

// lockFor returns the per-gem mutex, creating it on first use.
func (c *Cache) lockFor(name string) *sync.Mutex {
	c.mu.Lock()
	defer c.mu.Unlock()
	l, ok := c.locks[name]
	if !ok {
		l = &sync.Mutex{}
		c.locks[name] = l
	}
	return l
}

// safeFileName reduces a gem name to a filesystem-safe token.
func safeFileName(name string) string {
	return strings.Trim(unsafeFileChars.ReplaceAllString(name, "_"), "_")
}

// shortHash returns the first 16 hex characters (8 bytes) of the SHA-256 of
// srcURL, for use as the discriminating half of a cache filename.
//
// 16 hex is short for a global identifier and ample for this one, because the
// suffix only has to discriminate WITHIN one safeFileName bucket — the set of
// URLs a single name has ever mapped to, normally exactly one. Even treated as
// a global namespace over the whole 765-entry map, the 64-bit birthday bound is
// about 1.6e-14. The name stays in the filename, so this is a tiebreaker, never
// the identity.
//
// The URL, not the bytes: production cannot fetch (ADR-012), so the bytes are
// not available to it offline and the URL is the only identity it can evaluate.
// That also bounds what this fixes — poewiki's /images/<h>/<hh>/ path is the MD5
// of the FILE name, so a re-upload under the same name keeps the URL and is
// still invisible here. Renaming the file upstream, or correcting a wrong entry,
// is the case this catches.
//
// scripts/download-gem-icons.py reimplements this scheme (short_hash /
// cache_file_name) because the seeding path is Python and production reads what
// it writes. Both sides pin the same vector: TestShortHash_pinnedVector below
// and the script's import-time _self_check().
func shortHash(srcURL string) string {
	sum := sha256.Sum256([]byte(srcURL))
	return fmt.Sprintf("%x", sum[:8])
}

// etagFor returns a strong, quoted ETag derived from the image bytes.
func etagFor(body []byte) string {
	return fmt.Sprintf("%q", fmt.Sprintf("%x", sha256.Sum256(body)))
}
