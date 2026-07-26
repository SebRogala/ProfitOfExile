package lab

import (
	"context"
	"errors"
	"reflect"
	"testing"
	"time"

	"profitofexile/internal/league"
)

// sparklineNow is a fixed reference instant. RFC3339 has second resolution, so a
// truncated, explicit instant keeps every derived timestamp exactly reversible.
func sparklineNow() time.Time {
	return time.Date(2026, 7, 20, 12, 0, 0, 0, time.UTC)
}

// sparkPoint builds a point `ago` before now, the way the repository does:
// RFC3339 in UTC.
func sparkPoint(now time.Time, ago time.Duration, price float64) SparklinePoint {
	return SparklinePoint{
		Time:     now.Add(-ago).UTC().Format(time.RFC3339),
		Price:    price,
		Listings: 3,
	}
}

// sparkTimes extracts the Time fields so a failure message names the timestamps
// that were kept rather than dumping whole structs.
func sparkTimes(pts []SparklinePoint) []string {
	out := make([]string, len(pts))
	for i, p := range pts {
		out[i] = p.Time
	}
	return out
}

func hasSparkTime(pts []SparklinePoint, want string) bool {
	for _, p := range pts {
		if p.Time == want {
			return true
		}
	}
	return false
}

// --- mergeSparklineSeries -------------------------------------------------

// A point from the newest snapshot must land at the end of the series: that is
// the point the sparkline's right edge is drawn from.
func TestMergeSparklineSeries_NewestIncomingPointEndsTheSeries(t *testing.T) {
	now := sparklineNow()
	existing := []SparklinePoint{
		sparkPoint(now, 3*time.Hour, 10),
		sparkPoint(now, 2*time.Hour, 11),
		sparkPoint(now, time.Hour, 12),
	}
	incoming := []SparklinePoint{sparkPoint(now, 10*time.Minute, 25)}

	got := mergeSparklineSeries(existing, incoming, now)

	if len(got) != 4 {
		t.Fatalf("merged length: got %d (%v), want 4", len(got), sparkTimes(got))
	}
	last := got[len(got)-1]
	if last.Time != incoming[0].Time || last.Price != 25 {
		t.Fatalf("last point: got %+v, want the incoming point %+v", last, incoming[0])
	}
}

// Once the rolling window holds the tail minimum, older points carry no value
// and must be dropped — otherwise the series grows for the whole league.
func TestMergeSparklineSeries_DropsOutOfWindowPointWhenTailIsSatisfied(t *testing.T) {
	now := sparklineNow()
	stale := sparkPoint(now, 20*time.Hour, 99)
	existing := []SparklinePoint{
		stale,
		sparkPoint(now, 4*time.Hour, 10),
		sparkPoint(now, 3*time.Hour, 11),
		sparkPoint(now, 2*time.Hour, 12),
		sparkPoint(now, time.Hour, 13),
	}

	got := mergeSparklineSeries(existing, nil, now)

	if len(got) != 4 {
		t.Fatalf("merged length: got %d (%v), want the 4 in-window points", len(got), sparkTimes(got))
	}
	if hasSparkTime(got, stale.Time) {
		t.Fatalf("point %s is %v old and must be dropped, got %v", stale.Time, 20*time.Hour, sparkTimes(got))
	}
}

// A gem that stopped trading keeps a short tail so the sparkline still shows its
// last known shape instead of collapsing to two points.
func TestMergeSparklineSeries_ExtendsBackwardsToTailMinimum(t *testing.T) {
	now := sparklineNow()
	existing := []SparklinePoint{
		sparkPoint(now, 40*time.Hour, 7),
		sparkPoint(now, 30*time.Hour, 8),
		sparkPoint(now, 20*time.Hour, 9),
		sparkPoint(now, 2*time.Hour, 10),
		sparkPoint(now, time.Hour, 11),
	}

	got := mergeSparklineSeries(existing, nil, now)

	if len(got) != SparklineTailPoints {
		t.Fatalf("merged length: got %d (%v), want the tail minimum %d",
			len(got), sparkTimes(got), SparklineTailPoints)
	}
	if got[0].Time != existing[1].Time {
		t.Fatalf("oldest kept point: got %s, want %s (30h — the 4th-newest)", got[0].Time, existing[1].Time)
	}
}

// Past the lookback floor a flat line from last week is worse than nothing, so
// the tail extension stops even though it leaves fewer than the tail minimum.
func TestMergeSparklineSeries_DropsPointBeyondMaxLookbackDespiteShortTail(t *testing.T) {
	now := sparklineNow()
	ancient := sparkPoint(now, 200*time.Hour, 99)
	existing := []SparklinePoint{ancient, sparkPoint(now, time.Hour, 11)}

	got := mergeSparklineSeries(existing, nil, now)

	if len(got) != 1 {
		t.Fatalf("merged length: got %d (%v), want only the in-window point", len(got), sparkTimes(got))
	}
	if got[0].Time != existing[1].Time {
		t.Fatalf("kept point: got %s, want %s", got[0].Time, existing[1].Time)
	}
}

// RunV2 runs twice per snapshot (gem tick + T+15min recompute), so folding the
// same batch a second time must be a no-op. An append without dedup doubles the
// series here.
func TestMergeSparklineSeries_SecondMergeOfSameBatchIsANoOp(t *testing.T) {
	now := sparklineNow()
	existing := []SparklinePoint{
		sparkPoint(now, 4*time.Hour, 10),
		sparkPoint(now, 3*time.Hour, 11),
	}
	incoming := []SparklinePoint{
		sparkPoint(now, 2*time.Hour, 12),
		sparkPoint(now, time.Hour, 13),
	}

	once := mergeSparklineSeries(existing, incoming, now)
	twice := mergeSparklineSeries(once, incoming, now)

	if !reflect.DeepEqual(once, twice) {
		t.Fatalf("second merge of the same batch changed the series:\n once:  %v\n twice: %v",
			sparkTimes(once), sparkTimes(twice))
	}
}

// The cache contract is that stored data is immutable once assigned. Returning a
// reslice of an input — which is what pinning the history backing array for the
// tail would look like — makes the stored series follow the caller's mutation.
func TestMergeSparklineSeries_ResultDoesNotAliasExistingInput(t *testing.T) {
	now := sparklineNow()
	existing := []SparklinePoint{
		sparkPoint(now, 5*time.Hour, 10),
		sparkPoint(now, 4*time.Hour, 11),
		sparkPoint(now, 3*time.Hour, 12),
		sparkPoint(now, 2*time.Hour, 13),
		sparkPoint(now, time.Hour, 14),
	}

	got := mergeSparklineSeries(existing, nil, now)
	wantLast := got[len(got)-1].Price

	existing[len(existing)-1].Price = 999
	existing[0].Price = 999

	if got[len(got)-1].Price != wantLast {
		t.Fatalf("mutating the existing input changed the merged series: last price %v, want %v",
			got[len(got)-1].Price, wantLast)
	}
}

// The trim/tail path is where the alias would actually happen: once the window
// drops the head, returning the retained tail as-is is the cheap implementation,
// and it pins the whole history backing array for a gem that never appends
// again. The all-in-window case above never trims, so it cannot see that.
func TestMergeSparklineSeries_TrimmedTailDoesNotAliasExistingInput(t *testing.T) {
	now := sparklineNow()

	// 40 ascending points: 39 spaced 4h apart from 166h down to 14h, then one at
	// 2h — the only point inside the 12h window. The trim keeps that one point
	// and the tail extension reaches back over 14h/18h/22h to satisfy
	// SparklineTailPoints, so the result carries exactly the last 4 points of
	// `existing`: the same content a reslice of the tail would hand back.
	existing := make([]SparklinePoint, 0, 40)
	for ago := 166; ago >= 14; ago -= 4 {
		existing = append(existing, sparkPoint(now, time.Duration(ago)*time.Hour, float64(ago)))
	}
	existing = append(existing, sparkPoint(now, 2*time.Hour, 2))

	got := mergeSparklineSeries(existing, nil, now)

	// The arrangement has to actually reach the trim/tail path; if it stops doing
	// so the mutation below proves nothing.
	if len(got) != SparklineTailPoints {
		t.Fatalf("merged length: got %d (%v), want the tail minimum %d — the trim/tail path did not run",
			len(got), sparkTimes(got), SparklineTailPoints)
	}
	head := existing[:len(existing)-SparklineTailPoints]
	tail := existing[len(existing)-SparklineTailPoints:]
	if got[0].Time != tail[0].Time {
		t.Fatalf("oldest kept point: got %s, want %s (22h — the 4th-newest)", got[0].Time, tail[0].Time)
	}

	before := append([]SparklinePoint(nil), got...)

	// Both halves: the tail catches a reslice of the retained points, the head
	// catches a result that kept any trimmed-away point by reference.
	for i := range head {
		head[i].Price = 999
	}
	for i := range tail {
		tail[i].Price = 999
	}

	if !reflect.DeepEqual(got, before) {
		t.Fatalf("mutating the existing input changed the merged series:\n before: %+v\n after:  %+v", before, got)
	}
}

// Same contract on the other input: the repository reuses its result maps across
// consumers, so the merged series must not track later edits to the batch.
func TestMergeSparklineSeries_ResultDoesNotAliasIncomingInput(t *testing.T) {
	now := sparklineNow()
	incoming := []SparklinePoint{
		sparkPoint(now, 3*time.Hour, 10),
		sparkPoint(now, 2*time.Hour, 11),
		sparkPoint(now, time.Hour, 12),
	}

	got := mergeSparklineSeries(nil, incoming, now)

	incoming[0].Price = 999

	if got[0].Price != 10 {
		t.Fatalf("mutating the incoming input changed the merged series: first price %v, want 10", got[0].Price)
	}
}

// A tick that brings no new points for a gem still has to trim it, otherwise a
// long-lived series never shrinks.
func TestMergeSparklineSeries_EmptyIncomingStillTrimsToWindow(t *testing.T) {
	now := sparklineNow()
	existing := make([]SparklinePoint, 0, 40)
	for ago := 40; ago >= 1; ago-- {
		existing = append(existing, sparkPoint(now, time.Duration(ago)*time.Hour, float64(ago)))
	}

	got := mergeSparklineSeries(existing, nil, now)

	// 11 hourly points fall strictly inside the 12h window (1h .. 11h ago).
	if len(got) != 11 {
		t.Fatalf("merged length: got %d (%v), want the 11 in-window points", len(got), sparkTimes(got))
	}
	oldest, err := time.Parse(time.RFC3339, got[0].Time)
	if err != nil {
		t.Fatalf("oldest kept point has unparsable time %q: %v", got[0].Time, err)
	}
	if !oldest.After(now.Add(-SparklineWindowHours * time.Hour)) {
		t.Fatalf("oldest kept point %s is outside the %dh window", got[0].Time, SparklineWindowHours)
	}
}

// The repository orders by time, but the merge combines two sources; the drawn
// series is only correct if the result is ascending regardless of input order.
func TestMergeSparklineSeries_OutOfOrderPointsProduceAscendingSeries(t *testing.T) {
	now := sparklineNow()
	incoming := []SparklinePoint{
		sparkPoint(now, time.Hour, 13),
		sparkPoint(now, 4*time.Hour, 10),
		sparkPoint(now, 2*time.Hour, 12),
		sparkPoint(now, 3*time.Hour, 11),
	}

	got := mergeSparklineSeries(nil, incoming, now)

	if len(got) != 4 {
		t.Fatalf("merged length: got %d (%v), want 4", len(got), sparkTimes(got))
	}
	for i := 1; i < len(got); i++ {
		if got[i].Time <= got[i-1].Time {
			t.Fatalf("series is not ascending at index %d: %v", i, sparkTimes(got))
		}
	}
}

// Overlapping reads of the same snapshot deliver the same timestamp twice; the
// series must carry it once, keeping the already-stored point.
func TestMergeSparklineSeries_DuplicateTimestampCollapsesToStoredPoint(t *testing.T) {
	now := sparklineNow()
	existing := []SparklinePoint{sparkPoint(now, time.Hour, 10)}
	duplicate := sparkPoint(now, time.Hour, 77)

	got := mergeSparklineSeries(existing, []SparklinePoint{duplicate}, now)

	if len(got) != 1 {
		t.Fatalf("merged length: got %d (%v), want the duplicate collapsed to 1", len(got), sparkTimes(got))
	}
	if got[0].Price != 10 {
		t.Fatalf("duplicate timestamp: got price %v, want the stored 10 (first occurrence wins)", got[0].Price)
	}
}

// --- populateSparklineCache -----------------------------------------------

// fakeSparklineSource records the `since` argument of every call and replays
// canned series, so the incremental-read and high-water logic can be exercised
// without a database.
type fakeSparklineSource struct {
	sinceCalls  []time.Time
	boundsCalls []SparklineBounds
	series      map[sparklineKey][]SparklinePoint
	corrupted   map[sparklineKey][]SparklinePoint
	err         error
}

func (f *fakeSparklineSource) SparklineWindow(_ context.Context, _ league.Scope, since time.Time, bounds SparklineBounds) (map[sparklineKey][]SparklinePoint, map[sparklineKey][]SparklinePoint, error) {
	f.sinceCalls = append(f.sinceCalls, since)
	f.boundsCalls = append(f.boundsCalls, bounds)
	if f.err != nil {
		return nil, nil, f.err
	}
	return cloneSparklineTestMap(f.series), cloneSparklineTestMap(f.corrupted), nil
}

// cloneSparklineTestMap hands each call its own slices, matching a repository
// that builds fresh results per query — so a test never accidentally proves
// idempotency via shared memory.
func cloneSparklineTestMap(in map[sparklineKey][]SparklinePoint) map[sparklineKey][]SparklinePoint {
	if in == nil {
		return map[sparklineKey][]SparklinePoint{}
	}
	out := make(map[sparklineKey][]SparklinePoint, len(in))
	for k, pts := range in {
		cp := make([]SparklinePoint, len(pts))
		copy(cp, pts)
		out[k] = cp
	}
	return out
}

func TestPopulateSparklineCache_ColdCacheStoresEveryReturnedSeries(t *testing.T) {
	now := sparklineNow()
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	spark := sparkPoint(now, time.Hour, 12)
	src := &fakeSparklineSource{
		series: map[sparklineKey][]SparklinePoint{
			{name: "Spark of Nova", variant: "20/20"}: {spark},
			{name: "Spark of Nova", variant: "1"}:     {sparkPoint(now, time.Hour, 3)},
		},
		corrupted: map[sparklineKey][]SparklinePoint{
			{name: "Spark of Nova", variant: "21/23c"}: {sparkPoint(now, time.Hour, 40)},
		},
	}

	if err := populateSparklineCache(context.Background(), src, c, scope, now); err != nil {
		t.Fatalf("populate: %v", err)
	}

	got := c.For(scope).Sparklines("Spark of Nova", "20/20")
	if len(got) != 1 || got[0].Price != 12 || got[0].Time != spark.Time {
		t.Fatalf("20/20 series: got %+v, want the stored point %+v", got, spark)
	}
	if got := c.For(scope).Sparklines("Spark of Nova", "1"); len(got) != 1 || got[0].Price != 3 {
		t.Fatalf("1 series: got %+v, want the 3c point", got)
	}
	if got := c.For(scope).SparklinesCorrupted("Spark of Nova", "21/23c"); len(got) != 1 || got[0].Price != 40 {
		t.Fatalf("corrupted series: got %+v, want the 40c point", got)
	}
}

// RunV2 runs twice per snapshot. A second pass over unchanged source data must
// leave the series byte-identical — appending without dedup doubles them.
func TestPopulateSparklineCache_SecondPassLeavesContentIdentical(t *testing.T) {
	now := sparklineNow()
	scope := league.Historical("LeagueA")
	c := NewCache(scope)
	key := sparklineKey{name: "Spark of Nova", variant: "20/20"}

	src := &fakeSparklineSource{
		series: map[sparklineKey][]SparklinePoint{
			key: {sparkPoint(now, 2*time.Hour, 11), sparkPoint(now, time.Hour, 12)},
		},
	}

	if err := populateSparklineCache(context.Background(), src, c, scope, now); err != nil {
		t.Fatalf("first populate: %v", err)
	}
	first := c.For(scope).Sparklines(key.name, key.variant)

	if err := populateSparklineCache(context.Background(), src, c, scope, now); err != nil {
		t.Fatalf("second populate: %v", err)
	}
	second := c.For(scope).Sparklines(key.name, key.variant)

	if !reflect.DeepEqual(first, second) {
		t.Fatalf("second pass changed the cached series:\n first:  %v\n second: %v",
			sparkTimes(first), sparkTimes(second))
	}
}

// An empty incremental read means nothing new was collected, so the mark must
// stay put rather than jump to the wall clock and skip future rows.
func TestPopulateSparklineCache_EmptySecondReadLeavesHighWaterUnchanged(t *testing.T) {
	now := sparklineNow()
	scope := league.Historical("LeagueA")
	c := NewCache(scope)
	key := sparklineKey{name: "Spark of Nova", variant: "20/20"}

	src := &fakeSparklineSource{
		series: map[sparklineKey][]SparklinePoint{key: {sparkPoint(now, time.Hour, 12)}},
	}
	if err := populateSparklineCache(context.Background(), src, c, scope, now); err != nil {
		t.Fatalf("first populate: %v", err)
	}
	mark := c.For(scope).SparklineHighWater()

	src.series = map[sparklineKey][]SparklinePoint{}
	if err := populateSparklineCache(context.Background(), src, c, scope, now); err != nil {
		t.Fatalf("second populate: %v", err)
	}

	if got := c.For(scope).SparklineHighWater(); !got.Equal(mark) {
		t.Fatalf("high-water after an empty read: got %s, want the unchanged %s", got, mark)
	}
}

// The whole point of the mark is that the second read is incremental: it must
// ask for rows after the newest point already folded in.
func TestPopulateSparklineCache_SecondPassRequestsSinceEqualToStoredMark(t *testing.T) {
	now := sparklineNow()
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	src := &fakeSparklineSource{
		series: map[sparklineKey][]SparklinePoint{
			{name: "Spark of Nova", variant: "20/20"}: {sparkPoint(now, time.Hour, 12)},
		},
	}
	if err := populateSparklineCache(context.Background(), src, c, scope, now); err != nil {
		t.Fatalf("first populate: %v", err)
	}
	mark := c.For(scope).SparklineHighWater()

	if err := populateSparklineCache(context.Background(), src, c, scope, now); err != nil {
		t.Fatalf("second populate: %v", err)
	}

	if len(src.sinceCalls) != 2 {
		t.Fatalf("source calls: got %d, want 2", len(src.sinceCalls))
	}
	if src.sinceCalls[1].IsZero() {
		t.Fatalf("second read asked for a zero `since` — the read is not incremental")
	}
	if !src.sinceCalls[1].Equal(mark) {
		t.Fatalf("second read `since`: got %s, want the stored mark %s", src.sinceCalls[1], mark)
	}
}

// A cache whose series have all aged out still carries the old mark. Reading
// incrementally from it would leave the cache permanently empty, so an empty
// cache must reload the full window regardless of the mark.
func TestPopulateSparklineCache_ColdCacheRequestsZeroSince(t *testing.T) {
	now := sparklineNow()
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	staleMark := now.Add(-200 * time.Hour)
	c.For(scope).SetSparklines(
		map[sparklineKey][]SparklinePoint{},
		map[sparklineKey][]SparklinePoint{},
		staleMark,
	)

	src := &fakeSparklineSource{
		series: map[sparklineKey][]SparklinePoint{
			{name: "Spark of Nova", variant: "20/20"}: {sparkPoint(now, time.Hour, 12)},
		},
	}

	if err := populateSparklineCache(context.Background(), src, c, scope, now); err != nil {
		t.Fatalf("populate: %v", err)
	}

	if len(src.sinceCalls) != 1 {
		t.Fatalf("source calls: got %d, want 1", len(src.sinceCalls))
	}
	if !src.sinceCalls[0].IsZero() {
		t.Fatalf("cold-cache read `since`: got %s, want the zero time (full window)", src.sinceCalls[0])
	}
}

// The cold read happens at process start, alongside the analysis pass loading
// its own history, and every returned point stays live for the whole merge. So
// it must ask only for what the merge keeps — the rolling window plus a
// per-series tail — not the flat lookback, which is fourteen times the rows for
// the same cache contents.
func TestPopulateSparklineCache_RequestsOnlyTheBoundsTheMergeRetains(t *testing.T) {
	now := sparklineNow()
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	src := &fakeSparklineSource{
		series: map[sparklineKey][]SparklinePoint{
			{name: "Spark of Nova", variant: "20/20"}: {sparkPoint(now, time.Hour, 12)},
		},
	}
	if err := populateSparklineCache(context.Background(), src, c, scope, now); err != nil {
		t.Fatalf("populate: %v", err)
	}

	if len(src.boundsCalls) != 1 {
		t.Fatalf("source calls: got %d, want 1", len(src.boundsCalls))
	}
	want := SparklineBounds{
		WindowHours:   SparklineWindowHours,
		TailPoints:    SparklineTailPoints,
		LookbackHours: sparklineMaxLookbackHours,
	}
	if src.boundsCalls[0] != want {
		t.Fatalf("cold read bounds: got %+v, want %+v — the read must match what mergeSparklineSeries keeps",
			src.boundsCalls[0], want)
	}
}

// Snapshots lag the clock. Advancing the mark to `now` would skip every row
// written between the newest observed point and the read.
func TestPopulateSparklineCache_HighWaterAdvancesToNewestPointNotWallClock(t *testing.T) {
	now := sparklineNow()
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	newest := now.Add(-3 * time.Hour)
	src := &fakeSparklineSource{
		series: map[sparklineKey][]SparklinePoint{
			{name: "Spark of Nova", variant: "20/20"}: {
				sparkPoint(now, 5*time.Hour, 10),
				sparkPoint(now, 3*time.Hour, 12),
			},
		},
	}

	if err := populateSparklineCache(context.Background(), src, c, scope, now); err != nil {
		t.Fatalf("populate: %v", err)
	}

	if got := c.For(scope).SparklineHighWater(); !got.Equal(newest) {
		t.Fatalf("high-water: got %s, want the newest observed point %s", got, newest)
	}
}

// A gem that stops appearing in snapshots still has to age out; the merge must
// run for cached keys the read did not return.
func TestPopulateSparklineCache_UntouchedSeriesIsRemergedAndTrimmed(t *testing.T) {
	now := sparklineNow()
	scope := league.Historical("LeagueA")
	c := NewCache(scope)
	stale := sparklineKey{name: "Delisted Gem", variant: "20/20"}

	aged := []SparklinePoint{
		sparkPoint(now, 30*time.Hour, 5),
		sparkPoint(now, 29*time.Hour, 6),
		sparkPoint(now, 28*time.Hour, 7),
		sparkPoint(now, 27*time.Hour, 8),
		sparkPoint(now, 26*time.Hour, 9),
		sparkPoint(now, 25*time.Hour, 10),
	}
	c.For(scope).SetSparklines(
		map[sparklineKey][]SparklinePoint{stale: aged},
		map[sparklineKey][]SparklinePoint{},
		now.Add(-25*time.Hour),
	)

	src := &fakeSparklineSource{
		series: map[sparklineKey][]SparklinePoint{
			{name: "Spark of Nova", variant: "20/20"}: {sparkPoint(now, time.Hour, 12)},
		},
	}
	if err := populateSparklineCache(context.Background(), src, c, scope, now); err != nil {
		t.Fatalf("populate: %v", err)
	}

	got := c.For(scope).Sparklines(stale.name, stale.variant)
	if len(got) != SparklineTailPoints {
		t.Fatalf("untouched series: got %d points (%v), want it trimmed to the tail minimum %d",
			len(got), sparkTimes(got), SparklineTailPoints)
	}
	if hasSparkTime(got, aged[0].Time) {
		t.Fatalf("untouched series kept its oldest point %s: %v", aged[0].Time, sparkTimes(got))
	}
}

// A failed read must leave the warm cache serving what it had, not blank it.
func TestPopulateSparklineCache_SourceErrorLeavesCacheAndMarkUnchanged(t *testing.T) {
	now := sparklineNow()
	scope := league.Historical("LeagueA")
	c := NewCache(scope)
	key := sparklineKey{name: "Spark of Nova", variant: "20/20"}

	seeded := []SparklinePoint{sparkPoint(now, time.Hour, 12)}
	mark := now.Add(-time.Hour)
	c.For(scope).SetSparklines(
		map[sparklineKey][]SparklinePoint{key: seeded},
		map[sparklineKey][]SparklinePoint{},
		mark,
	)

	src := &fakeSparklineSource{err: errors.New("query gem_snapshots: connection refused")}
	err := populateSparklineCache(context.Background(), src, c, scope, now)

	if err == nil {
		t.Fatalf("populate: got nil error, want the source error surfaced")
	}
	got := c.For(scope).Sparklines(key.name, key.variant)
	if !reflect.DeepEqual(got, seeded) {
		t.Fatalf("cached series after a failed read: got %v, want the untouched %v",
			sparkTimes(got), sparkTimes(seeded))
	}
	if hw := c.For(scope).SparklineHighWater(); !hw.Equal(mark) {
		t.Fatalf("high-water after a failed read: got %s, want the untouched %s", hw, mark)
	}
}
