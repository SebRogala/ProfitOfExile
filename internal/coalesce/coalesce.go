// Package coalesce owns the "recompute, coalescing concurrent triggers into
// one rerun" loop shared by the feature services. It imports only the standard
// library so any package may embed it without crossing a feature boundary.
package coalesce

import "sync"

// Coalescer runs a function, coalescing concurrent requests. Its zero value is
// ready to use.
//
// A catch-up pass stores several hours in a row and the subscriber fires one
// trigger per stored hour. Running them all would mean N full window reads for
// one answer, so: while a run is in flight, further calls only mark it dirty
// and return immediately, and the running call then repeats ONCE for however
// many arrived. The rerun is what keeps the last event from being lost — the
// in-flight read may have started before its rows were committed.
//
// This is a mutex plus a rerun, not a debounce or a timer: nothing waits, and
// the mutex is never held while the function runs. A panicking run resets the
// state so later calls are not dropped; the panic propagates to the caller.
type Coalescer struct {
	mu      sync.Mutex
	running bool
	dirty   bool
}

// Run calls run, or, when a call is already in flight, marks it for one rerun
// and returns immediately.
func (c *Coalescer) Run(run func()) {
	c.mu.Lock()
	if c.running {
		c.dirty = true
		c.mu.Unlock()
		return
	}
	c.running = true
	c.mu.Unlock()

	done := false
	defer func() {
		if done {
			return
		}
		c.mu.Lock()
		c.running = false
		c.dirty = false
		c.mu.Unlock()
	}()

	for {
		run()

		c.mu.Lock()
		if c.dirty {
			c.dirty = false
			c.mu.Unlock()
			continue
		}
		c.running = false
		c.mu.Unlock()
		done = true
		return
	}
}
