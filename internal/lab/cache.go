package lab

import (
	"sort"
	"strings"
	"sync"
	"time"
)

// Cache holds pre-computed analysis results in memory for instant API serving.
// Thread-safe via sync.RWMutex — writers take a write lock, readers take a read lock.
// Readers get a snapshot of the slice header; the underlying data is treated as
// immutable once stored.
type Cache struct {
	mu          sync.RWMutex
	transfigure  []TransfigureResult
	fontSafe     []FontResult
	fontPremium  []FontResult
	fontJackpot  []FontResult
	quality      []QualityResult
	trends      []TrendResult
	gemNames    []string // unique transfigured gem names, sorted
	lastUpdated time.Time
	nextFetch   time.Time
	divineRate  float64

	// V2 pre-computed results. These three fields are populated together by
	// Analyzer.RunV2 from the same snapshot time, but may be nil independently
	// during startup or if a pipeline stage fails.
	marketContext *MarketContext
	gemFeatures   []GemFeature
	gemSignals    []GemSignal
}

// NewCache creates an empty analysis cache.
func NewCache() *Cache {
	return &Cache{}
}

// SetTransfigure replaces the cached transfigure results.
func (c *Cache) SetTransfigure(results []TransfigureResult) {
	// Extract unique gem names for autocomplete.
	seen := make(map[string]struct{}, len(results))
	for _, r := range results {
		seen[r.TransfiguredName] = struct{}{}
	}
	names := make([]string, 0, len(seen))
	for n := range seen {
		names = append(names, n)
	}
	sort.Strings(names)

	c.mu.Lock()
	defer c.mu.Unlock()
	c.transfigure = results
	c.gemNames = names
	c.lastUpdated = time.Now()
}

// SetFont replaces the cached font results for all three modes.
func (c *Cache) SetFont(analysis FontAnalysis) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.fontSafe = analysis.Safe
	c.fontPremium = analysis.Premium
	c.fontJackpot = analysis.Jackpot
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

// Font returns the cached font analysis with all three modes (nil slices if empty).
func (c *Cache) Font() FontAnalysis {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return FontAnalysis{Safe: c.fontSafe, Premium: c.fontPremium, Jackpot: c.fontJackpot}
}

// FontSafe returns the cached safe mode font results (nil if empty).
func (c *Cache) FontSafe() []FontResult {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.fontSafe
}

// FontPremium returns the cached premium mode font results (nil if empty).
func (c *Cache) FontPremium() []FontResult {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.fontPremium
}

// FontJackpot returns the cached jackpot mode font results (nil if empty).
func (c *Cache) FontJackpot() []FontResult {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.fontJackpot
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

// GemNamesSearch returns transfigured gem names matching all query words (case-insensitive).
// Runs entirely in memory — no DB query. Returns up to limit results.
func (c *Cache) GemNamesSearch(query string, limit int) []string {
	c.mu.RLock()
	names := c.gemNames
	c.mu.RUnlock()

	if len(names) == 0 || query == "" {
		return nil
	}

	words := strings.Fields(strings.ToLower(query))
	var results []string
	for _, name := range names {
		lower := strings.ToLower(name)
		match := true
		for _, w := range words {
			if !strings.Contains(lower, w) {
				match = false
				break
			}
		}
		if match {
			results = append(results, name)
			if len(results) >= limit {
				break
			}
		}
	}
	return results
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

// SetDivineRate stores the latest divine→chaos exchange rate.
func (c *Cache) SetDivineRate(rate float64) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.divineRate = rate
}

// DivineRate returns the cached divine→chaos exchange rate.
func (c *Cache) DivineRate() float64 {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.divineRate
}

// SetMarketContext replaces the cached market context.
func (c *Cache) SetMarketContext(mc *MarketContext) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.marketContext = mc
	c.lastUpdated = time.Now()
}

// MarketContext returns the cached market context (nil if empty).
func (c *Cache) MarketContext() *MarketContext {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.marketContext
}

// SetGemFeatures replaces the cached gem features.
func (c *Cache) SetGemFeatures(features []GemFeature) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.gemFeatures = features
	c.lastUpdated = time.Now()
}

// GemFeatures returns the cached gem features (nil if empty).
func (c *Cache) GemFeatures() []GemFeature {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.gemFeatures
}

// SetGemSignals replaces the cached gem signals.
func (c *Cache) SetGemSignals(signals []GemSignal) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.gemSignals = signals
	c.lastUpdated = time.Now()
}

// GemSignals returns the cached gem signals (nil if empty).
func (c *Cache) GemSignals() []GemSignal {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.gemSignals
}
