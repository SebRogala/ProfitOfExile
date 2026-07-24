// Package gemicon serves Path of Exile gem inventory icons to both the web and
// desktop clients through a single server endpoint.
//
// The clients only know a gem's display name (e.g. "Added Chaos Damage
// Support"). The correct poewiki image URL is a content-hashed path that cannot
// be constructed from the name, so a name→URL map is embedded from
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

// DefaultCacheDir is used when no cache directory is configured.
const DefaultCacheDir = "./data/gem-icons-cache"

// maxImageBytes bounds a single upstream fetch. Inventory icons are a few KB;
// this cap guards against a misbehaving or unexpected upstream response.
const maxImageBytes = 5 << 20 // 5 MiB

// cacheControl instructs browsers and webviews to cache an icon for a year and
// never revalidate. Icon bytes for a given gem name are immutable (the upstream
// URL is content-hashed), so this is safe and is what stops re-downloads on
// every app open.
const cacheControl = "public, max-age=31536000, immutable"

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
}

// New builds a Cache from the embedded name→URL map and ensures cacheDir exists.
// An empty cacheDir falls back to DefaultCacheDir. It returns an error if the
// embedded JSON is malformed (a build-time defect) or the cache directory
// cannot be created.
func New(cacheDir string) (*Cache, error) {
	var urls map[string]string
	if err := json.Unmarshal(iconURLsJSON, &urls); err != nil {
		return nil, fmt.Errorf("gemicon: parse embedded url map: %w", err)
	}
	if cacheDir == "" {
		cacheDir = DefaultCacheDir
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
			http.NotFound(w, r)
			return
		}

		body, err := c.load(r.Context(), name, srcURL)
		if err != nil {
			http.Error(w, "gem icon unavailable", http.StatusBadGateway)
			return
		}

		etag := etagFor(body)
		// Every source is a poewiki "*_inventory_icon.png" file and is persisted
		// with a .png extension, so the content type is a constant. This avoids
		// having to persist the upstream Content-Type across restarts.
		w.Header().Set("Content-Type", "image/png")
		w.Header().Set("Cache-Control", cacheControl)
		w.Header().Set("ETag", etag)
		if r.Header.Get("If-None-Match") == etag {
			w.WriteHeader(http.StatusNotModified)
			return
		}
		_, _ = w.Write(body)
	}
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
