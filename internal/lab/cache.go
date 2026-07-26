package lab

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"
	"sync"
	"time"

	"profitofexile/internal/league"
)

// Cache holds pre-computed analysis results in memory for instant API serving.
// Thread-safe via sync.RWMutex — writers take a write lock, readers take a read lock.
// Readers get a snapshot of the slice header; the underlying data is treated as
// immutable once stored.
//
// The sparkline maps follow the same contract: a writer builds replacement maps
// and series outside the lock and assigns them whole, so a reader holding a
// series it read earlier keeps a valid, unchanging view. Series are never
// mutated or compacted in place.
//
// A Cache is bound to exactly one league for its whole lifetime. scope is set
// at construction from the process-active league and never changes (scope is
// process-fixed until POE-121). Every read and write must go through For, which
// rejects access under any other league so a warm cache cannot serve one
// league's rows under another league's scope. See docs/adr/009.
type Cache struct {
	scope league.Scope

	mu          sync.RWMutex
	transfigure  []TransfigureResult
	fontSafe     []FontResult
	fontPremium  []FontResult
	fontJackpot  []FontResult
	quality      []QualityResult
	gemNames    []string // unique transfigured gem names, sorted
	lastUpdated time.Time
	nextFetch   time.Time
	divineRate      float64
	gcpPrice        float64
	offeringTiming  json.RawMessage // pre-computed offering timing JSON

	// Dedication lab analysis results.
	dedicationSkills       []DedicationResult
	dedicationTransfigured []DedicationResult
	corruptedGemNames            []string // corrupted non-transfigured gem names, sorted
	corruptedTransfiguredGemNames []string // corrupted transfigured gem names, sorted

	// V2 pre-computed results. These three fields are populated together by
	// Analyzer.RunV2 from the same snapshot time, but may be nil independently
	// during startup or if a pipeline stage fails.
	marketContext *MarketContext
	gemFeatures   []GemFeature
	gemSignals    []GemSignal

	// Rolling per-gem/variant price series, kept warm so sparkline requests do
	// not hit gem_snapshots. sparklineHighWater is the newest snapshot time
	// already folded in, so a repeated pass over the same snapshot only has to
	// query newer rows.
	sparklines          map[sparklineKey][]SparklinePoint
	sparklinesCorrupted map[sparklineKey][]SparklinePoint
	sparklineHighWater  time.Time
}

// NewCache creates an empty analysis cache bound to scope. The cache serves and
// stores only this league's data for its lifetime.
func NewCache(scope league.Scope) *Cache {
	return &Cache{scope: scope}
}

// For returns the cache for access under scope. The cache is bound to a single
// league (scope is process-fixed until POE-121), so a read or write under any
// other league is a tenancy violation and a programming error. For rejects it
// loudly rather than let a warm cache serve one league's rows under another
// league's scope — the leak that would otherwise appear once POE-121 enables
// league switching, because handlers read cache-first and only fall through to
// the now league-scoped repository on a cache miss.
func (c *Cache) For(scope league.Scope) *Cache {
	if scope.ID() != c.scope.ID() {
		panic(fmt.Sprintf("lab: cache bound to league %q accessed under league %q", c.scope.ID(), scope.ID()))
	}
	return c
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

// SetOfferingTiming stores pre-computed offering timing JSON.
func (c *Cache) SetOfferingTiming(data json.RawMessage) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.offeringTiming = data
}

// OfferingTiming returns cached offering timing JSON, or nil if not set.
func (c *Cache) OfferingTiming() json.RawMessage {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.offeringTiming
}

// SetGCPPrice stores the latest GCP price.
func (c *Cache) SetGCPPrice(price float64) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.gcpPrice = price
}

// GCPPrice returns the cached GCP price.
func (c *Cache) GCPPrice() float64 {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.gcpPrice
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

// SetDedication replaces the cached Dedication analysis results for both pools.
func (c *Cache) SetDedication(analysis DedicationAnalysis) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.dedicationSkills = analysis.Skills
	c.dedicationTransfigured = analysis.Transfigured
	c.lastUpdated = time.Now()
}

// Dedication returns the cached Dedication analysis (nil slices if empty).
func (c *Cache) Dedication() DedicationAnalysis {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return DedicationAnalysis{
		Skills:       c.dedicationSkills,
		Transfigured: c.dedicationTransfigured,
	}
}

// SetCorruptedGemNames stores autocomplete names for corrupted gem pools.
func (c *Cache) SetCorruptedGemNames(skills, transfigured []string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.corruptedGemNames = skills
	c.corruptedTransfiguredGemNames = transfigured
}

// CorruptedGemNames returns cached corrupted gem names for the given pool type.
func (c *Cache) CorruptedGemNames(isTransfigured bool) []string {
	c.mu.RLock()
	defer c.mu.RUnlock()
	if isTransfigured {
		return c.corruptedTransfiguredGemNames
	}
	return c.corruptedGemNames
}

// CorruptedGemNamesSearch returns corrupted gem names matching all query words (case-insensitive).
// Runs entirely in memory. Returns up to limit results.
func (c *Cache) CorruptedGemNamesSearch(query string, isTransfigured bool, limit int) []string {
	c.mu.RLock()
	var names []string
	if isTransfigured {
		names = c.corruptedTransfiguredGemNames
	} else {
		names = c.corruptedGemNames
	}
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

// SetSparklines replaces both sparkline maps and the high-water mark.
//
// Callers build the replacement maps outside the lock (see
// mergeSparklineSeries) and hand them over whole — this method assigns and
// nothing more, so the write lock is held for three assignments rather than for
// a corpus-wide merge. highWater is the newest snapshot time folded into the
// maps; pass the max time actually observed, so a repeated pass over an
// unchanged snapshot leaves it where it was.
func (c *Cache) SetSparklines(series, corrupted map[sparklineKey][]SparklinePoint, highWater time.Time) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.sparklines = series
	c.sparklinesCorrupted = corrupted
	c.sparklineHighWater = highWater
}

// Sparklines returns the cached non-corrupted series for a gem name and
// variant (nil when absent).
func (c *Cache) Sparklines(name, variant string) []SparklinePoint {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.sparklines[sparklineKey{name: name, variant: variant}]
}

// SparklinesCorrupted returns the cached corrupted series for a gem name and
// variant (nil when absent).
func (c *Cache) SparklinesCorrupted(name, variant string) []SparklinePoint {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.sparklinesCorrupted[sparklineKey{name: name, variant: variant}]
}

// SparklineHighWater returns the newest snapshot time folded into the sparkline
// maps (zero when never populated).
func (c *Cache) SparklineHighWater() time.Time {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.sparklineHighWater
}

// HasSparklines reports whether the sparkline cache has been populated at all.
// Handlers need this to tell a cold cache — where falling back to the database
// is correct — from a warm cache where a missing key genuinely means the gem
// has no recent points.
func (c *Cache) HasSparklines() bool {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return len(c.sparklines) > 0 || len(c.sparklinesCorrupted) > 0
}
