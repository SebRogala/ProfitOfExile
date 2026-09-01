package server

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"testing/fstest"
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/league"
	"profitofexile/internal/server/handlers"
)

func TestNewRouter_HealthRoute(t *testing.T) {
	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /api/health status = %d, want %d", w.Code, http.StatusOK)
	}

	contentType := w.Header().Get("Content-Type")
	if contentType != "application/json" {
		t.Errorf("Content-Type = %q, want %q", contentType, "application/json")
	}

	var body struct {
		Status  string `json:"status"`
		Version string `json:"version"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("failed to decode response body: %v", err)
	}
	if body.Status != "ok" {
		t.Errorf("status = %q, want %q", body.Status, "ok")
	}
	if body.Version != "dev" {
		t.Errorf("version = %q, want %q", body.Version, "dev")
	}
}

func TestNewRouter_HealthTakesPrecedenceOverStaticCatchAll(t *testing.T) {
	frontendFS := fstest.MapFS{
		"index.html": &fstest.MapFile{
			Data: []byte("<html><body>SPA</body></html>"),
		},
	}
	router := NewRouter(handlers.NopPinger{}, frontendFS, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /api/health status = %d, want %d", w.Code, http.StatusOK)
	}

	contentType := w.Header().Get("Content-Type")
	if contentType != "application/json" {
		t.Errorf("Content-Type = %q, want %q", contentType, "application/json")
	}

	var body struct {
		Status string `json:"status"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("failed to decode response body: %v", err)
	}
	if body.Status != "ok" {
		t.Errorf("status = %q, want %q", body.Status, "ok")
	}
}

func TestNewRouter_StaticCatchAllServesFiles(t *testing.T) {
	frontendFS := fstest.MapFS{
		"index.html": &fstest.MapFile{
			Data: []byte("<html><body>ProfitOfExile</body></html>"),
		},
	}
	router := NewRouter(handlers.NopPinger{}, frontendFS, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

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
}

func TestNewRouter_StaticCatchAllFallbackForUnknownPaths(t *testing.T) {
	frontendFS := fstest.MapFS{
		"index.html": &fstest.MapFile{
			Data: []byte("<html><body>ProfitOfExile</body></html>"),
		},
	}
	router := NewRouter(handlers.NopPinger{}, frontendFS, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/strategies/lab", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /strategies/lab status = %d, want %d", w.Code, http.StatusOK)
	}

	body, err := io.ReadAll(w.Body)
	if err != nil {
		t.Fatalf("failed to read response body: %v", err)
	}
	if !strings.Contains(string(body), "ProfitOfExile") {
		t.Errorf("GET /strategies/lab body = %q, want SPA fallback with %q", string(body), "ProfitOfExile")
	}
}

func TestNewRouter_CurrencyExchangeRouteIsServedWithoutTheLabStack(t *testing.T) {
	// Currency exchange is a separate pillar: its own collector, tables and
	// cache, sharing nothing with LabRepo. Registering it inside the
	// `if cfg.LabRepo != nil` block would make a lab-less server 404 on it —
	// and the cache the route answers from is the one in the config, which is
	// what the league and the play count below prove.
	cache := exchange.NewCache()
	cache.Set(exchange.DefaultHorizon, exchange.Result{
		League: "Mirage",
		Hours:  6,
		To:     time.Date(2026, 8, 19, 7, 0, 0, 0, time.UTC),
		Plays:  []exchange.Play{{Key: "direct:a", Mode: exchange.ModeDirect}},
	})
	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{
		League:        league.Historical("Mirage"),
		ExchangeCache: cache,
	})

	req := httptest.NewRequest(http.MethodGet, "/api/currency-exchange/plays", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /api/currency-exchange/plays status = %d, want %d (body: %s)",
			w.Code, http.StatusOK, w.Body.String())
	}
	var body struct {
		League string `json:"league"`
		Warm   bool   `json:"warm"`
		Count  int    `json:"count"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("failed to decode response body: %v", err)
	}
	if body.League != "Mirage" {
		t.Errorf("league = %q, want %q — the route must read the configured cache", body.League, "Mirage")
	}
	if !body.Warm {
		t.Error("warm = false, want true")
	}
	if body.Count != 1 {
		t.Errorf("count = %d, want 1", body.Count)
	}
}

func TestNewRouter_CurrencyExchangeRouteIsRegisteredWithoutACache(t *testing.T) {
	// A server started without the pillar leaves ExchangeCache nil; the route
	// still answers, cold, rather than 404ing.
	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/api/currency-exchange/plays", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /api/currency-exchange/plays status = %d, want %d (body: %s)",
			w.Code, http.StatusOK, w.Body.String())
	}
	var body struct {
		Warm  bool `json:"warm"`
		Count int  `json:"count"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("failed to decode response body: %v", err)
	}
	if body.Warm {
		t.Error("warm = true, want false")
	}
	if body.Count != 0 {
		t.Errorf("count = %d, want 0", body.Count)
	}
}

func TestNewRouter_NilFrontendFSReturns404ForNonAPIPaths(t *testing.T) {
	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	// With no frontendFS, non-API paths should return 404 or 405.
	if w.Code == http.StatusOK {
		t.Errorf("GET / with nil frontendFS status = %d, want non-200 (404 or 405)", w.Code)
	}
}

// currencyExchangeIconPath is the escaped-id path clients request, built the way
// exchange.IconPath builds it: the whole metadata id is ONE route segment, so
// its slashes arrive as %2F.
const currencyExchangeIconPath = "/api/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyNotInTheAssetYet"

// The icon route is registered whenever the cache root resolves. An unknown id
// is what proves it without a network call: the icon handler answers its own 404
// and stamps it no-store, where an unregistered route falls through to chi's
// NotFound, which sets no Cache-Control at all. Drop the r.Get and this fails on
// the header.
func TestNewRouter_CurrencyExchangeIconRouteIsRegisteredWhenACacheRootIsConfigured(t *testing.T) {
	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{
		IconCacheDir: t.TempDir(),
	})

	req := httptest.NewRequest(http.MethodGet, currencyExchangeIconPath, nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusNotFound {
		t.Fatalf("GET %s status = %d, want %d (body: %s)",
			currencyExchangeIconPath, w.Code, http.StatusNotFound, w.Body.String())
	}
	if got := w.Header().Get("Cache-Control"); got != "no-store" {
		t.Errorf("Cache-Control = %q, want %q — the icon handler answered this 404, chi's NotFound would not have set it",
			got, "no-store")
	}
}

// An unset root is not an unconfigured icon set (POE-221): there is one root for
// every set, it has a default, and the sub-directory split — not the presence of
// a second configured directory — is what keeps the two filename spaces apart.
// The fallback to DefaultIconCacheDir is what this pins, and only a seeded file
// makes it observable: the test runs in an empty working directory and writes
// one icon under the default root's currency-exchange sub-directory, so a 200
// carrying those exact bytes can only have been read from that root.
//
// Delete the `iconRoot == ""` branch in NewRouter and this fails — but not the
// way a missing route fails, which is why the assertion is on the bytes and not
// on a status or a header. filepath.Join("", "currency-exchange") is
// "currency-exchange": non-empty, so no constructor rejects it and the route is
// still registered. The cache simply lands in a working-directory-relative
// `currency-exchange` (and `gems`) instead of under the default root, misses the
// seeded file, and answers with anything but these bytes.
func TestNewRouter_CurrencyExchangeIconRouteServesFromTheDefaultRootWhenNoneIsConfigured(t *testing.T) {
	chdirToEmptyDir(t)

	// The seed path restates DefaultIconCacheDir's value and gemicon's filename
	// scheme literally, as an independent oracle: deriving it from the constant
	// would move the seed in lockstep with the value under test.
	const itemID = "Metadata/Items/Currency/CurrencyRerollRare"
	itemBytes := []byte("\x89PNG\r\n\x1a\nseeded-under-the-default-icon-cache-root")
	seedPath := filepath.Join("data", "icons-cache", "currency-exchange",
		"Metadata_Items_Currency_CurrencyRerollRare.png")
	writeIconFile(t, seedPath, itemBytes)

	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{})

	itemURL := "/api/currency-exchange/icon/" + url.PathEscape(itemID)
	req := httptest.NewRequest(http.MethodGet, itemURL, nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET %s status = %d, want %d — the icon route did not read %q (body: %s)",
			itemURL, w.Code, http.StatusOK, seedPath, w.Body.String())
	}
	if !bytes.Equal(w.Body.Bytes(), itemBytes) {
		t.Errorf("route served %q, want the bytes seeded at %q — other bytes mean the cache resolved somewhere other than the default root",
			w.Body.Bytes(), seedPath)
	}
}

// chdirToEmptyDir runs the rest of the test from a fresh empty directory, so a
// relative cache path resolves somewhere the test controls and starts unseeded.
// t.Chdir would say this in one line, but it arrived in go1.24 and go.mod
// targets go1.23; no test in this package calls t.Parallel, so changing the
// process-wide working directory is safe here.
func chdirToEmptyDir(t *testing.T) {
	t.Helper()
	prev, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	if err := os.Chdir(t.TempDir()); err != nil {
		t.Fatalf("chdir to temp dir: %v", err)
	}
	t.Cleanup(func() {
		if err := os.Chdir(prev); err != nil {
			t.Fatalf("restore working directory %q: %v", prev, err)
		}
	})
}

// The two icon sets share one cache root and one cache-filename scheme, so the
// per-set sub-directory is the only thing keeping a gem name and a metadata id
// that reduce to the same filename from serving each other's artwork.
//
// A pre-placed file per sub-directory is what makes that observable with no
// network at all: gemicon's load() returns the disk copy when it exists and only
// fetches upstream when it does not, so a 200 carrying these exact bytes can
// only have come from the directory the test wrote them to. Flatten both sets
// into the root, or swap the two sub-directory constants, and each route misses
// its file and leaves the byte comparison below.
//
// Both the sub-directory names and the two file names are written out here
// rather than taken from the production constants and helpers, deliberately, as
// an independent oracle. Seeding through gemIconSubdir would move the seed
// whenever the constant moved and the swap above would go undetected; the file
// names restate gemicon.safeFileName's scheme (runs of [^A-Za-z0-9] collapsed to
// "_") for the same reason, and because the seeding script and the operator
// runbook both hard-depend on that scheme staying put.
func TestNewRouter_EachIconRouteServesFromItsOwnSubdirectoryOfTheCacheRoot(t *testing.T) {
	const (
		// A key each embedded map really carries — the maps are what decide 404
		// before any directory is consulted.
		gemName = "Absolution"
		itemID  = "Metadata/Items/Currency/CurrencyRerollRare"
	)
	// The whole metadata id is ONE route segment, so its slashes are escaped —
	// the same shape exchange.IconPath builds for clients.
	itemEscapedURL := "/api/currency-exchange/icon/" + url.PathEscape(itemID)
	gemBytes := []byte("\x89PNG\r\n\x1a\nseeded-into-the-gems-subdir")
	itemBytes := []byte("\x89PNG\r\n\x1a\nseeded-into-the-currency-exchange-subdir")

	root := t.TempDir()
	gemDir := filepath.Join(root, "gems")
	itemDir := filepath.Join(root, "currency-exchange")
	writeIconFile(t, filepath.Join(gemDir, "Absolution.png"), gemBytes)
	writeIconFile(t, filepath.Join(itemDir, "Metadata_Items_Currency_CurrencyRerollRare.png"), itemBytes)

	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{IconCacheDir: root})

	gemW := httptest.NewRecorder()
	router.ServeHTTP(gemW, httptest.NewRequest(http.MethodGet, "/api/gem-icon/"+gemName, nil))
	if gemW.Code != http.StatusOK {
		t.Fatalf("GET /api/gem-icon/%s status = %d, want %d — the gem route did not find its file under %q (body: %s)",
			gemName, gemW.Code, http.StatusOK, gemDir, gemW.Body.String())
	}
	if !bytes.Equal(gemW.Body.Bytes(), gemBytes) {
		t.Errorf("gem route served %q, want the bytes seeded into %q",
			gemW.Body.Bytes(), gemDir)
	}

	itemW := httptest.NewRecorder()
	router.ServeHTTP(itemW, httptest.NewRequest(http.MethodGet, itemEscapedURL, nil))
	if itemW.Code != http.StatusOK {
		t.Fatalf("GET %s status = %d, want %d — the item route did not find its file under %q (body: %s)",
			itemEscapedURL, itemW.Code, http.StatusOK, itemDir, itemW.Body.String())
	}
	if !bytes.Equal(itemW.Body.Bytes(), itemBytes) {
		t.Errorf("currency-exchange route served %q, want the bytes seeded into %q — a match against the gem bytes means both sets resolved to one directory",
			itemW.Body.Bytes(), itemDir)
	}
}

// writeIconFile seeds one cache file, creating its directory. The directory is
// created here rather than relying on NewRouter so the file is in place before
// the first request, which is what keeps the test off the network.
func writeIconFile(t *testing.T, path string, body []byte) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("seed icon dir %q: %v", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, body, 0o644); err != nil {
		t.Fatalf("seed icon file %q: %v", path, err)
	}
}

func TestNewRouter_DeviceMeRouteIsRegisteredWithoutADeviceRepo(t *testing.T) {
	// A server started without device identity leaves DeviceRepo nil, so the
	// device middleware is never installed. The route is registered anyway and
	// answers with the unentitled defaults — the desktop app gates its beta
	// module on this reply, so a 404 would be a distinct failure mode for the
	// same "nothing hidden is available" outcome.
	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/api/device/me", nil)
	req.Header.Set("X-Device-ID", "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /api/device/me status = %d, want %d (body: %s)",
			w.Code, http.StatusOK, w.Body.String())
	}
	var body struct {
		Role     string   `json:"role"`
		Channel  string   `json:"channel"`
		Features []string `json:"features"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("failed to decode response body: %v", err)
	}
	if body.Role != "" {
		t.Errorf("role = %q, want empty", body.Role)
	}
	if body.Channel != "stable" {
		t.Errorf("channel = %q, want %q", body.Channel, "stable")
	}
	if len(body.Features) != 0 {
		t.Errorf("features = %#v, want empty", body.Features)
	}
}
