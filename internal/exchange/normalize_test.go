package exchange

import (
	"context"
	"encoding/json"
	"log/slog"
	"sync"
	"testing"
)

// Item ids and market ids taken verbatim from
// testdata/hour_allflame_sample.json.
const (
	chaosID  = "Metadata/Items/Currency/CurrencyRerollRare"
	divineID = "Metadata/Items/Currency/CurrencyModValues"
	cardID   = "Metadata/Items/DivinationCards/DivinationCardThunderousSkies"
	scarabID = "Metadata/Items/Scarabs/ScarabDomination3"
	omenID   = "Metadata/Items/Currency/AncestralOmenOnJewellersMakeFullSockets"
	hellID   = "Metadata/Items/Currency/CurrencyHellscapeUpgradeToUnique"

	// chaosDivineMarket trades chaos (A) against divine (B).
	chaosDivineMarket = chaosID + "|" + divineID
	// scarabMarket carries the fixture's only non-unit lowest_ratio (2:3).
	scarabMarket = scarabID + "|" + chaosID
	// zeroRatioMarket carries 0:0 in both ratio maps but non-zero stock.
	zeroRatioMarket = hellID + "|" + divineID
)

// loadFixtureHour decodes the recorded Allflame hour.
func loadFixtureHour(t *testing.T) *HourPayload {
	t.Helper()
	var payload HourPayload
	if err := json.Unmarshal(readTestdata(t, "hour_allflame_sample.json"), &payload); err != nil {
		t.Fatalf("decode fixture: %v", err)
	}
	return &payload
}

// rowByMarketID returns the single normalized row for marketID.
func rowByMarketID(t *testing.T, rows []Row, marketID string) Row {
	t.Helper()
	for _, row := range rows {
		if row.MarketID == marketID {
			return row
		}
	}
	t.Fatalf("no row for market %q (got %d rows)", marketID, len(rows))
	return Row{}
}

// marketSpec describes a synthetic market. Every [2]int64 is
// {valueForItemA, valueForItemB} in market_pair order.
type marketSpec struct {
	league       string
	itemA        string
	itemB        string
	volume       [2]int64
	lowestStock  [2]int64
	highestStock [2]int64
	lowestRatio  [2]int64
	highestRatio [2]int64
}

// market renders the spec in the feed's shape. All five quantity maps carry both
// ids, so the market is well-formed unless a test deliberately breaks it.
func (s marketSpec) market() Market {
	return Market{
		League:       s.league,
		MarketID:     s.itemA + "|" + s.itemB,
		MarketPair:   []string{s.itemA, s.itemB},
		VolumeTraded: s.pair(s.volume),
		LowestStock:  s.pair(s.lowestStock),
		HighestStock: s.pair(s.highestStock),
		LowestRatio:  s.pair(s.lowestRatio),
		HighestRatio: s.pair(s.highestRatio),
	}
}

func (s marketSpec) pair(v [2]int64) map[string]int64 {
	return map[string]int64{s.itemA: v[0], s.itemB: v[1]}
}

// validSpec is a well-formed chaos/divine market at 196:1 both sides.
func validSpec() marketSpec {
	return marketSpec{
		league:       "Allflame",
		itemA:        chaosID,
		itemB:        divineID,
		volume:       [2]int64{1000, 5},
		lowestStock:  [2]int64{40, 2},
		highestStock: [2]int64{50, 3},
		lowestRatio:  [2]int64{196, 1},
		highestRatio: [2]int64{196, 1},
	}
}

func TestNormalize_chaosDivineRow_pricesOneDivineInChaos(t *testing.T) {
	rows, _ := Normalize(loadFixtureHour(t))

	row := rowByMarketID(t, rows, chaosDivineMarket)
	if !row.PriceValid {
		t.Fatalf("PriceValid = false (InvalidReason %q), want true", row.InvalidReason)
	}
	if row.InvalidReason != "" {
		t.Errorf("InvalidReason = %q, want empty on a valid row", row.InvalidReason)
	}
	// ItemA/ItemB follow market_pair order: chaos is A, divine is B.
	if row.ItemA != chaosID {
		t.Errorf("ItemA = %q, want %q", row.ItemA, chaosID)
	}
	if row.ItemB != divineID {
		t.Errorf("ItemB = %q, want %q", row.ItemB, divineID)
	}
	// Hundreds of chaos buy one divine: 196 lowest, 201 highest.
	if row.LowestPriceBInA != 196 {
		t.Errorf("LowestPriceBInA = %v, want 196", row.LowestPriceBInA)
	}
	if row.HighestPriceBInA != 201 {
		t.Errorf("HighestPriceBInA = %v, want 201", row.HighestPriceBInA)
	}
}

func TestNormalize_copiesEveryQuantityPerItemSide(t *testing.T) {
	rows, _ := Normalize(loadFixtureHour(t))

	row := rowByMarketID(t, rows, chaosDivineMarket)
	checks := []struct {
		field string
		got   int64
		want  int64
	}{
		{"VolumeA", row.VolumeA, 13001051},
		{"VolumeB", row.VolumeB, 65361},
		{"LowestStockA", row.LowestStockA, 4169809},
		{"LowestStockB", row.LowestStockB, 5444},
		{"HighestStockA", row.HighestStockA, 4564191},
		{"HighestStockB", row.HighestStockB, 8878},
		{"LowestRatioA", row.LowestRatioA, 196},
		{"LowestRatioB", row.LowestRatioB, 1},
		{"HighestRatioA", row.HighestRatioA, 201},
		{"HighestRatioB", row.HighestRatioB, 1},
	}
	for _, c := range checks {
		if c.got != c.want {
			t.Errorf("%s = %d, want %d", c.field, c.got, c.want)
		}
	}
	if row.League != "Allflame" {
		t.Errorf("League = %q, want %q", row.League, "Allflame")
	}
}

func TestRatio_positiveQuantities_dividesQuoteByItem(t *testing.T) {
	tests := []struct {
		name     string
		quoteQty int64
		itemQty  int64
		want     float64
	}{
		{name: "196 chaos buy one divine", quoteQty: 196, itemQty: 1, want: 196},
		{name: "reverse direction of the same pair", quoteQty: 1, itemQty: 196, want: 1.0 / 196.0},
		{name: "non-unit quantities divide", quoteQty: 2, itemQty: 3, want: 2.0 / 3.0},
		{name: "reverse direction of a non-unit pair", quoteQty: 3, itemQty: 2, want: 1.5},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := Ratio(tt.quoteQty, tt.itemQty)
			if !ok {
				t.Fatalf("ok = false, want true for %d/%d", tt.quoteQty, tt.itemQty)
			}
			if got != tt.want {
				t.Errorf("Ratio(%d, %d) = %v, want %v", tt.quoteQty, tt.itemQty, got, tt.want)
			}
		})
	}
}

func TestRatio_nonPositiveQuantity_returnsZeroAndFalse(t *testing.T) {
	tests := []struct {
		name     string
		quoteQty int64
		itemQty  int64
	}{
		{name: "both quantities zero", quoteQty: 0, itemQty: 0},
		{name: "item quantity zero", quoteQty: 196, itemQty: 0},
		{name: "quote quantity zero", quoteQty: 0, itemQty: 196},
		{name: "item quantity negative", quoteQty: 196, itemQty: -1},
		{name: "quote quantity negative", quoteQty: -196, itemQty: 1},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := Ratio(tt.quoteQty, tt.itemQty)
			if ok {
				t.Errorf("ok = true, want false for %d/%d", tt.quoteQty, tt.itemQty)
			}
			if got != 0 {
				t.Errorf("price = %v, want 0", got)
			}
		})
	}
}

func TestPriceOf_derivesPriceFromQuantities(t *testing.T) {
	tests := []struct {
		name  string
		ratio map[string]int64
		item  string
		quote string
		want  float64
	}{
		{
			name:  "unit side is the smaller quantity, not a pair position",
			ratio: map[string]int64{"a": 196, "b": 1},
			item:  "b",
			quote: "a",
			want:  196,
		},
		{
			name:  "reverse direction from the same quantities",
			ratio: map[string]int64{"a": 196, "b": 1},
			item:  "a",
			quote: "b",
			want:  1.0 / 196.0,
		},
		{
			name:  "non-unit quantities divide",
			ratio: map[string]int64{"a": 2, "b": 3},
			item:  "b",
			quote: "a",
			want:  2.0 / 3.0,
		},
		{
			name:  "non-unit quantities divide in reverse",
			ratio: map[string]int64{"a": 2, "b": 3},
			item:  "a",
			quote: "b",
			want:  3.0 / 2.0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := PriceOf(tt.ratio, tt.item, tt.quote)
			if !ok {
				t.Fatalf("ok = false, want true for %v", tt.ratio)
			}
			if got != tt.want {
				t.Errorf("PriceOf(%v, %q, %q) = %v, want %v", tt.ratio, tt.item, tt.quote, got, tt.want)
			}
		})
	}
}

func TestPriceOf_unusableQuantity_returnsZeroAndFalse(t *testing.T) {
	tests := []struct {
		name  string
		ratio map[string]int64
		item  string
		quote string
	}{
		{name: "both quantities zero", ratio: map[string]int64{"a": 0, "b": 0}, item: "b", quote: "a"},
		{name: "item quantity zero", ratio: map[string]int64{"a": 196, "b": 0}, item: "b", quote: "a"},
		{name: "quote quantity zero", ratio: map[string]int64{"a": 0, "b": 1}, item: "b", quote: "a"},
		{name: "negative quantity", ratio: map[string]int64{"a": -196, "b": 1}, item: "b", quote: "a"},
		{name: "item missing from the map", ratio: map[string]int64{"a": 196}, item: "b", quote: "a"},
		{name: "quote missing from the map", ratio: map[string]int64{"b": 1}, item: "b", quote: "a"},
		{name: "empty map", ratio: map[string]int64{}, item: "b", quote: "a"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := PriceOf(tt.ratio, tt.item, tt.quote)
			if ok {
				t.Errorf("ok = true, want false for %v", tt.ratio)
			}
			if got != 0 {
				t.Errorf("price = %v, want 0", got)
			}
		})
	}
}

func TestNormalize_zeroRatioRow_isKeptFlaggedAndKeepsItsStock(t *testing.T) {
	rows, stats := Normalize(loadFixtureHour(t))

	row := rowByMarketID(t, rows, zeroRatioMarket)
	if row.PriceValid {
		t.Error("PriceValid = true, want false for a 0:0 ratio row")
	}
	if row.InvalidReason != "zero_ratio" {
		t.Errorf("InvalidReason = %q, want %q", row.InvalidReason, "zero_ratio")
	}
	if row.LowestPriceBInA != 0 {
		t.Errorf("LowestPriceBInA = %v, want 0", row.LowestPriceBInA)
	}
	if row.HighestPriceBInA != 0 {
		t.Errorf("HighestPriceBInA = %v, want 0", row.HighestPriceBInA)
	}
	// Volume and stock stay usable on an unpriced row. This market traded
	// nothing in the hour but still held stock.
	if row.VolumeA != 0 || row.VolumeB != 0 {
		t.Errorf("volume = %d/%d, want 0/0", row.VolumeA, row.VolumeB)
	}
	if row.LowestStockA != 237 || row.LowestStockB != 2 {
		t.Errorf("lowest stock = %d/%d, want 237/2", row.LowestStockA, row.LowestStockB)
	}
	if row.HighestStockA != 534 || row.HighestStockB != 2 {
		t.Errorf("highest stock = %d/%d, want 534/2", row.HighestStockA, row.HighestStockB)
	}
	if stats.Invalid < 1 {
		t.Errorf("Stats.Invalid = %d, want at least 1", stats.Invalid)
	}
}

func TestNormalize_zeroZeroRatio_producesNoPriceInsteadOfDividingByZero(t *testing.T) {
	spec := validSpec()
	spec.lowestRatio = [2]int64{0, 0}
	spec.highestRatio = [2]int64{0, 0}

	rows, stats := Normalize(&HourPayload{NextChangeID: 1787119200, Markets: []Market{spec.market()}})

	if len(rows) != 1 {
		t.Fatalf("got %d rows, want the market kept", len(rows))
	}
	if rows[0].PriceValid {
		t.Error("PriceValid = true, want false")
	}
	if rows[0].LowestPriceBInA != 0 || rows[0].HighestPriceBInA != 0 {
		t.Errorf("prices = %v/%v, want 0/0", rows[0].LowestPriceBInA, rows[0].HighestPriceBInA)
	}
	if stats.Invalid != 1 {
		t.Errorf("Stats.Invalid = %d, want 1", stats.Invalid)
	}
}

func TestNormalize_partialZeroRatio_invalidatesBothPrices(t *testing.T) {
	spec := validSpec()
	spec.lowestRatio = [2]int64{196, 1}
	spec.highestRatio = [2]int64{0, 0}

	rows, stats := Normalize(&HourPayload{NextChangeID: 1787119200, Markets: []Market{spec.market()}})

	if len(rows) != 1 {
		t.Fatalf("got %d rows, want the market kept", len(rows))
	}
	row := rows[0]
	if row.PriceValid {
		t.Error("PriceValid = true, want false: one unusable ratio map invalidates the row")
	}
	if row.InvalidReason != "zero_ratio" {
		t.Errorf("InvalidReason = %q, want %q", row.InvalidReason, "zero_ratio")
	}
	// The usable lowest ratio is deliberately NOT priced: the flag is a single
	// conservative one covering both maps.
	if row.LowestPriceBInA != 0 {
		t.Errorf("LowestPriceBInA = %v, want 0", row.LowestPriceBInA)
	}
	if row.HighestPriceBInA != 0 {
		t.Errorf("HighestPriceBInA = %v, want 0", row.HighestPriceBInA)
	}
	// The raw quantities survive so a later consumer can still see 196:1.
	if row.LowestRatioA != 196 || row.LowestRatioB != 1 {
		t.Errorf("lowest ratio = %d:%d, want 196:1", row.LowestRatioA, row.LowestRatioB)
	}
	if stats.Invalid != 1 {
		t.Errorf("Stats.Invalid = %d, want 1", stats.Invalid)
	}
}

func TestNormalize_nonUnitRatioRow_pricesAsAFraction(t *testing.T) {
	rows, _ := Normalize(loadFixtureHour(t))

	// The fixture's scarab market is 2 scarabs : 3 chaos on the lowest ratio and
	// 1:1 on the highest.
	row := rowByMarketID(t, rows, scarabMarket)
	if !row.PriceValid {
		t.Fatalf("PriceValid = false (InvalidReason %q), want true", row.InvalidReason)
	}
	if row.LowestPriceBInA != 2.0/3.0 {
		t.Errorf("LowestPriceBInA = %v, want %v", row.LowestPriceBInA, 2.0/3.0)
	}
	if row.HighestPriceBInA != 1 {
		t.Errorf("HighestPriceBInA = %v, want 1", row.HighestPriceBInA)
	}
}

func TestNormalize_malformedMarket_isSkippedAndCountedWhileOthersSurvive(t *testing.T) {
	tests := []struct {
		name   string
		mutate func(m *Market)
	}{
		{
			name:   "market_pair with a single id",
			mutate: func(m *Market) { m.MarketPair = []string{omenID} },
		},
		{
			name:   "market_pair with no ids",
			mutate: func(m *Market) { m.MarketPair = nil },
		},
		{
			name:   "empty market_id",
			mutate: func(m *Market) { m.MarketID = "" },
		},
		{
			name:   "pair id missing from lowest_ratio",
			mutate: func(m *Market) { delete(m.LowestRatio, chaosID) },
		},
		{
			name:   "pair id missing from volume_traded",
			mutate: func(m *Market) { delete(m.VolumeTraded, omenID) },
		},
		{
			name:   "pair id missing from highest_stock",
			mutate: func(m *Market) { delete(m.HighestStock, chaosID) },
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			good := validSpec().market()

			badSpec := validSpec()
			badSpec.itemA = omenID
			badSpec.itemB = chaosID
			bad := badSpec.market()
			tt.mutate(&bad)

			rows, stats := Normalize(&HourPayload{
				NextChangeID: 1787119200,
				Markets:      []Market{bad, good},
			})

			if len(rows) != 1 {
				t.Fatalf("got %d rows, want only the well-formed market", len(rows))
			}
			if rows[0].MarketID != chaosDivineMarket {
				t.Errorf("surviving MarketID = %q, want %q", rows[0].MarketID, chaosDivineMarket)
			}
			if !rows[0].PriceValid {
				t.Error("surviving row PriceValid = false, want the good row still priced")
			}
			if stats.Skipped != 1 {
				t.Errorf("Stats.Skipped = %d, want 1", stats.Skipped)
			}
			if stats.Rows != 1 {
				t.Errorf("Stats.Rows = %d, want 1", stats.Rows)
			}
			if stats.Leagues["Allflame"] != 1 {
				t.Errorf("Stats.Leagues[Allflame] = %d, want 1 (skipped rows are not counted)", stats.Leagues["Allflame"])
			}
		})
	}
}

func TestNormalize_malformedMarket_warnsOnceWithTheSkippedCount(t *testing.T) {
	logs := captureLogs(t)

	badSpec := validSpec()
	badSpec.itemA = omenID
	badSpec.itemB = chaosID
	bad := badSpec.market()
	bad.MarketPair = []string{omenID}

	_, stats := Normalize(&HourPayload{
		NextChangeID: 1787119200,
		Markets:      []Market{bad, validSpec().market()},
	})
	if stats.Skipped != 1 {
		t.Fatalf("Stats.Skipped = %d, want 1: the arrangement did not produce a skip", stats.Skipped)
	}

	records := logs.records()
	if len(records) != 1 {
		t.Fatalf("got %d log records, want exactly one report for the hour", len(records))
	}
	if records[0].Level != slog.LevelWarn {
		t.Errorf("level = %v, want %v", records[0].Level, slog.LevelWarn)
	}
	if got := attrInt64(t, records[0], "skipped"); got != 1 {
		t.Errorf("skipped attr = %d, want 1", got)
	}
}

func TestNormalize_wellFormedHour_logsNothing(t *testing.T) {
	logs := captureLogs(t)

	Normalize(loadFixtureHour(t))

	if records := logs.records(); len(records) != 0 {
		t.Errorf("got %d log records, want none when nothing was skipped: %+v", len(records), records)
	}
}

func TestNormalize_fixtureHour_statsMatchTheReturnedRows(t *testing.T) {
	rows, stats := Normalize(loadFixtureHour(t))

	if stats.Rows != len(rows) {
		t.Errorf("Stats.Rows = %d, want %d (len(rows))", stats.Rows, len(rows))
	}
	if stats.Rows != 25 {
		t.Errorf("Stats.Rows = %d, want 25", stats.Rows)
	}
	if stats.Leagues["Allflame"] != 25 {
		t.Errorf("Stats.Leagues[Allflame] = %d, want 25", stats.Leagues["Allflame"])
	}
	if len(stats.Leagues) != 1 {
		t.Errorf("Stats.Leagues = %v, want the single Allflame league", stats.Leagues)
	}
	if stats.Skipped != 0 {
		t.Errorf("Stats.Skipped = %d, want 0 (the recorded hour is well-formed)", stats.Skipped)
	}
	// Two of the 25 recorded markets carry 0:0 ratios.
	if stats.Invalid != 2 {
		t.Errorf("Stats.Invalid = %d, want 2", stats.Invalid)
	}
}

func TestNormalize_mixedLeagues_keepsAndCountsEveryLeague(t *testing.T) {
	softcore := validSpec()
	hardcore := validSpec()
	hardcore.league = "Hardcore Allflame"
	hardcore.itemA = omenID
	standard := validSpec()
	standard.league = "Standard"
	standard.itemA = cardID

	rows, stats := Normalize(&HourPayload{
		NextChangeID: 1787119200,
		Markets:      []Market{softcore.market(), hardcore.market(), standard.market()},
	})

	if len(rows) != 3 {
		t.Fatalf("got %d rows, want 3: Normalize never filters by league", len(rows))
	}
	want := map[string]int{"Allflame": 1, "Hardcore Allflame": 1, "Standard": 1}
	if len(stats.Leagues) != len(want) {
		t.Fatalf("Stats.Leagues = %v, want %v", stats.Leagues, want)
	}
	for league, count := range want {
		if stats.Leagues[league] != count {
			t.Errorf("Stats.Leagues[%q] = %d, want %d", league, stats.Leagues[league], count)
		}
	}
	leagues := map[string]int{}
	for _, row := range rows {
		leagues[row.League]++
	}
	for league, count := range want {
		if leagues[league] != count {
			t.Errorf("returned rows for league %q = %d, want %d", league, leagues[league], count)
		}
	}
}

func TestNormalize_emptyPayload_returnsNoRowsAndZeroStats(t *testing.T) {
	rows, stats := Normalize(&HourPayload{})

	if len(rows) != 0 {
		t.Errorf("got %d rows, want 0", len(rows))
	}
	if stats.Rows != 0 || stats.Invalid != 0 || stats.Skipped != 0 {
		t.Errorf("stats = %+v, want all counters 0", stats)
	}
	if len(stats.Leagues) != 0 {
		t.Errorf("Stats.Leagues = %v, want empty", stats.Leagues)
	}
	if stats.Leagues == nil {
		t.Error("Stats.Leagues = nil, want an allocated map so callers can read it")
	}
}

func TestNormalize_nilPayload_returnsNoRowsAndZeroStats(t *testing.T) {
	rows, stats := Normalize(nil)

	if len(rows) != 0 {
		t.Errorf("got %d rows, want 0", len(rows))
	}
	if stats.Rows != 0 || stats.Invalid != 0 || stats.Skipped != 0 {
		t.Errorf("stats = %+v, want all counters 0", stats)
	}
	if stats.Leagues == nil {
		t.Error("Stats.Leagues = nil, want an allocated map so callers can read it")
	}
}

// logCapture is a slog.Handler that keeps every record it is handed.
type logCapture struct {
	mu      sync.Mutex
	entries []slog.Record
}

func (c *logCapture) Enabled(context.Context, slog.Level) bool { return true }

func (c *logCapture) Handle(_ context.Context, rec slog.Record) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.entries = append(c.entries, rec.Clone())
	return nil
}

func (c *logCapture) WithAttrs([]slog.Attr) slog.Handler { return c }

func (c *logCapture) WithGroup(string) slog.Handler { return c }

func (c *logCapture) records() []slog.Record {
	c.mu.Lock()
	defer c.mu.Unlock()
	return append([]slog.Record(nil), c.entries...)
}

// captureLogs points the default logger at a logCapture for one test and puts
// the previous logger back afterwards.
func captureLogs(t *testing.T) *logCapture {
	t.Helper()
	capture := &logCapture{}
	previous := slog.Default()
	slog.SetDefault(slog.New(capture))
	t.Cleanup(func() { slog.SetDefault(previous) })
	return capture
}

// attrInt64 returns the named integer attribute of a captured record.
func attrInt64(t *testing.T, rec slog.Record, key string) int64 {
	t.Helper()
	var (
		value int64
		found bool
	)
	rec.Attrs(func(a slog.Attr) bool {
		if a.Key != key {
			return true
		}
		value, found = a.Value.Int64(), true
		return false
	})
	if !found {
		t.Fatalf("record %q carries no %q attribute", rec.Message, key)
	}
	return value
}
