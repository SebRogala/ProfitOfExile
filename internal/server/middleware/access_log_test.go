package middleware

import (
	"bytes"
	"encoding/json"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"
)

// accessLogRecorder captures the middleware's output as parsed JSON so tests
// assert on the fields a log query would filter on.
type accessLogRecorder struct {
	buf bytes.Buffer
}

func (r *accessLogRecorder) logger() *slog.Logger {
	return slog.New(slog.NewJSONHandler(&r.buf, &slog.HandlerOptions{Level: slog.LevelDebug}))
}

// only returns the single record emitted, failing when the middleware emitted
// none or more than one — "exactly one line per request" is part of the
// contract, since the whole point was to stop double-logging.
func (r *accessLogRecorder) only(t *testing.T) map[string]any {
	t.Helper()

	dec := json.NewDecoder(bytes.NewReader(r.buf.Bytes()))
	var records []map[string]any
	for dec.More() {
		var rec map[string]any
		if err := dec.Decode(&rec); err != nil {
			t.Fatalf("decoding log output %q: %v", r.buf.String(), err)
		}
		records = append(records, rec)
	}
	if len(records) != 1 {
		t.Fatalf("got %d log records, want exactly 1; output: %s", len(records), r.buf.String())
	}
	return records[0]
}

// serve runs one request through AccessLog wrapping the given handler.
func serve(t *testing.T, h http.HandlerFunc, target string) map[string]any {
	t.Helper()

	rec := &accessLogRecorder{}
	AccessLog(rec.logger())(h).ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, target, nil))
	return rec.only(t)
}

func TestAccessLog_RecordsTheStatusTheHandlerWrote(t *testing.T) {
	got := serve(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusCreated)
	}, "/api/thing")

	if status := got["status"]; status != float64(http.StatusCreated) {
		t.Errorf("status = %v, want %d", status, http.StatusCreated)
	}
}

func TestAccessLog_RecordsTheBytesTheHandlerWrote(t *testing.T) {
	body := []byte(`{"ok":true}`)

	got := serve(t, func(w http.ResponseWriter, r *http.Request) {
		w.Write(body)
	}, "/api/thing")

	if bytesWritten := got["bytes"]; bytesWritten != float64(len(body)) {
		t.Errorf("bytes = %v, want %d", bytesWritten, len(body))
	}
}

func TestAccessLog_ReportsTheImplicit200WhenTheHandlerNeverWrites(t *testing.T) {
	// A handler that returns without touching the ResponseWriter still causes
	// net/http to send 200. Logging the wrapper's zero value here would make
	// every such route look like status 0 in production.
	got := serve(t, func(w http.ResponseWriter, r *http.Request) {}, "/api/thing")

	if status := got["status"]; status != float64(http.StatusOK) {
		t.Errorf("status = %v, want %d for a handler that never writes", status, http.StatusOK)
	}
}

func TestAccessLog_RecordsElapsedTimeInMilliseconds(t *testing.T) {
	const handlerDelay = 15 * time.Millisecond

	got := serve(t, func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(handlerDelay)
	}, "/api/thing")

	durationMS, ok := got["duration_ms"].(float64)
	if !ok {
		t.Fatalf("duration_ms = %v (%T), want a number", got["duration_ms"], got["duration_ms"])
	}
	if wantMin := float64(handlerDelay.Milliseconds()); durationMS < wantMin {
		t.Errorf("duration_ms = %v, want at least %v (the handler slept that long)", durationMS, wantMin)
	}
}

func TestAccessLog_LogsAFastSuccessAtInfo(t *testing.T) {
	got := serve(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
	}, "/api/thing")

	if level := got["level"]; level != "INFO" {
		t.Errorf("level = %v, want INFO for a fast 2xx", level)
	}
}

func TestAccessLog_LogsARequestSlowerThanTheThresholdAtWarn(t *testing.T) {
	// The threshold is the point of the middleware: a slow route has to stand
	// out in the log stream without anyone querying for it first.
	got := serve(t, func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(slowRequestThreshold + 10*time.Millisecond)
		w.WriteHeader(http.StatusOK)
	}, "/api/slow")

	if level := got["level"]; level != "WARN" {
		t.Errorf("level = %v, want WARN for a request over %s", level, slowRequestThreshold)
	}
}

func TestAccessLog_LogsAServerErrorAtError(t *testing.T) {
	got := serve(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}, "/api/broken")

	if level := got["level"]; level != "ERROR" {
		t.Errorf("level = %v, want ERROR for a 5xx", level)
	}
}

func TestAccessLog_LogsAFastClientErrorAtInfo(t *testing.T) {
	// Only server errors escalate. A 404 or a rejected payload is normal
	// traffic; promoting 4xx would drown the level that signals a real fault.
	got := serve(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusBadRequest)
	}, "/api/thing")

	if level := got["level"]; level != "INFO" {
		t.Errorf("level = %v, want INFO for a fast 4xx", level)
	}
}

func TestAccessLog_RecordsTheChiRoutePatternNotTheConcretePath(t *testing.T) {
	// Aggregating by route is only useful if a parameterised route is one
	// bucket rather than one bucket per parameter value.
	rec := &accessLogRecorder{}
	router := chi.NewRouter()
	router.Use(AccessLog(rec.logger()))
	router.Get("/api/gem-icon/{name}", func(w http.ResponseWriter, r *http.Request) {})

	router.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/api/gem-icon/Enlighten", nil))

	got := rec.only(t)
	if route := got["route"]; route != "/api/gem-icon/{name}" {
		t.Errorf("route = %v, want the chi pattern %q", route, "/api/gem-icon/{name}")
	}
	if path := got["path"]; path != "/api/gem-icon/Enlighten" {
		t.Errorf("path = %v, want the concrete path", path)
	}
}

func TestAccessLog_MarksAnUnroutedRequestAsUnmatched(t *testing.T) {
	// Outside a chi mux (or below a 404) there is no pattern; the field must
	// still be present so a log query never silently drops these rows.
	got := serve(t, func(w http.ResponseWriter, r *http.Request) {}, "/api/thing")

	if route := got["route"]; route != "unmatched" {
		t.Errorf("route = %v, want %q when routing produced no pattern", route, "unmatched")
	}
}

func TestAccessLog_StillLogsWhenTheHandlerPanics(t *testing.T) {
	// SlogRecoverer sits below this middleware in the real chain, but a line
	// has to be emitted even when nothing recovers — a panicking route that
	// logs nothing is exactly the blind spot POE-155 is about.
	rec := &accessLogRecorder{}
	h := AccessLog(rec.logger())(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		panic("boom")
	}))

	func() {
		defer func() {
			if recover() == nil {
				t.Fatal("expected the panic to propagate past AccessLog")
			}
		}()
		h.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/api/thing", nil))
	}()

	got := rec.only(t)
	if method := got["method"]; method != http.MethodGet {
		t.Errorf("method = %v, want %q", method, http.MethodGet)
	}
}
