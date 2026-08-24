package lab

import (
	"encoding/json"
	"fmt"
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
//	it read.
//
// The instances found first were all a handler deciding warmth from a *narrowed*
// value — the rows left after selecting one variant, the names left after
// matching a query — which goes to zero while the corpus behind it is full.
// Narrowing is what made those easy to hit, not what made them wrong. A bare
// len(corpus) > 0 on an un-narrowed read is the same defect with a rarer
// trigger: it needs a tick whose own answer was empty rather than a filter that
// matched nothing, and when that happens it fails identically and permanently.
//
// This rule was first written down while eight accessors still returned a bare
// value with no warmth signal at all, which left the nine handlers reading them
// nothing but length to test — the rule was stated as settled before the type
// could honour it. Every accessor here now reports its own warmth. Warmth is a
// property of the corpus, and only the field's owner can report it.
//
// TWO ACCESSOR SHAPES, CHOSEN BY ADDRESSING MODE
//
// Whole-corpus read — the caller takes everything and narrows it itself.
// Return (value, ok bool). ok is computed here from the stored corpus, before
// any narrowing. When ok is true the value is authoritative *including when it
// is empty* and the caller must not fall back; when ok is false the value is
// empty and the fallback is the only correct move. Transfigure, Font, Quality,
// GemFeatures, GemSignals, Dedication, DoubleCorrupt, GemDictionary,
// GemNamesSearch and CorruptedGemNamesSearch all take this shape.
//
// Keyed read — the caller asks for one key of a map. Comma-ok is wrong here.
// ok would report "this key is present", a different question, and a caller
// reading it as warmth falls back per missing key: one cold read becomes one
// query per gem, and a warm cache with a genuinely absent gem re-queries for it
// on every request. Pair a plain accessor with a separate corpus-warmth
// predicate the caller checks first — SignalHistory with HasSignalHistory is the
// reference: one stored flag, checked before the keyed read.
//
// The sparkline pair is the other instance, and shows that a corpus is not
// always one field: two maps, one predicate, because one SparklineWindow read
// fills both — see HasSparklines.
//
// One exception, and only one: a single-value field whose writer never stores a
// zero value carries the signal honestly by itself. MarketContext is a
// *MarketContext the tick only ever sets to a real struct, so nil means COLD and
// can mean nothing else. That is a property of the writer rather than of the
// read, so it is stated at the accessor instead of left for a caller to work out.
//
// ADDING A FIELD
//
//  1. Name the corpus: what does one tick fill as a unit? Two corpora filled by
//     two queries report warmth separately — one succeeding says nothing about
//     the other (CorruptedGemNamesSearch reports per pool for exactly this
//     reason). Fields one call fills together share one flag, as the Dedication
//     corpus and the three Font modes do.
//  2. Pick the shape by addressing mode above, not by taste. An accessor that
//     returns a bare value is not a lighter version of this contract, it is a
//     handler with no way to follow it.
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

	mu sync.RWMutex

	transfigureSet bool
	transfigure    []TransfigureResult

	// The Font (normal-mode) autocomplete pool, filled by its own query and so
	// carrying its own flag — see SetGemNamePool.
	gemNamesSet bool
	gemNames    []string // eligible transfigured gem names, sorted

	// One SetFont fills all three modes from a single AnalyzeFont pass, so they
	// share one flag: there is no outcome where Safe is authoritative and
	// Jackpot was never computed.
	fontSet     bool
	fontSafe    []FontResult
	fontPremium []FontResult
	fontJackpot []FontResult

	qualitySet bool
	quality    []QualityResult

	lastUpdated    time.Time
	nextFetch      time.Time
	divineRate     float64
	gcpPrice       float64
	offeringTiming json.RawMessage // pre-computed offering timing JSON

	// Dedication lab analysis results, plus the two request-shaped corpora
	// derived from the same pass. dedicationSet is the COLD/WARM discriminator
	// for all four fields — see SetDedication.
	dedicationSet          bool
	dedicationSkills       []DedicationResult
	dedicationTransfigured []DedicationResult
	dedicationRankings     map[string][]CollectiveResult // variant -> price-ranked corrupted gems
	dedicationGemPrices    map[string][]GemPrice         // variant -> that variant's corrupted gems
	// Double-corruption (Doryani's Institute) EV results, plus the per-input-
	// variant view derived from the same pass. doubleCorruptSet is the
	// COLD/WARM discriminator for both fields — see SetDoubleCorrupt.
	doubleCorruptSet       bool
	doubleCorruptResults   []DoubleCorruptResult
	doubleCorruptByVariant map[string][]DoubleCorruptResult // input variant -> profit-ranked results
	// Corrupted autocomplete pools. Two queries fill these, either of which can
	// fail on its own, so each pool carries its own flag — see
	// SetCorruptedGemNamePool.
	corruptedNamesSet             bool
	corruptedGemNames             []string // corrupted non-transfigured gem names, sorted
	corruptedTransfiguredNamesSet bool
	corruptedTransfiguredGemNames []string // corrupted transfigured gem names, sorted

	// V2 pre-computed results. RunV2 computes all three from the same snapshot
	// but stores them in three separate calls, each after a persist that can
	// fail the run — so they report warmth separately rather than sharing a flag
	// the way the Font modes do. marketContext needs no flag: RunV2 only ever
	// stores a real struct, so nil is an unambiguous COLD.
	marketContext  *MarketContext
	gemFeaturesSet bool
	gemFeatures    []GemFeature
	gemSignalsSet  bool
	gemSignals     []GemSignal

	// Rolling per-gem/variant price series, kept warm so sparkline requests do
	// not hit gem_snapshots. One SparklineWindow read fills both maps, so
	// sparklinesSet is the COLD/WARM discriminator for both — see HasSparklines.
	// sparklineHighWater is the newest snapshot time already folded in, so a
	// repeated pass over the same snapshot only has to query newer rows.
	sparklinesSet       bool
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
	c.mu.Lock()
	defer c.mu.Unlock()
	c.transfigureSet = true
	c.transfigure = results
	c.lastUpdated = time.Now()
}

// SetGemNamePool stores the Font (normal-mode) autocomplete pool and marks it
// warm.
//
// It takes the names rather than deriving them, and that is the point. The pool
// is what GemNamesSearch answers nearly every keystroke from, and the cold
// fallback behind it is Repository.GemNamesAutocomplete; if the two are filled
// by two different rules they answer the same query two different ways, and the
// cold path — the one every integration test exercises — is not the one users
// hit. So the caller fills this from that same query (RunTransfigure), and the
// eligibility rules live in one place: the SQL fragments in eligibility.go.
//
// Derived from the transfigure results until POE-142, which is how the picker
// came to offer a gem the Font cannot hand out: those results are gated on
// isTransfigurePairCandidate, which carries neither the support nor the WHITE
// rule.
//
// It carries its own flag for the same reason SetCorruptedGemNamePool does: one
// query fills it, that query can fail on its own, and a caller that skips this
// call on failure leaves the previous names and the previous warmth alone.
//
// It stores an empty result as readily as a full one — see
// SetCorruptedGemNamePool.
func (c *Cache) SetGemNamePool(names []string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.gemNames = names
	c.gemNamesSet = true
}

// SetFont replaces the cached font results for all three modes and marks them
// warm. It stores the analysis as it stands, empty modes included — see the
// writer half of the cache-state contract at the top of this file.
func (c *Cache) SetFont(analysis FontAnalysis) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.fontSet = true
	c.fontSafe = analysis.Safe
	c.fontPremium = analysis.Premium
	c.fontJackpot = analysis.Jackpot
	c.lastUpdated = time.Now()
}

// SetQuality replaces the cached quality results and marks them warm.
func (c *Cache) SetQuality(results []QualityResult) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.qualitySet = true
	c.quality = results
	c.lastUpdated = time.Now()
}

// Transfigure returns every cached transfigure result, for the caller to narrow
// by variant and limit with filterTransfigure.
//
// ok reports whether a transfigure tick has stored results, and is the caller's
// cold-cache signal. The results alone cannot carry it: AnalyzeTransfigure over a
// snapshot holding no priced transfigured gem returns nothing, which is the same
// empty slice a process that has never ticked holds. When ok is true the results
// are authoritative — including when they are empty, and including for a variant
// none of them carry, since one pass computes every variant — and the caller must
// not fall back. When ok is false the slice is nil.
func (c *Cache) Transfigure() ([]TransfigureResult, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.transfigure, c.transfigureSet
}

// Font returns the cached font analysis with all three modes.
//
// ok reports whether a font tick has stored an analysis. One AnalyzeFont pass
// fills all three modes, so they cannot disagree about warmth: a run that
// classified no winner at all stores three empty modes, and that is an answer.
// When ok is true the analysis is authoritative and the caller must not fall
// back; when ok is false every mode is nil.
func (c *Cache) Font() (FontAnalysis, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return FontAnalysis{Safe: c.fontSafe, Premium: c.fontPremium, Jackpot: c.fontJackpot}, c.fontSet
}

// Quality returns every cached quality-roll result, for the caller to narrow by
// level and limit with filterQuality.
//
// ok reports whether a quality tick has stored results and carries the same
// cold-cache signal as Transfigure, for the same reason.
func (c *Cache) Quality() ([]QualityResult, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.quality, c.qualitySet
}

// GemNamesSearch returns transfigured gem names matching all query words
// (case-insensitive). Runs entirely in memory — no DB query. Returns up to
// limit results.
//
// The corpus is Repository.GemNamesAutocomplete's own answer, stored by
// SetGemNamePool — the cold fallback behind this method is that same query, so
// the two cannot offer different gems. See SetGemNamePool.
//
// ok reports whether a tick has stored that corpus, and is the caller's
// cold-cache signal. It exists because the result alone cannot carry that
// signal: an empty result means "the cache is warm and nothing matches" just as
// often as it means "there is nothing cached yet", and reading it as the latter
// sent every non-matching keystroke of a debounced autocomplete to a DISTINCT
// ... ILIKE over gem_snapshots (POE-152). When ok is true the result is
// authoritative — including when it is empty — and the caller must not fall back
// to the repository. When ok is false the result is always empty.
//
// ok is the stored flag rather than the size of the corpus: a snapshot with no
// priced transfigured gem yields no names, and re-deriving warmth from that put
// the same permanent fallback back one level up.
//
// An empty query matches nothing and is answered from a warm cache, rather than
// being handed to the database as a search for everything.
func (c *Cache) GemNamesSearch(query string, limit int) (names []string, ok bool) {
	c.mu.RLock()
	corpus := c.gemNames
	set := c.gemNamesSet
	c.mu.RUnlock()

	if !set {
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

// OfferingTiming returns cached offering timing JSON, or nil when nothing has
// stored one.
//
// The other single-value exception in the cache-state contract, and it holds only
// because every writer marshals a never-nil slice: an answer with no offerings in
// it is stored as `[]`, which is two bytes and not nil. So nil means COLD, and
// MarketOverview's fallback — ten queries — is reached exactly once rather than
// on every request. ComputeOfferingTimings returning a non-nil empty slice is
// what makes that true; see its doc comment.
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

// MarketContext returns the cached market context, or nil when no RunV2 has
// stored one.
//
// This is the single-value exception in the cache-state contract: RunV2 only ever
// stores a real struct, so nil here means COLD and cannot also mean
// warm-and-empty. A nil result is the caller's signal to fall back; a non-nil one
// is authoritative. It needs no companion flag for that reason and no other.
func (c *Cache) MarketContext() *MarketContext {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.marketContext
}

// SetGemFeatures replaces the cached gem features and marks them warm.
func (c *Cache) SetGemFeatures(features []GemFeature) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.gemFeaturesSet = true
	c.gemFeatures = features
	c.lastUpdated = time.Now()
}

// GemFeatures returns every cached gem feature, for the caller to narrow by
// variant and tier with filterGemFeatures.
//
// ok reports whether a RunV2 has stored features. It is separate from the signal
// flag on purpose: RunV2 stores the two in separate calls with a persist between
// them that can fail the run, so a warm feature corpus says nothing about the
// signals. When ok is true the features are authoritative — including when they
// are empty — and the caller must not fall back.
func (c *Cache) GemFeatures() ([]GemFeature, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.gemFeatures, c.gemFeaturesSet
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

// DoubleCorruptCorpus is everything one double-corruption tick fills as a unit:
// the profit-ranked results behind the ranking read, and the same results split
// by input variant for the request paths that must stay inside one market.
//
// The two are one corpus, not two. ByVariant is a partition of Results computed
// in the same pass from the same snapshot, so there is no outcome where one is
// authoritative and the other is not — which is why they share one warmth flag.
// See BuildDoubleCorruptCorpus.
//
// ByVariant carries a key for every DoubleCorruptVariants entry, so an input
// variant a caller may legitimately ask for is always addressable, and an empty
// value under it means "this snapshot priced no gems at that variant" rather
// than "not computed".
type DoubleCorruptCorpus struct {
	Results   []DoubleCorruptResult
	ByVariant map[string][]DoubleCorruptResult
}

// BuildDoubleCorruptCorpus partitions one analysis pass into the corpus the
// cache stores. Every DoubleCorruptVariants entry gets a key even when no gem
// priced at it, so a keyed read can never confuse "no gems" with "not computed".
func BuildDoubleCorruptCorpus(results []DoubleCorruptResult) DoubleCorruptCorpus {
	byVariant := make(map[string][]DoubleCorruptResult, len(DoubleCorruptVariants))
	for _, v := range DoubleCorruptVariants {
		byVariant[v] = nil
	}
	for _, dc := range results {
		byVariant[dc.InputVariant] = append(byVariant[dc.InputVariant], dc)
	}
	return DoubleCorruptCorpus{Results: results, ByVariant: byVariant}
}

// SetDoubleCorrupt replaces the whole cached double-corruption corpus and marks
// it warm.
//
// It stores the tick's answer as it stands, including a corpus with no results.
// That is the writer half of the cache-state contract: skipping the store on an
// empty result would leave the cache COLD, and every request would then take the
// database fallback forever for an answer the tick already computed.
func (c *Cache) SetDoubleCorrupt(corpus DoubleCorruptCorpus) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.doubleCorruptSet = true
	c.doubleCorruptResults = corpus.Results
	c.doubleCorruptByVariant = corpus.ByVariant
	c.lastUpdated = time.Now()
}

// DoubleCorrupt returns every cached double-corruption result, profit-ranked
// across all analyzed input variants.
//
// ok reports whether a double-corruption tick has stored a corpus, and is the
// caller's cold-cache signal. It is deliberately not derived from the returned
// slice: an empty analysis is a real answer, and reading it as COLD sends every
// request back to the database for the rest of the tick. When ok is true the
// results are authoritative — including when empty — and a caller wanting one
// market must narrow with DoubleCorruptByVariant rather than fall back; when ok
// is false the slice is nil and the fallback is the only correct move.
func (c *Cache) DoubleCorrupt() ([]DoubleCorruptResult, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.doubleCorruptResults, c.doubleCorruptSet
}

// HasDoubleCorrupt reports whether a double-corruption tick has stored a corpus.
// It is the warmth predicate for the keyed read below — check it first, then
// read.
//
// It covers all of DoubleCorruptCorpus because one tick fills all of it from one
// snapshot.
func (c *Cache) HasDoubleCorrupt() bool {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.doubleCorruptSet
}

// DoubleCorruptByVariant returns the profit-ranked results for one uncorrupted
// input variant. Callers narrow to the gem names they asked about with
// SelectDoubleCorruptByNames.
//
// Keyed read: check HasDoubleCorrupt first. A nil result from a warm cache means
// this snapshot priced no gems at that input variant, which the database would
// answer identically.
func (c *Cache) DoubleCorruptByVariant(inputVariant string) []DoubleCorruptResult {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.doubleCorruptByVariant[inputVariant]
}

// SetCorruptedGemNamePool stores one corrupted-gem autocomplete pool and marks
// that pool warm.
//
// One pool at a time, because the two come from two queries and either can fail
// alone. A caller that skips this call for a failed query leaves that pool's
// previous names *and* its previous warmth untouched, which is the only way a
// transient error neither wipes a good corpus nor advertises an unfetched one as
// authoritative.
//
// It stores an empty result as readily as a full one: a pool with no corrupted
// gems is an answer, and withholding it leaves the pool COLD and every keystroke
// on a DISTINCT ... ILIKE over gem_snapshots.
func (c *Cache) SetCorruptedGemNamePool(isTransfigured bool, names []string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if isTransfigured {
		c.corruptedTransfiguredGemNames = names
		c.corruptedTransfiguredNamesSet = true
		return
	}
	c.corruptedGemNames = names
	c.corruptedNamesSet = true
}

// CorruptedGemNamesSearch returns corrupted gem names matching all query words
// (case-insensitive). Runs entirely in memory. Returns up to limit results.
//
// ok carries the same cold-cache signal as GemNamesSearch, reported per pool:
// the two corpora are populated from separate queries, so a warm transfigured
// pool says nothing about the skill pool. See GemNamesSearch for why the signal
// cannot ride on the result — and note that it cannot ride on the corpus either,
// which is what it used to do: a league whose snapshots hold no corrupted gem of
// one pool produces an empty pool, and reading that as COLD sent every keystroke
// against that pool to the database for the life of the process.
func (c *Cache) CorruptedGemNamesSearch(query string, isTransfigured bool, limit int) (names []string, ok bool) {
	c.mu.RLock()
	var corpus []string
	var set bool
	if isTransfigured {
		corpus, set = c.corruptedTransfiguredGemNames, c.corruptedTransfiguredNamesSet
	} else {
		corpus, set = c.corruptedGemNames, c.corruptedNamesSet
	}
	c.mu.RUnlock()

	if !set {
		return nil, false
	}
	if query == "" {
		return nil, true
	}
	return searchNames(corpus, query, limit), true
}

// SetGemSignals replaces the cached gem signals and marks them warm.
func (c *Cache) SetGemSignals(signals []GemSignal) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.gemSignalsSet = true
	c.gemSignals = signals
	c.lastUpdated = time.Now()
}

// GemSignals returns every cached gem signal, for the caller to narrow by variant
// and tier with filterGemSignals.
//
// ok reports whether a RunV2 has stored signals, and reports separately from
// GemFeatures for the reason given there. When ok is true the signals are
// authoritative — including when they are empty — and the caller must not fall
// back.
func (c *Cache) GemSignals() ([]GemSignal, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.gemSignals, c.gemSignalsSet
}

// SetSparklines replaces both sparkline maps and the high-water mark, and marks
// the corpus warm. It stores the maps as they stand, empty ones included — see
// the writer half of the cache-state contract at the top of this file.
//
// Callers build the replacement maps outside the lock (see
// mergeSparklineSeries) and hand them over whole — this method assigns and
// nothing more, so the write lock is held for four assignments rather than for
// a corpus-wide merge. highWater is the newest snapshot time folded into the
// maps; pass the max time actually observed, so a repeated pass over an
// unchanged snapshot leaves it where it was.
func (c *Cache) SetSparklines(series, corrupted map[sparklineKey][]SparklinePoint, highWater time.Time) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.sparklinesSet = true
	c.sparklines = series
	c.sparklinesCorrupted = corrupted
	c.sparklineHighWater = highWater
}

// SetSparklinesByName replaces both sparkline maps from name-then-variant keyed
// input. sparklineKey is unexported, so callers outside this package cannot
// build the maps SetSparklines takes; this is their entry point.
//
// Both maps are converted before the lock is taken, keeping the write lock down
// to the four assignments SetSparklines performs.
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

// HasSparklines reports whether the sparkline corpus has been populated — both
// maps, non-corrupted and corrupted. Handlers need this to tell a cold cache —
// where falling back to the database is correct — from a warm cache where a
// missing key genuinely means the gem has no recent points.
//
// One flag covers both maps because one SparklineWindow read fills both and one
// SetSparklines stores them: there is no outcome where the non-corrupted map is
// authoritative and the corrupted one was never computed.
//
// It covers every variant either map is keyed at, for the same reason. The read
// filters on sparklineVariants and sparklineCorruptedVariants — the latter is
// DedicationVariants itself, the same list cachedCorruptedSparklines admits — so
// after a population a variant with no keys has no series rather than no data.
// The per-variant predicate this replaces asked the narrower question and sent
// every Dedication request for such a variant back to gem_snapshots, scanning
// the whole corrupted map under the read lock to do it.
//
// It answers from the stored flag rather than from map length deliberately:
// mergeSparklineMaps drops any key whose merged series came out empty, so a tick
// that retained nothing leaves both maps empty, and reading that as COLD would
// put the query back on every collective, compare and trend request until some
// series survived — the WARM-AND-EMPTY failure in the contract above.
func (c *Cache) HasSparklines() bool {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.sparklinesSet
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
// this gem produced no signal rows inside signalHistorySeedMaxDays, which is
// what the seed asked the database and what it answered; reading nil as cold
// would send one query per gem on every scan.
//
// That equivalence is the seed's responsibility and it is load-bearing. It holds
// only because SignalHistoryWindow caps rows per series: a seed that bounded the
// corpus more narrowly than the key space callers address would make nil mean
// "outside the seed's reach" while the database still held rows, and no reader
// discipline could tell the two apart.
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
