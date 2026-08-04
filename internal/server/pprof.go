package server

import (
	"crypto/subtle"
	"log/slog"
	"net/http"
	"net/http/pprof"
	"os"
	"strings"

	"github.com/go-chi/chi/v5"
)

const (
	// pprofTokenEnv holds the shared secret that enables profiling on a
	// deployment where DevMode is false. Unset means pprof is not mounted at
	// all — the routes do not exist, so this is off by default and stays off
	// unless an operator turns it on for a specific investigation.
	pprofTokenEnv = "POE_PPROF_TOKEN"

	// pprofMinTokenLen rejects a token short enough to be guessed. A weak token
	// is worse than no token: it reads as protection while leaving the
	// endpoints open, so the mount is refused outright rather than downgraded.
	pprofMinTokenLen = 24
)

// mountPprof registers net/http/pprof under /debug/pprof, but only when
// profiling has been deliberately enabled.
//
// POE-155 asked for pprof behind "the DevMode gate or the device middleware".
// Neither is right on its own, and the reason is worth recording:
//
//   - The device middleware is identity, not authentication. Any client can
//     mint a well-formed 64-hex fingerprint and the middleware auto-registers
//     it on first sight (internal/server/middleware/device.go). Gating on it
//     would make pprof effectively public.
//   - DevMode alone cannot answer the question the task was opened to answer.
//     The whole point is measuring the production process — whether 2 CPU cores
//     really is the ceiling, and whether goroutines are parked in
//     pgxpool.Acquire under load. A dev-only endpoint measures a laptop.
//
// So: DevMode mounts it unauthenticated for local work, and in every other
// environment it appears only when POE_PPROF_TOKEN is set, with every request
// required to present that token. Default is not mounted.
//
// This matters more than a typical debug route because pprof is both an
// information leak (goroutine stacks, the process command line, live heap
// contents) and a denial-of-service lever: /debug/pprof/profile pins a core for
// its full sample window, and production has two.
//
// Operating it: fetch profiles with curl and open them locally, since the token
// travels in a header.
//
//	curl -H "X-Pprof-Token: $TOKEN" 'https://HOST/debug/pprof/goroutine?debug=2'
//	curl -H "X-Pprof-Token: $TOKEN" -o cpu.pb.gz 'https://HOST/debug/pprof/profile?seconds=20'
//	go tool pprof cpu.pb.gz
//
// Keep seconds below the server's 30 s WriteTimeout (cmd/server/main.go) or the
// connection is closed before the profile is written.
func mountPprof(r chi.Router, devMode bool) {
	token := strings.TrimSpace(os.Getenv(pprofTokenEnv))

	switch {
	case token != "":
		if len(token) < pprofMinTokenLen {
			slog.Error("pprof: token too short, profiling endpoints NOT mounted",
				"env", pprofTokenEnv,
				"min_length", pprofMinTokenLen,
				"got_length", len(token),
			)
			return
		}
		r.Route("/debug/pprof", func(pr chi.Router) {
			pr.Use(pprofTokenGate(token))
			pprofRoutes(pr)
		})
		slog.Warn("pprof: profiling endpoints mounted at /debug/pprof (token required)")

	case devMode:
		r.Route("/debug/pprof", pprofRoutes)
		slog.Warn("pprof: profiling endpoints mounted at /debug/pprof UNAUTHENTICATED (dev mode)")
	}
}

// pprofRoutes registers the standard net/http/pprof handler set on a subrouter
// mounted at /debug/pprof.
func pprofRoutes(pr chi.Router) {
	pr.HandleFunc("/", pprof.Index)
	pr.HandleFunc("/cmdline", pprof.Cmdline)
	pr.HandleFunc("/profile", pprof.Profile)
	pr.HandleFunc("/symbol", pprof.Symbol)
	pr.HandleFunc("/trace", pprof.Trace)
	// heap, goroutine, allocs, block, mutex and threadcreate are all served by
	// pprof.Index, which derives the profile name from the request path. chi
	// prefers the static patterns above over this wildcard.
	pr.HandleFunc("/{name}", pprof.Index)
}

// pprofTokenGate rejects any request that does not present the configured
// token in X-Pprof-Token or as an Authorization bearer value.
//
// Rejection is a 404 rather than a 401 so an unauthenticated caller cannot
// discover that profiling is enabled on this deployment; a probe sees the same
// response it would get if the routes had never been registered.
func pprofTokenGate(token string) func(http.Handler) http.Handler {
	want := []byte(token)
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			got := []byte(presentedPprofToken(r))
			// ConstantTimeCompare returns 0 on unequal lengths, but it is
			// documented as undefined for them, so the length check is explicit.
			if len(got) != len(want) || subtle.ConstantTimeCompare(got, want) != 1 {
				http.NotFound(w, r)
				return
			}
			next.ServeHTTP(w, r)
		})
	}
}

// presentedPprofToken extracts the token from the request headers. The token is
// kept out of the query string deliberately: query strings land in proxy access
// logs and browser history, and this one is a production secret.
func presentedPprofToken(r *http.Request) string {
	if v := r.Header.Get("X-Pprof-Token"); v != "" {
		return v
	}
	const bearer = "Bearer "
	if v := r.Header.Get("Authorization"); strings.HasPrefix(v, bearer) {
		return strings.TrimPrefix(v, bearer)
	}
	return ""
}
