package lab

import (
	"container/list"
	"context"
	"fmt"
	"log/slog"
	"slices"
	"strings"
	"sync"
	"unsafe"
)

// RankingInput names the tables a ranking answer reads; a write to one
// invalidates every answer that reads it.
type RankingInput uint8

const (
	RankingInputTransfigure  RankingInput = 1 << iota // transfigure_results
	RankingInputGemSignals                            // gem_signals
	RankingInputGemFeatures                           // gem_features
	RankingInputGemSnapshots                          // gem_snapshots
	RankingInputDedication                            // dedication_snapshots
)

const rankingInputCount = 5

// The inputs each answer reads: the Normal CTE, the Dedication CTE and the
// GCP base lookup.
const (
	normalRankingInputs     = RankingInputTransfigure | RankingInputGemSignals | RankingInputGemFeatures
	dedicationRankingInputs = RankingInputGemSnapshots | RankingInputDedication
	basePriceRankingInputs  = RankingInputTransfigure
)

// Bounds on the retained answers; the first one reached evicts the least
// recently used answer.
const (
	rankingAnswerMaxEntries = 256
	rankingAnswerMaxBytes   = 16 << 20
)

const (
	normalKeyPrefix     = "n|"
	dedicationKeyPrefix = "d|"
	basePriceKeyPrefix  = "b|"
)

// rankingGeneration is the revision of every input an answer reads; inputs it
// does not read stay zero, so a write to them leaves it unchanged.
type rankingGeneration [rankingInputCount]uint64

// RankingAnswers retains the undecorated ranked collective answers (Normal
// pages, Dedication pages, GCP base-price maps) of one league until an input
// they read changes. There is no TTL: an answer is current until a publication
// for one of its inputs.
//
// Equal requests share one in-flight load per answer generation. The load
// runs detached from any one caller, so a waiter that gives up neither cancels
// it nor the others' answer. Only successful loads are retained.
//
// The publication barrier is separate from Cache's data mutex: fills hold it
// shared across their SQL, writers hold it exclusively across their commit
// and the revision bump, so a fill never retains rows of one revision under
// another. No SQL runs under mu.
type RankingAnswers struct {
	barrier sync.RWMutex

	mu         sync.Mutex
	revisions  rankingGeneration
	entries    map[string]*list.Element // of *rankingEntry
	lru        *list.List
	bytes      int64
	flights    map[rankingFlightKey]*rankingFlight
	maxEntries int
	maxBytes   int64
}

type rankingEntry struct {
	key        string
	generation rankingGeneration
	value      any
	size       int64
}

type rankingFlightKey struct {
	key        string
	generation rankingGeneration
}

type rankingFlight struct {
	done    chan struct{}
	waiters int
	value   any
	err     error
}

func newRankingAnswers(maxEntries int, maxBytes int64) *RankingAnswers {
	return &RankingAnswers{
		entries:    make(map[string]*list.Element),
		lru:        list.New(),
		flights:    make(map[rankingFlightKey]*rankingFlight),
		maxEntries: maxEntries,
		maxBytes:   maxBytes,
	}
}

// NormalPage returns the Normal ranking page for q, loading it through load
// on a miss. The rows are the caller's copy to decorate.
func (rc *RankingAnswers) NormalPage(ctx context.Context, q CollectiveQuery, load func(context.Context) (CollectivePage, error)) (CollectivePage, error) {
	return rc.page(ctx, normalPageKey(q), normalRankingInputs, load)
}

// DedicationPage returns the Dedication ranking page for q, loading it
// through load on a miss. The rows are the caller's copy to decorate.
func (rc *RankingAnswers) DedicationPage(ctx context.Context, q DedicationCollectiveQuery, load func(context.Context) (CollectivePage, error)) (CollectivePage, error) {
	return rc.page(ctx, dedicationPageKey(q), dedicationRankingInputs, load)
}

func (rc *RankingAnswers) page(ctx context.Context, key string, inputs RankingInput, load func(context.Context) (CollectivePage, error)) (CollectivePage, error) {
	v, err := rc.get(ctx, key, inputs, func(ctx context.Context) (any, int64, error) {
		p, err := load(ctx)
		return p, rankingPageBytes(p), err
	})
	if err != nil {
		return CollectivePage{}, err
	}
	p := v.(CollectivePage)
	p.Rows = slices.Clone(p.Rows)
	return p, nil
}

// BasePrices returns the GCP base-price map for the set of bases, loading it
// through load on a miss. Equal sets in any order share one answer. The map is
// shared and must not be modified.
func (rc *RankingAnswers) BasePrices(ctx context.Context, bases []string, load func(context.Context, []string) (map[string]float64, error)) (map[string]float64, error) {
	set := slices.Clone(bases)
	slices.Sort(set)
	set = slices.Compact(set)
	v, err := rc.get(ctx, basePriceKeyPrefix+strings.Join(set, "\x00"), basePriceRankingInputs, func(ctx context.Context) (any, int64, error) {
		prices, err := load(ctx, bases)
		size := int64(len(prices)) * 48
		for name := range prices {
			size += int64(len(name))
		}
		return prices, size, err
	})
	if err != nil {
		return nil, err
	}
	return v.(map[string]float64), nil
}

// Commit runs write (a commit to the named inputs) under the publication
// barrier, then advances their revision. The revision advances even when
// write fails: a failed write may have committed part of its rows.
func (rc *RankingAnswers) Commit(inputs RankingInput, write func() error) error {
	rc.barrier.Lock()
	defer rc.barrier.Unlock()
	defer rc.advance(inputs)
	return write()
}

// Publish advances the revision of inputs another process already committed,
// such as a gem snapshot the collector stored before its event.
func (rc *RankingAnswers) Publish(inputs RankingInput) {
	rc.barrier.Lock()
	defer rc.barrier.Unlock()
	rc.advance(inputs)
}

func (rc *RankingAnswers) advance(inputs RankingInput) {
	rc.mu.Lock()
	defer rc.mu.Unlock()
	for i := 0; i < rankingInputCount; i++ {
		if inputs&(1<<i) != 0 {
			rc.revisions[i]++
		}
	}
	for el := rc.lru.Front(); el != nil; {
		next := el.Next()
		if e := el.Value.(*rankingEntry); e.generation != rc.generationLocked(rankingInputsOf(e.key)) {
			rc.removeLocked(el)
		}
		el = next
	}
}

func (rc *RankingAnswers) generationLocked(inputs RankingInput) rankingGeneration {
	var g rankingGeneration
	for i := 0; i < rankingInputCount; i++ {
		if inputs&(1<<i) != 0 {
			g[i] = rc.revisions[i]
		}
	}
	return g
}

func rankingInputsOf(key string) RankingInput {
	switch {
	case strings.HasPrefix(key, normalKeyPrefix):
		return normalRankingInputs
	case strings.HasPrefix(key, dedicationKeyPrefix):
		return dedicationRankingInputs
	default:
		return basePriceRankingInputs
	}
}

func (rc *RankingAnswers) get(ctx context.Context, key string, inputs RankingInput, load func(context.Context) (any, int64, error)) (any, error) {
	// A caller already gone loads on its own context, unshared and unretained,
	// as it would without the cache.
	if ctx.Err() != nil {
		v, _, err := load(ctx)
		return v, err
	}
	rc.mu.Lock()
	generation := rc.generationLocked(inputs)
	if el, ok := rc.entries[key]; ok {
		e := el.Value.(*rankingEntry)
		if e.generation == generation {
			rc.lru.MoveToFront(el)
			rc.mu.Unlock()
			return e.value, nil
		}
		rc.removeLocked(el)
	}
	fk := rankingFlightKey{key: key, generation: generation}
	f, ok := rc.flights[fk]
	if !ok {
		f = &rankingFlight{done: make(chan struct{})}
		rc.flights[fk] = f
		go rc.fill(context.WithoutCancel(ctx), fk, inputs, f, load)
	}
	f.waiters++
	rc.mu.Unlock()

	select {
	case <-f.done:
		return f.value, f.err
	case <-ctx.Done():
		rc.mu.Lock()
		f.waiters--
		rc.mu.Unlock()
		return nil, ctx.Err()
	}
}

func (rc *RankingAnswers) fill(ctx context.Context, fk rankingFlightKey, inputs RankingInput, f *rankingFlight, load func(context.Context) (any, int64, error)) {
	var value any
	var size int64
	var err error
	rc.barrier.RLock()
	defer func() {
		if recovered := recover(); recovered != nil {
			slog.Error("lab: ranking answer load panicked", "key", fk.key, "recover", recovered)
			value, err = nil, fmt.Errorf("lab: ranking answer load panicked: %v", recovered)
		}
		rc.mu.Lock()
		// Retained only when no publication for its inputs landed since the
		// flight began; a newer generation's requests start their own flight.
		if err == nil && size <= rc.maxBytes && rc.generationLocked(inputs) == fk.generation {
			rc.storeLocked(fk, value, size)
		}
		delete(rc.flights, fk)
		f.value, f.err = value, err
		close(f.done)
		rc.mu.Unlock()
		rc.barrier.RUnlock()
	}()
	value, size, err = load(ctx)
}

func (rc *RankingAnswers) storeLocked(fk rankingFlightKey, value any, size int64) {
	if el, ok := rc.entries[fk.key]; ok {
		rc.removeLocked(el)
	}
	rc.entries[fk.key] = rc.lru.PushFront(&rankingEntry{key: fk.key, generation: fk.generation, value: value, size: size})
	rc.bytes += size
	for rc.lru.Len() > rc.maxEntries || rc.bytes > rc.maxBytes {
		rc.removeLocked(rc.lru.Back())
	}
}

func (rc *RankingAnswers) removeLocked(el *list.Element) {
	e := rc.lru.Remove(el).(*rankingEntry)
	delete(rc.entries, e.key)
	rc.bytes -= e.size
}

// waiting reports how many callers wait on in-flight loads whose key starts
// with prefix.
func (rc *RankingAnswers) waiting(prefix string) int {
	rc.mu.Lock()
	defer rc.mu.Unlock()
	n := 0
	for fk, f := range rc.flights {
		if strings.HasPrefix(fk.key, prefix) {
			n += f.waiters
		}
	}
	return n
}

// normalPageKey is the canonical key of a Normal page: every field the SQL
// reads. A search ignores page and page size (it answers its whole match set);
// SparkVariant feeds decorations only.
func normalPageKey(q CollectiveQuery) string {
	page, size := q.Page, q.PageSize
	if q.Search != "" {
		page, size = 0, 0
	}
	return fmt.Sprintf("%s%q|%q|%v|%t|%q|%q|%d|%d|%t", normalKeyPrefix,
		q.Variant, q.Color, q.Budget, q.HideLowConfidence, q.Search, q.Sort, page, size, q.Legacy)
}

// dedicationPageKey is the canonical key of a Dedication page. A paged
// (non-legacy) search ignores page and page size.
func dedicationPageKey(q DedicationCollectiveQuery) string {
	page, size := q.Page, q.PageSize
	if q.Search != "" && !q.Legacy {
		page, size = 0, 0
	}
	return fmt.Sprintf("%s%q|%q|%q|%v|%t|%q|%q|%d|%d|%t", dedicationKeyPrefix,
		q.Variant, q.Pool, q.Color, q.Budget, q.HideLowConfidence, q.Search, q.Sort, page, size, q.Legacy)
}

// rankingPageBytes estimates a page's retained size: the row structs plus
// their string bytes.
func rankingPageBytes(p CollectivePage) int64 {
	size := int64(unsafe.Sizeof(p)) + int64(len(p.Rows))*int64(unsafe.Sizeof(CollectiveResult{}))
	for _, r := range p.Rows {
		for _, s := range []string{r.TransfiguredName, r.BaseName, r.Variant, r.GemColor, r.Confidence, r.Signal,
			r.WindowSignal, r.AdvancedSignal, r.LiquidityTier, r.PriceTier, r.TierAction, r.SellUrgency, r.SellReason,
			r.SellabilityLabel, r.SellConfidence, r.TradeConfidenceNote} {
			size += int64(len(s))
		}
	}
	return size
}
