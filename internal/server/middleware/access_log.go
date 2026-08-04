package middleware

import (
	"log/slog"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"
	chimw "github.com/go-chi/chi/v5/middleware"
)

// slowRequestThreshold is the elapsed time past which a request is logged at
// WARN instead of INFO.
//
// It is set from measurement, not taste: the two incidents this middleware
// exists to have caught were a 14.7 s route (POE-150) and a 6.35 s worst case
// under a 20-request burst (POE-152). Everything served out of lab.Cache
// answers in single-digit milliseconds, so a second is already two orders of
// magnitude off the normal path and cheap to leave at WARN — it does not fire
// during ordinary traffic, which is what keeps the level meaningful.
const slowRequestThreshold = time.Second

// AccessLog returns middleware that emits exactly one structured line per
// request, at a level chosen from the outcome:
//
//	5xx           -> ERROR
//	>= 1 s        -> WARN
//	everything else -> INFO
//
// Why this replaced chi's middleware.Logger (POE-155): chi's logger writes its
// own human-readable text format, so in production it interleaved unparseable
// lines into an otherwise structured slog stream and carried no field a log
// query could filter or aggregate on. The consequence was measured — a 14.7 s
// endpoint and a 6.35 s p95 sat unnoticed until someone probed prod by hand.
//
// The other half of that anti-pattern is per-handler timing: handlers grew
// their own ad-hoc duration_ms lines one at a time, so a route was only visible
// if somebody had already suspected it. Those lines stay where they carry
// context this middleware cannot see (row counts, cache hit/miss, query
// shape) — the point is that no handler has to add one to be observable at all.
//
// The route field is chi's matched pattern (/api/gem-icon/{name}), not the raw
// path, so aggregating by route does not explode into one bucket per gem name.
// The raw path is logged alongside it for the cases where the parameter is the
// thing you need.
func AccessLog(logger *slog.Logger) func(http.Handler) http.Handler {
	if logger == nil {
		logger = slog.Default()
	}

	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			ww := chimw.NewWrapResponseWriter(w, r.ProtoMajor)
			start := time.Now()

			// Deferred so the line is still emitted when a handler panics. This
			// middleware wraps SlogRecoverer, which recovers and writes 500
			// first, so the status observed here is the one the client got.
			defer func() {
				elapsed := time.Since(start)
				status := ww.Status()
				if status == 0 {
					// The handler returned without ever calling WriteHeader or
					// Write; net/http sends 200 on its behalf. Report what the
					// client will see, not the zero value.
					status = http.StatusOK
				}

				logger.LogAttrs(r.Context(), accessLogLevel(status, elapsed), "http request",
					slog.String("method", r.Method),
					slog.String("route", routePattern(r)),
					slog.String("path", r.URL.Path),
					slog.Int("status", status),
					slog.Float64("duration_ms", millis(elapsed)),
					slog.Int("bytes", ww.BytesWritten()),
					slog.String("request_id", chimw.GetReqID(r.Context())),
				)
			}()

			next.ServeHTTP(ww, r)
		})
	}
}

// accessLogLevel maps a request outcome onto a log level. A server error
// outranks slowness: a 500 that took 3 s is an error, not a slow request.
func accessLogLevel(status int, elapsed time.Duration) slog.Level {
	switch {
	case status >= http.StatusInternalServerError:
		return slog.LevelError
	case elapsed >= slowRequestThreshold:
		return slog.LevelWarn
	default:
		return slog.LevelInfo
	}
}

// routePattern returns the chi route pattern matched for this request, or
// "unmatched" when routing produced no pattern (404s below the router, and any
// caller that uses this middleware outside a chi mux).
//
// It is read after next.ServeHTTP has run because chi fills the pattern in
// during routing. That works from a deferred closure because the *chi.Context
// in the request context is a pointer the router mutates in place.
func routePattern(r *http.Request) string {
	rctx := chi.RouteContext(r.Context())
	if rctx == nil {
		return "unmatched"
	}
	if p := rctx.RoutePattern(); p != "" {
		return p
	}
	return "unmatched"
}

// millis renders a duration as milliseconds with microsecond resolution.
// Sub-millisecond requests are the normal case here, so truncating to whole
// milliseconds would log 0 for most of the traffic.
func millis(d time.Duration) float64 {
	return float64(d.Microseconds()) / 1000
}
