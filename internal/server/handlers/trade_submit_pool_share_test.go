package handlers

import (
	"testing"

	"profitofexile/internal/db"
)

// The trade-submit writers are the one place in the server that starts a fixed
// number of connection-holding goroutines, so their count is spent against
// db.DefaultMaxConns whether or not anyone writes it down. Nobody did: the
// writers were sized at 4 "of the pool's 50" and the pool became 6 in an
// adjacent commit the same day, leaving four of six connections claimable by
// background writes.
//
// These tests are the mechanical form of that budget. They fail on a change to
// either side, which is what the two prose comments could not do.

// fenceConns is the connection cmd/server/main.go parks for the process-lifetime
// advisory fence. It is held for the whole run, so it is never available to a
// worker and never available to a request.
const fenceConns = 1

func TestSubmitWriters_TakeAtMostHalfTheUsablePool(t *testing.T) {
	usable := db.DefaultMaxConns - fenceConns
	if got, limit := defaultSubmitWriterConfig.workers, usable/2; got > limit {
		t.Fatalf("submit writers = %d, want at most %d (half of %d usable connections: DefaultMaxConns %d minus the fence). "+
			"Persisting a lookup is the only work here nobody is waiting on — the caller already has its 204 — so it takes the smaller share, not the larger",
			got, limit, usable, db.DefaultMaxConns)
	}
}

func TestSubmitWriters_LeaveTheRequestPathMoreThanTheyTake(t *testing.T) {
	left := db.DefaultMaxConns - fenceConns - defaultSubmitWriterConfig.workers
	if left <= defaultSubmitWriterConfig.workers {
		t.Fatalf("a full writer pool leaves %d connections against %d taken; at the default pool of %d "+
			"background writes would hold the majority of the process's connections during a submit burst",
			left, defaultSubmitWriterConfig.workers, db.DefaultMaxConns)
	}
}

// One worker would satisfy every arithmetic bound above and still stop draining
// for the whole insertTimeout on a single stuck insert.
func TestSubmitWriters_KeepADrainPathWhenOneWorkerStalls(t *testing.T) {
	if defaultSubmitWriterConfig.workers < 2 {
		t.Fatalf("submit writers = %d: one stuck insert holds the queue for the full insertTimeout (%s) with no second worker draining it",
			defaultSubmitWriterConfig.workers, defaultSubmitWriterConfig.insertTimeout)
	}
}
