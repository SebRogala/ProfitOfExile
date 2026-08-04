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

// THE CACHE-STATE CONTRACT
//
// Canonical for how a caller learns whether the cache can answer.
// docs/ANALYSIS-CACHE.md points here for this rule and stays canonical for the
// process topology, the tick chain, the immutability contract, and the
// sparkline specifics — none of which have a code home.
//
// Two states, and only one of them justifies a query:
//
//	COLD            Nothing has been stored. The tick has not run yet (the
//	                server serves traffic against a cold cache after every
//	                deploy — see the cold-start window in the doc) or its stage
//	                failed. Only the database can answer, so falling through to
//	                the repository is correct.
//
//	WARM-AND-EMPTY  The tick ran, and its answer was empty. The database has
//	                nothing to add: the tick computed from it. Falling through
//	                re-runs the same query on every single request, forever,
//	                for an answer that cannot change before the next tick.
//
// A stored value cannot distinguish them. Nil, an empty slice and an empty map
// each mean both. Reading empty as COLD is the bug fixed one site at a time
// across POE-144, POE-152 and POE-158 — autocomplete, offering timings,
// transfigure, quality, and all three Dedication paths — and it is invisible in
// the response, because both states emit the same body. Reading empty as WARM
// is the opposite failure: an empty answer served with no fallback and no log
// for the whole cold-start window.
//
// THE RULE
//
//	The cache answers "am I warm". A handler never infers warmth from a value
//	it filtered.
//
// None of those defects was a bare cache read. Each was a handler deciding
// warmth from a *narrowed* value — the rows left after selecting one variant,
// the names left after matching a query. Those go to zero while the corpus
// behind them is full. Warmth is a property of the corpus, so only the field's
// owner can report it.
//
// TWO ACCESSOR SHAPES, CHOSEN BY ADDRESSING MODE
//
// Whole-corpus read — the caller takes everything and narrows it itself.
// Return (value, ok bool). ok is computed here from the stored corpus, before
// any narrowing. When ok is true the value is authoritative *including when it
// is empty* and the caller must not fall back; when ok is false the value is
// empty and the fallback is the only correct move. GemNamesSearch and
// CorruptedGemNamesSearch are the reference.
//
// Keyed read — the caller asks for one key of a map. Comma-ok is wrong here.
// ok would report "this key is present", a different question, and a caller
// reading it as warmth falls back per missing key: one cold read becomes one
// query per gem, and a warm cache with a genuinely absent gem re-queries for it
// on every request. Pair a plain accessor with a separate corpus-warmth
// predicate the caller checks first — Sparklines with HasSparklines is the
// reference. HasSparklinesCorruptedVariant shows where the corpus boundary
// sits: whatever the tick fills as a unit, which there is per variant, because
// a warm 21/23c map says nothing about 21/20c.
//
// ADDING A FIELD
//
//  1. Name the corpus: what does one tick fill as a unit? Two corpora filled by
//     two queries report warmth separately — one succeeding says nothing about
//     the other (CorruptedGemNamesSearch reports per pool for exactly this
//     reason).
//  2. Pick the shape by addressing mode above, not by taste.
//  3. Store the tick's answer even when it is empty. A writer that skips Set on
//     an empty result leaves the cache COLD, and then no reader discipline can
//     help: every request takes the fallback until the data happens to be
//     non-empty. This is the same defect on the writer side.
//  4. State in the accessor's doc comment what ok (or the predicate) reports
//     and what the caller must do with each answer.
//  5. Test both halves against whether the *database was reached* — warm
//     answers empty without querying, cold still falls back. Asserting on the
//     response shape passes with the bug present; that is why these shipped.
//     internal/server/handlers uses a nil *lab.Repository as the seam.

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

	// Dedication lab analysis results, plus the two request-shaped corpora
	// derived from the same pass. dedicationSet is the COLD/WARM discriminator
	// for all four fields — see SetDedication.
	dedicationSet          bool
	dedicationSkills       []DedicationResult
	dedicationTransfigured []DedicationResult
	dedicationRankings     map[string][]CollectiveResult // variant -> price-ranked corrupted gems
	dedicationGemPrices    map[string][]GemPrice         // variant -> that variant's corrupted gems
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

	// Bounded per-gem/variant signal transition rings, kept warm so the desktop's
	// three-per-scan history reads do not plan a hypertable query each time.
	signalHistory    map[signalKey][]SignalChange
	signalHistorySet bool

	// OCR gem-name dictionary, per pool. Monotonically grows as the tick unions
	// each snapshot's names into it — see populateGemDictionary.
	gemDictSkills       []string
	gemDictTransfigured []string
	gemDictSet          bool
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

// GemNamesSearch returns transfigured gem names matching all query words
// (case-insensitive). Runs entirely in memory — no DB query. Returns up to
// limit results.
//
// ok reports whether the cache holds a name corpus to search, and is the
// caller's cold-cache signal. It exists because the result alone cannot carry
// that signal: an empty result means "the cache is warm and nothing matches"
// just as often as it means "there is nothing cached yet", and reading it as
// the latter sent every non-matching keystroke of a debounced autocomplete to a
// DISTINCT ... ILIKE over gem_snapshots (POE-152). When ok is true the result
// is authoritative — including when it is empty — and the caller must not fall
// back to the repository. When ok is false the result is always empty.
//
// An empty query matches nothing and is answered from a warm cache, rather than
// being handed to the database as a search for everything.
func (c *Cache) GemNamesSearch(query string, limit int) (names []string, ok bool) {
	c.mu.RLock()
	corpus := c.gemNames
	c.mu.RUnlock()

	if len(corpus) == 0 {
		return nil, false
	}
	if query == "" {
		return nil, true
	}
	return searchNames(corpus, query, limit), true
}

// searchNames returns up to limit entries of names containing every word of
// query, case-insensitively, in any order. names is a snapshot of an immutable
// slice, so this runs outside the lock.
func searchNames(names []string, query string, limit int) []string {
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

// DedicationCorpus is everything one Dedication tick fills as a unit: the EV
// analysis both pools are served from, the price-ranked list behind
// /api/analysis/collective?mode=dedication, and this snapshot's corrupted gem
// prices behind /api/analysis/compare?mode=dedication.
//
// The three are one corpus, not three. All of them are derived in-process from
// the single gem snapshot AnalyzeDedication was handed, so there is no partial
// outcome where one is authoritative and another is not — which is why they
// share one warmth flag. See BuildDedicationCorpus.
//
// Rankings and GemPrices are keyed by the DB-format variant ("21/23c"). Both
// maps carry a key for every DedicationVariants entry, so a variant a caller may
// legitimately ask for (parseDedicationVariant validates against that same list)
// is always addressable, and an empty value under it means "this snapshot had no
// such gems" rather than "not computed".
type DedicationCorpus struct {
	Analysis  DedicationAnalysis
	Rankings  map[string][]CollectiveResult
	GemPrices map[string][]GemPrice
}

// SetDedication replaces the whole cached Dedication corpus and marks it warm.
//
// It stores the tick's answer as it stands, including an analysis with no rows
// and maps with no entries. That is the writer half of the cache-state contract:
// skipping the store on an empty result would leave the cache COLD, and every
// request would then take the database fallback forever for an answer the tick
// already computed.
func (c *Cache) SetDedication(corpus DedicationCorpus) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.dedicationSet = true
	c.dedicationSkills = corpus.Analysis.Skills
	c.dedicationTransfigured = corpus.Analysis.Transfigured
	c.dedicationRankings = corpus.Rankings
	c.dedicationGemPrices = corpus.GemPrices
	c.lastUpdated = time.Now()
}

// Dedication returns the cached Dedication analysis for every analyzed variant.
//
// ok reports whether a Dedication tick has stored a corpus, and is the caller's
// cold-cache signal. It is deliberately not derived from the returned slices:
// three handlers used to each compute `len(Skills) > 0 || len(Transfigured) > 0`
// for themselves, which is the handler-infers-warmth failure this package's
// contract names, and it also reads a genuinely empty analysis as COLD. When ok
// is true the analysis is authoritative — including when it is empty — and the
// caller must narrow it with FilterDedicationVariant rather than fall back; when
// ok is false the slices are nil and the fallback is the only correct move.
//
// ok is the same signal HasDedication reports; the keyed reads over the other
// two corpus fields take it from there, because comma-ok on a keyed read would
// answer "this variant is present" instead.
func (c *Cache) Dedication() (DedicationAnalysis, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return DedicationAnalysis{
		Skills:       c.dedicationSkills,
		Transfigured: c.dedicationTransfigured,
	}, c.dedicationSet
}

// HasDedication reports whether a Dedication tick has stored a corpus. It is the
// warmth predicate for the two keyed reads below — check it first, then read.
//
// It covers all of DedicationCorpus because one tick fills all of it from one
// snapshot: unlike the corrupted autocomplete pools, which come from two
// separate queries and so report warmth separately, there is no way for the
// rankings to be authoritative while the prices are not.
func (c *Cache) HasDedication() bool {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.dedicationSet
}

// DedicationRankings returns the pre-ranked corrupted gem list for one variant,
// price-descending and unfiltered — apply search and limit with
// FilterDedicationRankings, which is what RankDedicationCollective applies to
// the same list.
//
// Keyed read: check HasDedication first. A nil result from a warm cache means
// this snapshot held no rankable gems at variant, which the database would
// answer identically.
func (c *Cache) DedicationRankings(variant string) []CollectiveResult {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.dedicationRankings[variant]
}

// CorruptedGemPrices returns the latest snapshot's corrupted gem prices at one
// variant. It is the whole variant, not a name-narrowed slice: callers select
// the names they asked about with SelectGemPricesByNames.
//
// Keyed read: check HasDedication first.
func (c *Cache) CorruptedGemPrices(variant string) []GemPrice {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.dedicationGemPrices[variant]
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

// CorruptedGemNamesSearch returns corrupted gem names matching all query words
// (case-insensitive). Runs entirely in memory. Returns up to limit results.
//
// ok carries the same cold-cache signal as GemNamesSearch, reported per pool:
// the two corpora are populated from separate queries, so a warm transfigured
// pool says nothing about the skill pool. See GemNamesSearch for why the signal
// cannot ride on the result.
func (c *Cache) CorruptedGemNamesSearch(query string, isTransfigured bool, limit int) (names []string, ok bool) {
	c.mu.RLock()
	var corpus []string
	if isTransfigured {
		corpus = c.corruptedTransfiguredGemNames
	} else {
		corpus = c.corruptedGemNames
	}
	c.mu.RUnlock()

	if len(corpus) == 0 {
		return nil, false
	}
	if query == "" {
		return nil, true
	}
	return searchNames(corpus, query, limit), true
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

// SetSparklinesByName replaces both sparkline maps from name-then-variant keyed
// input. sparklineKey is unexported, so callers outside this package cannot
// build the maps SetSparklines takes; this is their entry point.
//
// Both maps are converted before the lock is taken, keeping the write lock down
// to the three assignments SetSparklines performs.
func (c *Cache) SetSparklinesByName(series, corrupted map[string]map[string][]SparklinePoint, highWater time.Time) {
	c.SetSparklines(keyBySparklineName(series), keyBySparklineName(corrupted), highWater)
}

// keyBySparklineName flattens name-then-variant nesting into sparklineKey keys.
// A nil input yields an empty map, so a caller warming only one of the two
// corpora still leaves the other addressable rather than nil.
func keyBySparklineName(byName map[string]map[string][]SparklinePoint) map[sparklineKey][]SparklinePoint {
	out := make(map[sparklineKey][]SparklinePoint, len(byName))
	for name, byVariant := range byName {
		for variant, points := range byVariant {
			out[sparklineKey{name: name, variant: variant}] = points
		}
	}
	return out
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

// HasSparklines reports whether the NON-corrupted sparkline map has been
// populated. Handlers need this to tell a cold cache — where falling back to the
// database is correct — from a warm cache where a missing key genuinely means
// the gem has no recent points.
//
// It reports on one corpus only. A caller reading the non-corrupted map must not
// be told the cache is warm because the corrupted map was filled: it would serve
// empty series with no fallback and no log. HasSparklinesCorruptedVariant is the
// same signal for the other corpus, asked per variant.
func (c *Cache) HasSparklines() bool {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return len(c.sparklines) > 0
}

// HasSparklinesCorruptedVariant reports whether the corrupted (Dedication)
// sparkline map holds any series for one variant. See HasSparklines for why the
// corrupted and non-corrupted corpora report separately.
//
// There is deliberately no corpus-wide counterpart. Once the Dedication pool
// became selectable, "the corrupted map has something in it" stopped being an
// answer to "can I serve this request": with 21/23c series in memory and no
// 21/20c ones, a corpus-level "warm" makes a 21/20c read return empty series
// with no database fallback and no warning — the caller cannot tell "this gem
// has no recent points" from "this whole variant was never populated".
func (c *Cache) HasSparklinesCorruptedVariant(variant string) bool {
	c.mu.RLock()
	defer c.mu.RUnlock()
	for k := range c.sparklinesCorrupted {
		if k.variant == variant {
			return true
		}
	}
	return false
}

// SetSignalHistory replaces the signal transition rings and marks them warm.
//
// Callers build the replacement map outside the lock (see
// populateSignalHistory) and hand it over whole; this method assigns and nothing
// more. Rings are never appended to or compacted in place — a reader holding a
// ring it read earlier keeps a valid, unchanging view.
func (c *Cache) SetSignalHistory(history map[signalKey][]SignalChange) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.signalHistory = history
	c.signalHistorySet = true
}

// SetSignalHistoryByName replaces the rings from name-then-variant keyed input.
// signalKey is unexported, so callers outside this package cannot build the map
// SetSignalHistory takes; this is their entry point, as SetSparklinesByName is
// for the sparkline maps.
func (c *Cache) SetSignalHistoryByName(byName map[string]map[string][]SignalChange) {
	out := make(map[signalKey][]SignalChange, len(byName))
	for name, byVariant := range byName {
		for variant, changes := range byVariant {
			out[signalKey{name: name, variant: variant}] = changes
		}
	}
	c.SetSignalHistory(out)
}

// SignalHistory returns the cached transitions for one gem and variant, newest
// first, or nil when the ring holds none.
//
// Keyed read: check HasSignalHistory first. On a warm cache a nil result means
// this gem has produced no signal rows, which is what the query would return
// too; reading nil as cold would send one query per gem on every scan.
func (c *Cache) SignalHistory(name, variant string) []SignalChange {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.signalHistory[signalKey{name: name, variant: variant}]
}

// HasSignalHistory reports whether the signal rings have been populated. It is
// the corpus-warmth predicate SignalHistory's callers check first.
//
// One corpus: the rings are filled for every gem the tick computed signals for,
// in one assignment, so warmth cannot be true for one gem and false for another.
func (c *Cache) HasSignalHistory() bool {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.signalHistorySet
}

// SetGemDictionary replaces both OCR dictionary pools and marks them warm.
//
// Both pools are stored in one call because the population step commits both or
// neither: its seed reads them with two queries and abandons the whole store if
// either fails, so there is no state where one pool is authoritative and the
// other was never filled. Contrast SetCorruptedGemNames, whose two pools each
// keep the previous value when their own query fails and therefore have to be
// able to report warmth apart.
func (c *Cache) SetGemDictionary(skills, transfigured []string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.gemDictSkills = skills
	c.gemDictTransfigured = transfigured
	c.gemDictSet = true
}

// GemDictionary returns the OCR gem-name dictionary for one pool, sorted.
//
// ok reports whether the dictionary has been populated and is the caller's
// cold-cache signal; the names alone cannot carry it, since a league whose
// snapshots and gem_colors are both empty produces the same empty list as a
// process that has not ticked yet. When ok is true the list is authoritative and
// the caller must not fall back.
//
// The dictionary is league-scoped and market-derived, not static game data: it
// is gem_colors unioned with the names this league's market has shown, so it
// belongs to the league-bound cache and grows as the tick sees new names.
func (c *Cache) GemDictionary(transfigured bool) (names []string, ok bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	if !c.gemDictSet {
		return nil, false
	}
	if transfigured {
		return c.gemDictTransfigured, true
	}
	return c.gemDictSkills, true
}
