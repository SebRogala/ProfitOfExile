package coalesce

import (
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestKeepCritical_triggersDuringARunCauseExactlyOneRerun(t *testing.T) {
	var c Coalescer
	var calls atomic.Int32
	started := make(chan struct{})
	release := make(chan struct{})

	firstDone := make(chan struct{})
	go func() {
		defer close(firstDone)
		c.Run(func() {
			if calls.Add(1) == 1 {
				close(started)
				<-release
			}
		})
	}()
	<-started

	var wg sync.WaitGroup
	for i := 0; i < 5; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			c.Run(func() { calls.Add(1) })
		}()
	}
	waited := make(chan struct{})
	go func() { wg.Wait(); close(waited) }()
	select {
	case <-waited:
	case <-time.After(5 * time.Second):
		t.Fatal("concurrent Run calls blocked while a run was in flight")
	}

	close(release)
	select {
	case <-firstDone:
	case <-time.After(5 * time.Second):
		t.Fatal("first Run did not return after release")
	}

	if got := calls.Load(); got != 2 {
		t.Fatalf("run called %d times, want 2 (one run plus one coalesced rerun)", got)
	}
}

func TestKeepCritical_panickingRunDoesNotLeaveTheCoalescerRunning(t *testing.T) {
	var c Coalescer
	sentinel := "boom"

	recovered := func() (r any) {
		defer func() { r = recover() }()
		c.Run(func() { panic(sentinel) })
		return nil
	}()
	if recovered != sentinel {
		t.Fatalf("recovered %v, want %v", recovered, sentinel)
	}

	var calls int
	c.Run(func() { calls++ })
	if calls != 1 {
		t.Fatalf("run after panic called %d times, want 1", calls)
	}
}
