package lab

import (
	"sync"
	"time"
)

// Cache holds pre-computed analysis results in memory for instant API serving.
// Thread-safe via sync.RWMutex — writers take a write lock, readers take a read lock.
// Readers get a snapshot of the slice header; the underlying data is treated as
// immutable once stored.
type Cache struct {
	mu          sync.RWMutex
	transfigure []TransfigureResult
	font        []FontResult
	quality     []QualityResult
	trends      []TrendResult
	lastUpdated time.Time
	nextFetch   time.Time
}

// NewCache creates an empty analysis cache.
func NewCache() *Cache {
	return &Cache{}
}

// SetTransfigure replaces the cached transfigure results.
func (c *Cache) SetTransfigure(results []TransfigureResult) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.transfigure = results
	c.lastUpdated = time.Now()
}

// SetFont replaces the cached font results.
func (c *Cache) SetFont(results []FontResult) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.font = results
	c.lastUpdated = time.Now()
}

// SetQuality replaces the cached quality results.
func (c *Cache) SetQuality(results []QualityResult) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.quality = results
	c.lastUpdated = time.Now()
}

// SetTrends replaces the cached trend results.
func (c *Cache) SetTrends(results []TrendResult) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.trends = results
	c.lastUpdated = time.Now()
}

// Transfigure returns the cached transfigure results (nil if empty).
func (c *Cache) Transfigure() []TransfigureResult {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.transfigure
}

// Font returns the cached font results (nil if empty).
func (c *Cache) Font() []FontResult {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.font
}

// Quality returns the cached quality results (nil if empty).
func (c *Cache) Quality() []QualityResult {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.quality
}

// Trends returns the cached trend results (nil if empty).
func (c *Cache) Trends() []TrendResult {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.trends
}

// LastUpdated returns the time the cache was last updated.
func (c *Cache) LastUpdated() time.Time {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.lastUpdated
}

// SetNextFetch stores the next expected data fetch time.
func (c *Cache) SetNextFetch(t time.Time) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.nextFetch = t
}

// NextFetch returns the next expected data fetch time.
func (c *Cache) NextFetch() time.Time {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.nextFetch
}
