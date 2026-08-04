package lab

import (
	"context"
	"errors"
	"testing"
	"time"

	"profitofexile/internal/league"
)

var signalHistoryScope = league.Historical("Mirage")

func signalNow() time.Time {
	return time.Date(2026, 7, 20, 12, 0, 0, 0, time.UTC)
}

// change builds a transition `ago` before now, carrying the signal name so an
// assertion can say which point survived.
func change(now time.Time, ago time.Duration, signal string) SignalChange {
	return SignalChange{Time: now.Add(-ago), Signal: signal, Price: 100}
}

func signalNames(changes []SignalChange) []string {
	out := make([]string, len(changes))
	for i, c := range changes {
		out[i] = c.Signal
	}
	return out
}

// fakeSignalSource records whether the seed read was issued, which is the
// observable that distinguishes "seeded once" from "seeded every tick".
type fakeSignalSource struct {
	rings map[signalKey][]SignalChange
	err   error
	calls int
	depth int
}

func (f *fakeSignalSource) SignalHistoryWindow(_ context.Context, _ league.Scope, depth int) (map[signalKey][]SignalChange, error) {
	f.calls++
	f.depth = depth
	if f.err != nil {
		return nil, f.err
	}
	return f.rings, nil
}

// --- mergeSignalHistory ----------------------------------------------------

// The endpoint serves newest first, so the newest transition must head the ring
// regardless of the order it arrived in.
func TestMergeSignalHistory_NewestTransitionHeadsTheRing(t *testing.T) {
	now := signalNow()
	got := mergeSignalHistory(
		[]SignalChange{change(now, 2*time.Hour, "older")},
		[]SignalChange{change(now, 0, "newest")},
		SignalHistoryDepth,
	)

	if len(got) != 2 {
		t.Fatalf("ring has %d entries, want 2: %v", len(got), signalNames(got))
	}
	if got[0].Signal != "newest" || got[1].Signal != "older" {
		t.Errorf("ring = %v, want [newest older]", signalNames(got))
	}
}

// The ring is what bounds memory: 7,300 series that grew without a cap would
// accumulate the whole league's transitions.
func TestMergeSignalHistory_DropsTheOldestPastDepth(t *testing.T) {
	now := signalNow()
	existing := []SignalChange{
		change(now, 1*time.Hour, "keep"),
		change(now, 2*time.Hour, "drop"),
	}

	got := mergeSignalHistory(existing, []SignalChange{change(now, 0, "newest")}, 2)

	if len(got) != 2 {
		t.Fatalf("ring has %d entries, want 2: %v", len(got), signalNames(got))
	}
	if got[0].Signal != "newest" || got[1].Signal != "keep" {
		t.Errorf("ring = %v, want [newest keep] — the oldest entry must fall off", signalNames(got))
	}
}

// RunV2 runs twice per snapshot. A second pass over the same timestamp must not
// occupy a second ring slot, or two recomputes would halve the history depth.
func TestMergeSignalHistory_SamePassTwiceLeavesTheRingUnchanged(t *testing.T) {
	now := signalNow()
	incoming := []SignalChange{change(now, 0, "stored")}
	first := mergeSignalHistory([]SignalChange{change(now, time.Hour, "older")}, incoming, SignalHistoryDepth)

	second := mergeSignalHistory(first, incoming, SignalHistoryDepth)

	if len(second) != len(first) {
		t.Fatalf("second pass changed the ring: %v -> %v", signalNames(first), signalNames(second))
	}
}

// The stored rows win on a repeated timestamp: SaveGemSignals inserts ON
// CONFLICT DO NOTHING, so a recompute leaves the database holding the first
// answer and the cache must not disagree with it.
func TestMergeSignalHistory_RecomputedPointDoesNotReplaceTheStoredOne(t *testing.T) {
	now := signalNow()
	stored := SignalChange{Time: now, Signal: "STABLE", Price: 100}
	recomputed := SignalChange{Time: now, Signal: "DEMAND", Price: 250}

	got := mergeSignalHistory([]SignalChange{stored}, []SignalChange{recomputed}, SignalHistoryDepth)

	if len(got) != 1 {
		t.Fatalf("ring has %d entries, want 1: %v", len(got), signalNames(got))
	}
	if got[0].Signal != "STABLE" || got[0].Price != 100 {
		t.Errorf("entry = %+v, want the stored STABLE/100 point", got[0])
	}
}

// The cache's immutability contract: a reader holding the previous ring must
// keep seeing it unchanged, so the merge may not write through to its input.
func TestMergeSignalHistory_ResultDoesNotAliasExistingInput(t *testing.T) {
	now := signalNow()
	existing := []SignalChange{change(now, time.Hour, "existing")}

	got := mergeSignalHistory(existing, []SignalChange{change(now, 0, "newest")}, SignalHistoryDepth)
	got[len(got)-1].Signal = "mutated"

	if existing[0].Signal != "existing" {
		t.Errorf("existing input entry = %q, want %q — the merge wrote through to a stored ring",
			existing[0].Signal, "existing")
	}
}

func TestMergeSignalHistory_NoInputYieldsNothing(t *testing.T) {
	if got := mergeSignalHistory(nil, nil, SignalHistoryDepth); got != nil {
		t.Errorf("merge of nothing = %v, want nil", got)
	}
}

// --- signalChangesFromTick -------------------------------------------------

// The endpoint's price and velocity columns come from the feature computed for
// the same gem in the same pass; crossing them with another gem's would report
// one gem's price under another's name.
func TestSignalChangesFromTick_JoinsTheFeatureOfTheSameNameAndVariant(t *testing.T) {
	now := signalNow()
	signals := []GemSignal{
		{Time: now, Name: "Spark", Variant: "20/20", Signal: "DEMAND", WindowSignal: "UP", AdvancedSignal: "CASCADE"},
	}
	features := []GemFeature{
		{Time: now, Name: "Spark", Variant: "1/0", Chaos: 5, Listings: 2, VelLongPrice: -1, VelLongListing: -2},
		{Time: now, Name: "Spark", Variant: "20/20", Chaos: 90, Listings: 12, VelLongPrice: 3.5, VelLongListing: 1.5},
	}

	got := signalChangesFromTick(signals, features)

	ring := got[signalKey{name: "Spark", variant: "20/20"}]
	if len(ring) != 1 {
		t.Fatalf("got %d entries for Spark 20/20, want 1", len(ring))
	}
	c := ring[0]
	if c.Price != 90 || c.Listings != 12 || c.PriceVel != 3.5 || c.ListVel != 1.5 {
		t.Errorf("entry = %+v, want the 20/20 feature (price 90, listings 12, vel 3.5/1.5)", c)
	}
	if c.Signal != "DEMAND" || c.Window != "UP" || c.Advanced != "CASCADE" {
		t.Errorf("entry = %+v, want signal DEMAND / window UP / advanced CASCADE", c)
	}
}

// The repository's LEFT JOIN still returns the transition when the feature row
// is missing, so the in-memory path must too — a signal with no feature is a
// transition worth showing, not a row to drop.
func TestSignalChangesFromTick_KeepsATransitionWithNoMatchingFeature(t *testing.T) {
	now := signalNow()
	got := signalChangesFromTick(
		[]GemSignal{{Time: now, Name: "Spark", Variant: "20/20", Signal: "STABLE"}},
		nil,
	)

	ring := got[signalKey{name: "Spark", variant: "20/20"}]
	if len(ring) != 1 {
		t.Fatalf("got %d entries, want the featureless transition kept", len(ring))
	}
	if ring[0].Signal != "STABLE" || ring[0].Price != 0 {
		t.Errorf("entry = %+v, want signal STABLE with a zeroed price", ring[0])
	}
}

// --- populateSignalHistory -------------------------------------------------

func tickSignals(now time.Time, signal string) ([]GemSignal, []GemFeature) {
	return []GemSignal{{Time: now, Name: "Spark", Variant: "20/20", Signal: signal}},
		[]GemFeature{{Time: now, Name: "Spark", Variant: "20/20", Chaos: 40, Listings: 7}}
}

// The cold window is why the seed exists: a ring that only ever appends takes
// ~10 hours to fill, so the first pass reads the history it missed.
func TestPopulateSignalHistory_ColdCacheSeedsFromTheSourceAndAddsTheTick(t *testing.T) {
	now := signalNow()
	cache := NewCache(signalHistoryScope)
	src := &fakeSignalSource{rings: map[signalKey][]SignalChange{
		{name: "Spark", variant: "20/20"}: {change(now, time.Hour, "seeded")},
	}}
	signals, features := tickSignals(now, "fromTick")

	if err := populateSignalHistory(context.Background(), src, cache, signalHistoryScope, signals, features); err != nil {
		t.Fatalf("populate: %v", err)
	}

	ring := cache.For(signalHistoryScope).SignalHistory("Spark", "20/20")
	if len(ring) != 2 {
		t.Fatalf("ring = %v, want the tick's point and the seeded one", signalNames(ring))
	}
	if ring[0].Signal != "fromTick" || ring[1].Signal != "seeded" {
		t.Errorf("ring = %v, want [fromTick seeded]", signalNames(ring))
	}
}

// The seed is paid for once per process. A second tick that re-read it would
// turn the one-off cost into a per-tick one.
func TestPopulateSignalHistory_WarmCacheDoesNotReadTheSource(t *testing.T) {
	now := signalNow()
	cache := NewCache(signalHistoryScope)
	src := &fakeSignalSource{}
	signals, features := tickSignals(now, "first")
	if err := populateSignalHistory(context.Background(), src, cache, signalHistoryScope, signals, features); err != nil {
		t.Fatalf("first populate: %v", err)
	}

	laterSignals, laterFeatures := tickSignals(now.Add(30*time.Minute), "second")
	if err := populateSignalHistory(context.Background(), src, cache, signalHistoryScope, laterSignals, laterFeatures); err != nil {
		t.Fatalf("second populate: %v", err)
	}

	if src.calls != 1 {
		t.Errorf("source read %d times, want exactly 1 — the seed must not repeat per tick", src.calls)
	}
	if ring := cache.For(signalHistoryScope).SignalHistory("Spark", "20/20"); len(ring) != 2 {
		t.Errorf("ring = %v, want both ticks' points", signalNames(ring))
	}
}

// A failed seed must leave the cache COLD. Storing the tick's single point would
// mark the corpus warm, and the endpoint would then answer every gem with at
// most one transition instead of falling back to the query that has them all.
func TestPopulateSignalHistory_SeedFailureLeavesTheCacheCold(t *testing.T) {
	now := signalNow()
	cache := NewCache(signalHistoryScope)
	src := &fakeSignalSource{err: errors.New("boom")}
	signals, features := tickSignals(now, "fromTick")

	if err := populateSignalHistory(context.Background(), src, cache, signalHistoryScope, signals, features); err == nil {
		t.Fatal("populate returned no error on a failed seed")
	}
	if cache.For(signalHistoryScope).HasSignalHistory() {
		t.Error("cache reports warm after a failed seed; handlers would stop falling back")
	}
}

// The seed must ask for exactly what a ring holds: asking for less would leave
// the ring permanently short of the depth the endpoint may serve.
func TestPopulateSignalHistory_SeedsToTheRingDepth(t *testing.T) {
	cache := NewCache(signalHistoryScope)
	src := &fakeSignalSource{}
	signals, features := tickSignals(signalNow(), "fromTick")

	if err := populateSignalHistory(context.Background(), src, cache, signalHistoryScope, signals, features); err != nil {
		t.Fatalf("populate: %v", err)
	}

	if src.depth != SignalHistoryDepth {
		t.Errorf("seed asked for depth %d, want %d", src.depth, SignalHistoryDepth)
	}
}

// A tick that produces nothing for a league still leaves the cache warm and
// empty — the writer half of the cache-state contract.
func TestPopulateSignalHistory_EmptyTickStillMarksTheCorpusWarm(t *testing.T) {
	cache := NewCache(signalHistoryScope)

	if err := populateSignalHistory(context.Background(), &fakeSignalSource{}, cache, signalHistoryScope, nil, nil); err != nil {
		t.Fatalf("populate: %v", err)
	}

	if !cache.For(signalHistoryScope).HasSignalHistory() {
		t.Error("cache reports cold after an empty tick; every request would query for an answer the tick already has")
	}
}
