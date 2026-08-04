package db

import "testing"

// The constants below are not configuration — they are the measured production
// facts defaultMaxConns was chosen against (POE-154). They live in the test so
// that retuning the pool has to confront them: a change that violates one of
// these is a change that reintroduces either the 120-second pgbouncer stall or
// request starvation during a recompute.

const (
	// pgbouncerSharedBackends is default_pool_size (10) + reserve_pool_size (5)
	// on the shared instance. Server and collector use a byte-identical
	// DATABASE_URL, so they land in the same (user,db) pool and this is the
	// ceiling for both processes combined.
	pgbouncerSharedBackends = 15

	// poolClientProcesses is how many long-lived processes open a pool with
	// this default: the server and the collector.
	poolClientProcesses = 2

	// serverReservedConns is the server's non-request demand at peak: one
	// connection parked for the process-lifetime fence, one held under
	// DataLockKey by the delayed recompute, and three concurrent analysis
	// goroutines (RunTransfigure, RunQuality, RunV2 -> RunFont -> RunDedication)
	// fired from a single gem event.
	serverReservedConns = 5
)

func TestDefaultMaxConns_BothProcessesFitTheSharedPgbouncerPool(t *testing.T) {
	total := poolClientProcesses * defaultMaxConns
	if total > pgbouncerSharedBackends {
		t.Fatalf("defaultMaxConns = %d: %d processes advertise %d connections into a %d-backend pgbouncer pool; "+
			"exhaustion then queues in pgbouncer and fails after query_wait_timeout (120s) instead of failing locally in Go",
			defaultMaxConns, poolClientProcesses, total, pgbouncerSharedBackends)
	}
}

func TestDefaultMaxConns_LeavesAConnectionForRequestsDuringRecompute(t *testing.T) {
	if defaultMaxConns <= serverReservedConns {
		t.Fatalf("defaultMaxConns = %d but the server's fence, data lock and three analysis goroutines "+
			"can hold %d at once; a gem-event recompute would starve the request path of every connection",
			defaultMaxConns, serverReservedConns)
	}
}
