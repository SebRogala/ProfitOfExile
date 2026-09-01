// Package gemicon serves Path of Exile artwork to both the web and desktop
// clients through a persistent, on-disk icon cache.
//
// It backs two endpoints over two independent instances: gem inventory icons at
// /api/gem-icon/{name} from the embedded gem map (New), and currency exchange
// item icons at /api/currency-exchange/icon/{name} from internal/exchange's
// asset (NewWithMap). Nothing below is gem-specific — a key is whatever the
// route parameter carries — so the package keeps its name for its origin, and
// the description that follows says "gem" because that is the set it was
// written for.
//
// The clients only know a gem's display name (e.g. "Added Chaos Damage
// Support"). The correct poewiki image URL is an /images/<h>/<hh>/ path that
// MediaWiki derives from the MD5 of the image's FILE name — stable for that
// name, so a re-upload keeps the URL, but not constructible from the gem's
// display name — so a name→URL map is embedded from
// gem-icon-urls.json. On the first request for a gem the upstream image is
// fetched once and written to a persistent on-disk cache directory ("our own
// copy"); every subsequent request — including after a server restart — is
// served straight from disk with no upstream fetch. poewiki is therefore hit at
// most once, ever, per gem. Clients additionally cache aggressively via a long
// immutable Cache-Control header, so icons are not re-downloaded on every app
// open.
//
// Only URLs live in the repo (the embedded map). The fetched image bytes are
// never committed — they are copyrighted and heavy — so the cache directory is
// git-ignored. In production the cache directory must be a persistent volume;
// otherwise every redeploy starts with an empty cache and re-fetches.
package gemicon

import (
	"context"
	"crypto/sha256"
	_ "embed"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/go-chi/chi/v5"
)

//go:embed gem-icon-urls.json
var iconURLsJSON []byte

// maxImageBytes bounds a single upstream fetch. Inventory icons are a few KB;
// this cap guards against a misbehaving or unexpected upstream response.
const maxImageBytes = 5 << 20 // 5 MiB

// cacheControl instructs browsers and webviews to cache an icon for a year and
// never revalidate. The upstream URL is stable per file name (MediaWiki hashes
// the name, not the bytes), so a cached icon is treated as immutable; this is
// what stops re-downloads on every app open.
const cacheControl = "public, max-age=31536000, immutable"

// notFoundCacheControl stops clients from caching an unknown-gem 404 at all.
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

// Cache resolves gem names to poewiki icons and persists fetched bytes to disk.
type Cache struct {
	urls   map[string]string
	client *http.Client
	dir    string

	mu    sync.Mutex
	locks map[string]*sync.Mutex

	// etags memoises the strong ETag per gem name so a conditional request can
	// be answered without reading the file or hashing it again. See etagOf.
	etagMu sync.RWMutex
	etags  map[string]string
}

// New builds a Cache from the embedded gem name→URL map and ensures cacheDir
// exists. It returns an error if the embedded JSON is malformed (a build-time
// defect), cacheDir is empty, or the cache directory cannot be created.
//
// cacheDir is required here for the same reason NewWithMap requires it, and
// this package deliberately holds no default: any default it held would be the
// GEM set's directory, and nothing stops a second map from being handed it. The
// caller owns the layout — internal/server derives one sub-directory per map
// from a single configured cache root (POE-221).
func New(cacheDir string) (*Cache, error) {
	var urls map[string]string
	if err := json.Unmarshal(iconURLsJSON, &urls); err != nil {
		return nil, fmt.Errorf("gemicon: parse embedded url map: %w", err)
	}
	return NewWithMap(urls, cacheDir)
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
// other's keys — would serve one set's artwork under the other's name.
// cacheDir is therefore required: an empty one is an unconfigured caller, not a
// request for a default. Since POE-221 those directories are sub-directories of
// one configured cache root, which is what lets production mount a single
// volume without reintroducing the collision.
func NewWithMap(urls map[string]string, cacheDir string) (*Cache, error) {
	if cacheDir == "" {
		return nil, errors.New("gemicon: cache dir is required")
	}
	if err := os.MkdirAll(cacheDir, 0o755); err != nil {
		return nil, fmt.Errorf("gemicon: create cache dir %q: %w", cacheDir, err)
	}
	return newCache(urls, &http.Client{Timeout: 10 * time.Second}, cacheDir), nil
}

// newCache is the shared constructor used by New and by tests that inject a stub
// upstream and a scratch directory.
func newCache(urls map[string]string, client *http.Client, dir string) *Cache {
	return &Cache{
		urls:   urls,
		client: client,
		dir:    dir,
		locks:  make(map[string]*sync.Mutex),
		etags:  make(map[string]string),
	}
}

// Handler serves GET /api/gem-icon/{name}. Unknown names yield 404 (the client
// renders its "?" fallback); upstream failures yield 502 and write nothing to
// disk, so a later request can retry.
func (c *Cache) Handler() http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
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
			slog.Error("gemicon: serve icon failed",
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
// it cannot grow past that map's size (762 entries, ~100 KB) no matter what a
// client requests. That bound is why this is a name→ETag memo rather than a
// name→bytes cache: caching the bodies too would remove the disk read as well,
// but costs ~6 MB (measured: 8.2 KB mean over 116 cached icons × 762 entries) to
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
func (c *Cache) load(ctx context.Context, name, srcURL string) ([]byte, error) {
	path := c.filePath(name)
	if body, err := os.ReadFile(path); err == nil {
		return body, nil // served from our own disk copy — no upstream hit
	}

	// Serialize concurrent first-fetches for the same gem so poewiki is hit at
	// most once even under a burst of simultaneous requests.
	lock := c.lockFor(name)
	lock.Lock()
	defer lock.Unlock()

	// Another goroutine may have fetched and written it while we waited.
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
		slog.Error("gemicon: persist icon failed; serving without caching",
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
		return nil, fmt.Errorf("gemicon: upstream %s returned %d", srcURL, resp.StatusCode)
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

// filePath returns the on-disk cache path for a gem name.
func (c *Cache) filePath(name string) string {
	return filepath.Join(c.dir, safeFileName(name)+".png")
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

// etagFor returns a strong, quoted ETag derived from the image bytes.
func etagFor(body []byte) string {
	return fmt.Sprintf("%q", fmt.Sprintf("%x", sha256.Sum256(body)))
}
