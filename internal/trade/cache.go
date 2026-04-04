package trade

import (
	"sync"
	"time"
)

// TradeCache is a concurrency-safe LRU cache for trade lookup results.
// All operations acquire a write lock because even Get promotes the
// accessed entry (LRU touch), so sync.RWMutex would not help.
type TradeCache struct {
	mu      sync.Mutex
	entries map[string]*cacheEntry
	order   []string // LRU order: oldest at [0], newest at end
	maxSize int
}

type cacheEntry struct {
	Result *TradeLookupResult
}

// NewTradeCache creates an LRU cache that evicts the least-recently-used
// entry once maxSize is reached.
func NewTradeCache(maxSize int) *TradeCache {
	return &TradeCache{
		entries: make(map[string]*cacheEntry, maxSize),
		order:   make([]string, 0, maxSize),
		maxSize: maxSize,
	}
}

// Get retrieves a cached result and promotes the key to most-recently-used.
// Returns nil, false on cache miss.
func (c *TradeCache) Get(key string) (*TradeLookupResult, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()

	e, ok := c.entries[key]
	if !ok {
		return nil, false
	}

	c.promote(key)
	return e.Result, true
}

// Set inserts or updates a cache entry. If the key already exists, its result
// is updated and the key is promoted. If the cache is at capacity, the
// least-recently-used entry is evicted first.
func (c *TradeCache) Set(key string, result *TradeLookupResult) {
	c.mu.Lock()
	defer c.mu.Unlock()

	if _, ok := c.entries[key]; ok {
		c.entries[key].Result = result
		c.promote(key)
		return
	}

	if len(c.entries) >= c.maxSize {
		c.evictOldest()
	}

	c.entries[key] = &cacheEntry{Result: result}
	c.order = append(c.order, key)
}

// Delete removes a key from the cache. No-op if the key does not exist.
func (c *TradeCache) Delete(key string) {
	c.mu.Lock()
	defer c.mu.Unlock()

	if _, ok := c.entries[key]; !ok {
		return
	}

	delete(c.entries, key)
	c.removeFromOrder(key)
}

// Warm loads trade lookup results into the cache (e.g. from DB on startup).
func (c *TradeCache) Warm(results []TradeLookupResult) int {
	loaded := 0
	for i := range results {
		key := CacheKey(results[i].Gem, results[i].Variant)
		c.Set(key, &results[i])
		loaded++
	}
	return loaded
}

// Len returns the number of entries in the cache.
func (c *TradeCache) Len() int {
	c.mu.Lock()
	defer c.mu.Unlock()
	return len(c.entries)
}

// promote moves key to the end of the order slice (most-recently-used).
// Caller must hold c.mu.
func (c *TradeCache) promote(key string) {
	c.removeFromOrder(key)
	c.order = append(c.order, key)
}

// removeFromOrder removes the first occurrence of key from the order slice.
// Caller must hold c.mu.
func (c *TradeCache) removeFromOrder(key string) {
	for i, k := range c.order {
		if k == key {
			c.order = append(c.order[:i], c.order[i+1:]...)
			return
		}
	}
}

// OldestStale returns the cache key whose FetchedAt is oldest among entries
// matching the filter. Only entries older than minAge are considered.
// Returns ("", false) if nothing qualifies.
func (c *TradeCache) OldestStale(minAge time.Duration, filter func(key string, r *TradeLookupResult) bool) (string, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()

	cutoff := time.Now().Add(-minAge)
	var oldestKey string
	var oldestTime time.Time

	for key, e := range c.entries {
		if e.Result.FetchedAt.After(cutoff) {
			continue // too fresh
		}
		if filter != nil && !filter(key, e.Result) {
			continue
		}
		if oldestKey == "" || e.Result.FetchedAt.Before(oldestTime) {
			oldestKey = key
			oldestTime = e.Result.FetchedAt
		}
	}
	return oldestKey, oldestKey != ""
}

// GetSnapshot returns a shallow copy of all cached entries keyed by cache key.
// Unlike Get(), this does NOT promote entries (no LRU touch), so it acquires
// the lock only once for the entire batch — ideal for analysis pipelines that
// need to read all entries without 500 individual write-lock acquisitions.
func (c *TradeCache) GetSnapshot() map[string]*TradeLookupResult {
	c.mu.Lock()
	defer c.mu.Unlock()

	snap := make(map[string]*TradeLookupResult, len(c.entries))
	for k, e := range c.entries {
		snap[k] = e.Result
	}
	return snap
}

// evictOldest removes the least-recently-used entry (order[0]).
// Caller must hold c.mu.
func (c *TradeCache) evictOldest() {
	if len(c.order) == 0 {
		return
	}
	oldest := c.order[0]
	c.order = c.order[1:]
	delete(c.entries, oldest)
}
