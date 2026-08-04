package server

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"testing/fstest"

	"profitofexile/internal/server/handlers"
)

// validPprofToken is long enough to satisfy pprofMinTokenLen.
const validPprofToken = "0123456789abcdef0123456789abcdef"

// pprofProbePath is a named runtime profile rather than the index or the CPU
// profile: it responds immediately (no sample window) and its body starts with
// a marker no other route in this router can produce.
const pprofProbePath = "/debug/pprof/goroutine?debug=1"

// pprofBodyMarker is the first line net/http/pprof writes for a debug=1
// goroutine profile.
const pprofBodyMarker = "goroutine profile:"

// spaFS mimics the embedded frontend, which is what production actually serves
// on the catch-all. Without it an unmounted /debug/pprof would 404 and a test
// could pass on the 404 alone while production quietly served something else.
func spaFS() fstest.MapFS {
	return fstest.MapFS{
		"index.html": &fstest.MapFile{Data: []byte("<html><body>ProfitOfExile</body></html>")},
	}
}

func getPprof(t *testing.T, router http.Handler, header, value string) *httptest.ResponseRecorder {
	t.Helper()
	req := httptest.NewRequest(http.MethodGet, pprofProbePath, nil)
	if header != "" {
		req.Header.Set(header, value)
	}
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)
	return w
}

func TestPprof_NotServedWhenNeitherDevModeNorTokenIsSet(t *testing.T) {
	t.Setenv(pprofTokenEnv, "")
	router := NewRouter(handlers.NopPinger{}, spaFS(), RouterConfig{DevMode: false})

	w := getPprof(t, router, "", "")

	if body := w.Body.String(); strings.Contains(body, pprofBodyMarker) {
		t.Fatalf("GET %s served a goroutine profile with profiling disabled; body: %s", pprofProbePath, body)
	}
	// Production's catch-all answers unknown paths with the SPA, so this is the
	// response an unauthenticated prober actually sees.
	if !strings.Contains(w.Body.String(), "ProfitOfExile") {
		t.Errorf("GET %s body = %q, want the SPA fallback", pprofProbePath, w.Body.String())
	}
}

func TestPprof_ServedUnauthenticatedInDevMode(t *testing.T) {
	t.Setenv(pprofTokenEnv, "")
	router := NewRouter(handlers.NopPinger{}, spaFS(), RouterConfig{DevMode: true})

	w := getPprof(t, router, "", "")

	if w.Code != http.StatusOK {
		t.Fatalf("GET %s status = %d, want %d", pprofProbePath, w.Code, http.StatusOK)
	}
	if !strings.Contains(w.Body.String(), pprofBodyMarker) {
		t.Errorf("GET %s body = %q, want it to contain %q", pprofProbePath, w.Body.String(), pprofBodyMarker)
	}
}

func TestPprof_TokenConfiguredRejectsRequestWithoutToken(t *testing.T) {
	t.Setenv(pprofTokenEnv, validPprofToken)
	router := NewRouter(handlers.NopPinger{}, spaFS(), RouterConfig{DevMode: false})

	w := getPprof(t, router, "", "")

	if w.Code != http.StatusNotFound {
		t.Errorf("GET %s without a token status = %d, want %d", pprofProbePath, w.Code, http.StatusNotFound)
	}
	if strings.Contains(w.Body.String(), pprofBodyMarker) {
		t.Fatalf("GET %s without a token served a goroutine profile", pprofProbePath)
	}
}

func TestPprof_TokenConfiguredRejectsWrongToken(t *testing.T) {
	t.Setenv(pprofTokenEnv, validPprofToken)
	router := NewRouter(handlers.NopPinger{}, spaFS(), RouterConfig{DevMode: false})

	w := getPprof(t, router, "X-Pprof-Token", strings.Repeat("f", len(validPprofToken)))

	if w.Code != http.StatusNotFound {
		t.Errorf("GET %s with a wrong token status = %d, want %d", pprofProbePath, w.Code, http.StatusNotFound)
	}
	if strings.Contains(w.Body.String(), pprofBodyMarker) {
		t.Fatalf("GET %s with a wrong token served a goroutine profile", pprofProbePath)
	}
}

func TestPprof_TokenConfiguredAcceptsCorrectTokenHeader(t *testing.T) {
	t.Setenv(pprofTokenEnv, validPprofToken)
	router := NewRouter(handlers.NopPinger{}, spaFS(), RouterConfig{DevMode: false})

	w := getPprof(t, router, "X-Pprof-Token", validPprofToken)

	if w.Code != http.StatusOK {
		t.Fatalf("GET %s with the correct token status = %d, want %d", pprofProbePath, w.Code, http.StatusOK)
	}
	if !strings.Contains(w.Body.String(), pprofBodyMarker) {
		t.Errorf("GET %s body = %q, want it to contain %q", pprofProbePath, w.Body.String(), pprofBodyMarker)
	}
}

func TestPprof_TokenConfiguredAcceptsAuthorizationBearer(t *testing.T) {
	t.Setenv(pprofTokenEnv, validPprofToken)
	router := NewRouter(handlers.NopPinger{}, spaFS(), RouterConfig{DevMode: false})

	w := getPprof(t, router, "Authorization", "Bearer "+validPprofToken)

	if w.Code != http.StatusOK {
		t.Fatalf("GET %s with a bearer token status = %d, want %d", pprofProbePath, w.Code, http.StatusOK)
	}
	if !strings.Contains(w.Body.String(), pprofBodyMarker) {
		t.Errorf("GET %s body = %q, want it to contain %q", pprofProbePath, w.Body.String(), pprofBodyMarker)
	}
}

func TestPprof_ShortTokenLeavesProfilingUnmounted(t *testing.T) {
	short := strings.Repeat("a", pprofMinTokenLen-1)
	t.Setenv(pprofTokenEnv, short)
	router := NewRouter(handlers.NopPinger{}, spaFS(), RouterConfig{DevMode: false})

	// Even presenting the configured value must not work: a too-short token is
	// refused as a mount, not accepted as a weaker one.
	w := getPprof(t, router, "X-Pprof-Token", short)

	if strings.Contains(w.Body.String(), pprofBodyMarker) {
		t.Fatalf("GET %s served a goroutine profile with a %d-char token (minimum is %d)",
			pprofProbePath, len(short), pprofMinTokenLen)
	}
}

func TestPprof_WhitespaceOnlyTokenLeavesProfilingUnmounted(t *testing.T) {
	// A token set to whitespace by a broken deploy template must not be treated
	// as "configured" — it would mount the routes behind a guessable secret.
	t.Setenv(pprofTokenEnv, "   ")
	router := NewRouter(handlers.NopPinger{}, spaFS(), RouterConfig{DevMode: false})

	w := getPprof(t, router, "X-Pprof-Token", "   ")

	if strings.Contains(w.Body.String(), pprofBodyMarker) {
		t.Fatalf("GET %s served a goroutine profile with a whitespace-only token", pprofProbePath)
	}
}
