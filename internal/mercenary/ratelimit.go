package mercenary

import (
	"sync"
	"time"
)

// Default write budget for one device: 60 requests per 10 minutes.
//
// Sized against the real corpus rather than against a guess. A device holding
// the full local store (459 keys, up to 1377 signatures — families x tiers x
// MaxSamplesPerKey, see lockPoolSQL) needs 22 requests to publish everything at
// the server's own MaxTemplatesPerUpload cap, or 43 at the 32 per batch the
// desktop actually sends; a device that is merely learning sends one request per
// new hover. So the worst realistic publish leaves 17 of the 60 for a session's
// hovers — headroom, but not much of it, and a client that batched below 32
// would run out. 60 still bounds a spoofed fingerprint to 60 requests before it
// has to wait.
const (
	DefaultUploadBudget = 60
	DefaultUploadWindow = 10 * time.Minute
	// DefaultRateLimitDevices bounds the bucket map, the same safety valve the
	// device middleware puts on its cache: a flood of distinct well-formed
	// fingerprints must not grow memory without limit.
	DefaultRateLimitDevices = 8192
)

// RateLimiter is a per-device token bucket over write requests.
//
// It is in-memory and therefore per-process: with one server this is the whole
// limit, and if the server is ever replicated the budget multiplies by the
// replica count. That is acceptable for what it defends — the cap and the
// tombstone are what actually bound a bad actor's effect on the pool; this only
// bounds the traffic it takes to find that out.
type RateLimiter struct {
	capacity     float64
	refillPerSec float64
	maxDevices   int
	now          func() time.Time

	mu      sync.Mutex
	buckets map[string]*bucket
}

type bucket struct {
	tokens float64
	last   time.Time
}

// NewRateLimiter builds a limiter granting budget requests per window, holding
// at most maxDevices buckets.
func NewRateLimiter(budget int, window time.Duration, maxDevices int) *RateLimiter {
	return &RateLimiter{
		capacity:     float64(budget),
		refillPerSec: float64(budget) / window.Seconds(),
		maxDevices:   maxDevices,
		now:          time.Now,
		buckets:      make(map[string]*bucket),
	}
}

// Allow spends one token for a device. When it returns false the second value
// is how long until a token is available, for a Retry-After header.
func (l *RateLimiter) Allow(device string) (bool, time.Duration) {
	now := l.now()

	l.mu.Lock()
	defer l.mu.Unlock()

	b, known := l.buckets[device]
	if !known {
		if len(l.buckets) >= l.maxDevices && !l.evictIdleLocked() {
			// Every tracked device is actively spending its budget and the map
			// is full. Refusing is the point: this is the shape a spoofed-
			// fingerprint flood takes, and admitting it would trade the memory
			// bound for an unbounded map.
			return false, l.windowLocked()
		}
		b = &bucket{tokens: l.capacity, last: now}
		l.buckets[device] = b
	}

	elapsed := now.Sub(b.last).Seconds()
	if elapsed > 0 {
		b.tokens += elapsed * l.refillPerSec
		if b.tokens > l.capacity {
			b.tokens = l.capacity
		}
		b.last = now
	}

	if b.tokens < 1 {
		deficit := 1 - b.tokens
		return false, time.Duration(deficit/l.refillPerSec*float64(time.Second)) + time.Second
	}
	b.tokens--
	return true, 0
}

// evictIdleLocked drops buckets that are back at full budget — devices that
// have stopped spending. Reports whether it freed anything.
func (l *RateLimiter) evictIdleLocked() bool {
	now := l.now()
	freed := false
	for key, b := range l.buckets {
		if b.tokens+now.Sub(b.last).Seconds()*l.refillPerSec >= l.capacity {
			delete(l.buckets, key)
			freed = true
		}
	}
	return freed
}

// windowLocked is one full refill window, the wait handed to a caller the
// limiter could not even track.
func (l *RateLimiter) windowLocked() time.Duration {
	return time.Duration(l.capacity / l.refillPerSec * float64(time.Second))
}
