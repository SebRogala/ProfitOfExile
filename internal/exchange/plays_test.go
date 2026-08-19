package exchange

import (
	"encoding/json"
	"math"
	"reflect"
	"sort"
	"strconv"
	"testing"
	"time"
)

// storedAt stamps rows with the feed hour they were published in, the shape
// Repository.LoadRows hands BestPlays.
func storedAt(hour time.Time, rows ...Row) []StoredRow {
	stored := make([]StoredRow, 0, len(rows))
	for _, row := range rows {
		stored = append(stored, StoredRow{Hour: hour, Row: row})
	}
	return stored
}

// quotedInDivine is a market where item is priced in divine: low divine each at
// the hour's cheapest, high at the dearest. Divine leads the default quote
// priority, so the traded side is item, the direct edge is high/low - 1 and the
// depth is volume.
func quotedInDivine(item string, low, high, volume int64) rowSpec {
	return rowSpec{
		itemA:        divineID,
		itemB:        item,
		volume:       [2]int64{volume, volume},
		highestStock: [2]int64{50, 50},
		lowestRatio:  [2]int64{low, 1},
		highestRatio: [2]int64{high, 1},
	}
}

// arbitrageTriangle is a hand-built loop whose every price is exact in binary:
// a scarab costs a sixteenth of a divine, sells for eight chaos, and 64 chaos
// buy the divine back — one turn doubles the stake, so the scarab's
// divine-to-chaos route carries an edge of exactly 1 and its mirror an edge of
// exactly -0.5. Each market's lowest and highest ratio are the same, so no
// market has a direct edge of its own and the whole gain sits in the loop.
func arbitrageTriangle() []Row {
	market := func(itemA, itemB string, ratio [2]int64) Row {
		return rowSpec{
			itemA:        itemA,
			itemB:        itemB,
			volume:       [2]int64{100, 100},
			highestStock: [2]int64{50, 50},
			lowestRatio:  ratio,
			highestRatio: ratio,
		}.row()
	}
	return []Row{
		market(divineID, scarabID, [2]int64{1, 16}),
		market(chaosID, scarabID, [2]int64{8, 1}),
		market(chaosID, divineID, [2]int64{64, 1}),
	}
}

// playKeys lists the ranked keys in order.
func playKeys(plays []Play) []string {
	keys := make([]string, 0, len(plays))
	for _, play := range plays {
		keys = append(keys, play.Key)
	}
	return keys
}

// playByKey returns the single ranked play carrying key.
func playByKey(t *testing.T, result Result, key string) Play {
	t.Helper()
	for _, play := range result.Plays {
		if play.Key == key {
			return play
		}
	}
	t.Fatalf("no play keyed %q (got %v)", key, playKeys(result.Plays))
	return Play{}
}

// directKey names the direct play of a market quoted in divine.
func directKey(item string) string {
	return "direct:" + divineID + "|" + item
}

func TestBestPlays_moreHoursThanTheWindow_aggregatesOnlyTheNewestOnes(t *testing.T) {
	var rows []StoredRow
	for i := 0; i < 8; i++ {
		rows = append(rows, storedAt(feedHour.Add(time.Duration(i)*time.Hour), chaosDivineSpec().row())...)
	}

	got := BestPlays("Allflame", rows, DefaultConfig())

	if got.League != "Allflame" {
		t.Errorf("League = %q, want %q", got.League, "Allflame")
	}
	if got.Hours != 6 {
		t.Errorf("Hours = %d, want 6 (the window), not 8 (the rows)", got.Hours)
	}
	// The window is half-open: the two oldest hours are left out and To is the
	// newest hour plus one.
	if want := feedHour.Add(2 * time.Hour); !got.From.Equal(want) {
		t.Errorf("From = %v, want %v", got.From, want)
	}
	if want := feedHour.Add(8 * time.Hour); !got.To.Equal(want) {
		t.Errorf("To = %v, want %v", got.To, want)
	}
	if play := playByKey(t, got, "direct:"+chaosDivineMarket); play.HoursSeen != 6 {
		t.Errorf("HoursSeen = %d, want 6", play.HoursSeen)
	}
}

func TestBestPlays_twoHours_weightsTheNewerEdgeTwiceAsHeavily(t *testing.T) {
	// Ten percent in the newest hour, twenty in the one before: with N = 2 the
	// weights are 1 and 1/2, so the mean leans on the newer print.
	newest := quotedInDivine(cardID, 100, 110, 100).row()
	older := quotedInDivine(cardID, 100, 120, 100).row()
	rows := append(storedAt(feedHour, newest), storedAt(feedHour.Add(-time.Hour), older)...)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(cardID))
	wantClose(t, "Edge", play.Edge, (1.0*0.10+0.5*0.20)/1.5)
}

func TestBestPlays_twoHours_averagesLegVolumeWithTheSameWeights(t *testing.T) {
	newest := quotedInDivine(cardID, 2, 4, 100).row()
	older := quotedInDivine(cardID, 2, 4, 40).row()
	rows := append(storedAt(feedHour, newest), storedAt(feedHour.Add(-time.Hour), older)...)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(cardID))
	want := (1.0*100 + 0.5*40) / 1.5
	for i, leg := range play.Legs {
		wantClose(t, "leg "+strconv.Itoa(i)+" volume", leg.Volume, want)
	}
	wantClose(t, "Depth", play.Depth, want)
}

func TestBestPlays_playSeenInTwoHours_snapshotsTheNewestHour(t *testing.T) {
	// The older hour is deliberately the more dramatic one: its prices and
	// stocks must not leak into the play.
	older := chaosDivineSpec()
	older.lowestRatio = [2]int64{150, 1}
	older.highestRatio = [2]int64{300, 1}
	older.highestStock = [2]int64{1000, 20}
	rows := append(
		storedAt(feedHour, chaosDivineSpec().row()),
		storedAt(feedHour.Add(-time.Hour), older.row())...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, "direct:"+chaosDivineMarket)
	if !play.LastHour.Equal(feedHour) {
		t.Errorf("LastHour = %v, want %v", play.LastHour, feedHour)
	}
	if play.HoursSeen != 2 {
		t.Errorf("HoursSeen = %d, want 2", play.HoursSeen)
	}
	if len(play.Legs) != 2 {
		t.Fatalf("got %d legs, want 2", len(play.Legs))
	}
	// The older hour would have priced the buy at 1/300 and stocked 1000.
	wantPrices := []float64{1.0 / 201.0, 1.0 / 196.0}
	for i, leg := range play.Legs {
		if leg.Price != wantPrices[i] {
			t.Errorf("leg %d price = %v, want the newest hour's %v", i, leg.Price, wantPrices[i])
		}
		if leg.Stock != 4564191 {
			t.Errorf("leg %d stock = %d, want the newest hour's 4564191", i, leg.Stock)
		}
	}
}

func TestBestPlays_oneHopPlay_depthIsItsThinnestLeg(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, triangle()...), DefaultConfig())

	play := playByKey(t, got, oneHopKey(cardID, chaosID, divineID))
	wantVolumes := []float64{300, 250, 65361}
	for i, leg := range play.Legs {
		if leg.Volume != wantVolumes[i] {
			t.Errorf("leg %d volume = %v, want %v", i, leg.Volume, wantVolumes[i])
		}
	}
	// The card sold against divine is the bottleneck: the recipe cannot move
	// more units per hour than its thinnest step.
	if play.Depth != 250 {
		t.Errorf("Depth = %v, want 250", play.Depth)
	}
}

func TestBestPlays_singleHourWindow_capsMinHoursSeenAtTheHoursPresent(t *testing.T) {
	// The default asks for two hours; only one exists. Capping is what keeps a
	// fresh league (or a just-restarted walk) from returning nothing at all.
	got := BestPlays("Allflame", storedAt(feedHour, chaosDivineSpec().row()), DefaultConfig())

	play := playByKey(t, got, "direct:"+chaosDivineMarket)
	if play.HoursSeen != 1 {
		t.Errorf("HoursSeen = %d, want 1", play.HoursSeen)
	}
}

func TestBestPlays_playSeenInOnlyOneOfTwoHours_isDroppedAsAGhost(t *testing.T) {
	steady := chaosDivineSpec().row()
	ghost := quotedInDivine(cardID, 2, 4, 100).row()
	rows := append(
		storedAt(feedHour, steady, ghost),
		storedAt(feedHour.Add(-time.Hour), steady)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	// The ghost carries by far the bigger edge (100% against 2.5%) and is still
	// dropped: printing once in the window is the disqualifier.
	if want := []string{"direct:" + chaosDivineMarket}; !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want %v", playKeys(got.Plays), want)
	}
}

func TestBestPlays_minEdge_cutsThePlaysBelowTheFloor(t *testing.T) {
	rows := storedAt(feedHour,
		quotedInDivine(hellID, 2, 4, 100).row(), // edge 1.0
		quotedInDivine(cardID, 2, 3, 100).row(), // edge 0.5
	)

	tests := []struct {
		name    string
		minEdge float64
		want    []string
	}{
		{
			name:    "floor below both edges keeps both",
			minEdge: 0.25,
			want:    []string{directKey(hellID), directKey(cardID)},
		},
		{
			name:    "floor exactly on the smaller edge keeps it",
			minEdge: 0.5,
			want:    []string{directKey(hellID), directKey(cardID)},
		},
		{
			name:    "floor a hair above the smaller edge drops it",
			minEdge: 0.5000001,
			want:    []string{directKey(hellID)},
		},
		{
			name:    "floor above both edges keeps nothing",
			minEdge: 1.5,
			want:    []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			cfg := DefaultConfig()
			cfg.MinEdge = tt.minEdge

			got := BestPlays("Allflame", rows, cfg)
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v", playKeys(got.Plays), tt.want)
			}
		})
	}
}

func TestBestPlays_negativeMinEdge_keepsTheLosingRoutes(t *testing.T) {
	// Only a negative floor can surface the unprofitable direction of a loop,
	// which is why an explicitly negative MinEdge is not treated as unset.
	cfg := DefaultConfig()
	cfg.MinEdge = -1

	got := BestPlays("Allflame", storedAt(feedHour, arbitrageTriangle()...), cfg)

	losing := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	wantClose(t, "Edge", losing.Edge, -0.5)
}

func TestBestPlays_maxPlays_truncatesToTheHighestRanked(t *testing.T) {
	rows := storedAt(feedHour,
		quotedInDivine(hellID, 2, 4, 100).row(),   // edge 1.0
		quotedInDivine(cardID, 2, 3, 500).row(),   // edge 0.5, deepest
		quotedInDivine(omenID, 2, 3, 100).row(),   // edge 0.5
		quotedInDivine(scarabID, 2, 3, 100).row(), // edge 0.5
	)
	cfg := DefaultConfig()
	cfg.MaxPlays = 2

	got := BestPlays("Allflame", rows, cfg)

	want := []string{directKey(hellID), directKey(cardID)}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want the two highest ranked %v", playKeys(got.Plays), want)
	}
}

func TestBestPlays_ranksByEdgeThenDepthThenKey(t *testing.T) {
	// Three of the four markets are crafted to the same edge, and two of those
	// to the same depth, so each tie-break decides exactly one position.
	rows := storedAt(feedHour,
		quotedInDivine(scarabID, 2, 3, 100).row(), // edge 0.5, depth 100
		quotedInDivine(omenID, 2, 3, 100).row(),   // edge 0.5, depth 100
		quotedInDivine(cardID, 2, 3, 500).row(),   // edge 0.5, depth 500
		quotedInDivine(hellID, 2, 4, 100).row(),   // edge 1.0
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	want := []string{
		directKey(hellID),   // biggest edge
		directKey(cardID),   // same edge as the rest, deepest
		directKey(omenID),   // tied on edge and depth, smaller key
		directKey(scarabID), // tied on edge and depth, bigger key
	}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want %v", playKeys(got.Plays), want)
	}
}

func TestBestPlays_directPlayTiedWithAOneHop_isRankedFirst(t *testing.T) {
	// The loop's winning route and the flip both carry an edge of exactly 1 and
	// a depth of exactly 100, so only the mode can separate them. The flip's key
	// sorts behind the route's, which is how this proves the mode tie-break
	// outranks the key tie-break.
	rows := storedAt(feedHour, append(arbitrageTriangle(), quotedInDivine(cardID, 2, 4, 100).row())...)

	got := BestPlays("Allflame", rows, DefaultConfig())

	want := []string{
		directKey(cardID),
		oneHopKey(scarabID, divineID, chaosID),
	}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want the direct flip ahead of the loop %v", playKeys(got.Plays), want)
	}
	for _, play := range got.Plays {
		wantClose(t, play.Key+" edge", play.Edge, 1)
		wantClose(t, play.Key+" depth", play.Depth, 100)
	}
}

func TestModeRank_unknownMode_sortsBehindBothShapes(t *testing.T) {
	// Nothing in the engine emits a third mode today; the rank exists so that
	// adding one without touching the comparator parks it at the back of a tie
	// rather than ahead of the two shapes that were ranked deliberately.
	if got := modeRank(Mode("2-hop")); got <= modeRank(ModeOneHop) {
		t.Errorf("modeRank(2-hop) = %d, want more than the one-hop rank %d", got, modeRank(ModeOneHop))
	}
}

func TestBestPlays_marketRepeatedWithinOneHour_countsAsOneHour(t *testing.T) {
	// Two rows for the same market in one hour is a storage accident, not a
	// second sighting: counting it twice would let a duplicate satisfy the
	// ghost filter on its own.
	row := chaosDivineSpec().row()

	got := BestPlays("Allflame", storedAt(feedHour, row, row), DefaultConfig())

	play := playByKey(t, got, "direct:"+chaosDivineMarket)
	if play.HoursSeen != 1 {
		t.Errorf("HoursSeen = %d, want 1", play.HoursSeen)
	}
	if play.Legs[0].Volume != 13001051 {
		t.Errorf("leg volume = %v, want the hour's traded volume 13001051", play.Legs[0].Volume)
	}
}

func TestBestPlays_noRows_returnsAnEmptyWindow(t *testing.T) {
	got := BestPlays("Allflame", nil, DefaultConfig())

	if got.League != "Allflame" {
		t.Errorf("League = %q, want %q", got.League, "Allflame")
	}
	if got.Hours != 0 {
		t.Errorf("Hours = %d, want 0", got.Hours)
	}
	if !got.From.IsZero() || !got.To.IsZero() {
		t.Errorf("window = %v..%v, want the zero times", got.From, got.To)
	}
	if got.Plays == nil {
		t.Fatal("Plays = nil, want an allocated empty slice so it marshals as []")
	}
	if len(got.Plays) != 0 {
		t.Errorf("Plays = %v, want none", playKeys(got.Plays))
	}
}

func TestBestPlays_recordedHour_ranksFinitePlaysOfBothShapes(t *testing.T) {
	rows, _ := Normalize(loadFixtureHour(t))

	got := BestPlays("Allflame", storedAt(feedHour, rows...), DefaultConfig())

	// The recorded hour is frozen input: 23 priced markets out of 25 have to
	// yield plays of both shapes, and every number in them has to be finite and
	// positive. The exact counts are a characterization, not a spec — measured
	// on this fixture under DefaultConfig: 18 direct and 8 one-hop — so they are
	// read as "some of each" rather than pinned, which would make an unrelated
	// tuning change look like a regression.
	direct, oneHop := 0, 0
	for _, play := range got.Plays {
		switch play.Mode {
		case ModeDirect:
			direct++
		case ModeOneHop:
			oneHop++
		default:
			t.Errorf("%s: mode = %q, want %q or %q", play.Key, play.Mode, ModeDirect, ModeOneHop)
		}

		if math.IsNaN(play.Edge) || math.IsInf(play.Edge, 0) {
			t.Errorf("%s: Edge = %v, want a finite number", play.Key, play.Edge)
		}
		if play.Depth <= 0 {
			t.Errorf("%s: Depth = %v, want the thinnest leg's volume", play.Key, play.Depth)
		}
		if len(play.Legs) < 2 {
			t.Errorf("%s: %d legs, want at least the buy and the sell", play.Key, len(play.Legs))
		}
		for i, leg := range play.Legs {
			if !(leg.Price > 0) || math.IsInf(leg.Price, 0) {
				t.Errorf("%s: leg %d price = %v, want a positive finite price", play.Key, i, leg.Price)
			}
		}
	}
	if direct <= 0 {
		t.Errorf("direct plays = %d, want the recorded hour to yield same-market flips", direct)
	}
	if oneHop <= 0 {
		t.Errorf("one-hop plays = %d, want the recorded hour to yield cross-quote routes", oneHop)
	}
}

func TestBestPlays_zeroValueConfig_scoresTheHourLikeDefaultConfig(t *testing.T) {
	rows, _ := Normalize(loadFixtureHour(t))
	stored := storedAt(feedHour, rows...)

	got := BestPlays("Allflame", stored, Config{})

	if want := BestPlays("Allflame", stored, DefaultConfig()); !reflect.DeepEqual(got, want) {
		t.Errorf("zero-value config produced %d plays (%v...), want the DefaultConfig result of %d",
			len(got.Plays), playKeys(got.Plays), len(want.Plays))
	}
}

func TestDefaultConfig_isTheDocumentedTuning(t *testing.T) {
	want := Config{
		WindowHours:      6,
		MinVolumePerHour: 10,
		MinEdge:          0.02,
		MinHoursSeen:     2,
		MaxPlays:         100,
		QuotePriority:    []string{DivineID, ChaosID},
	}

	if got := DefaultConfig(); !reflect.DeepEqual(got, want) {
		t.Errorf("DefaultConfig() = %+v, want %+v", got, want)
	}
}

func TestConfigWithDefaults_zeroValue_fillsEveryFieldFromDefaultConfig(t *testing.T) {
	if got, want := (Config{}).withDefaults(), DefaultConfig(); !reflect.DeepEqual(got, want) {
		t.Errorf("Config{}.withDefaults() = %+v, want %+v", got, want)
	}
}

func TestConfigWithDefaults_fillsTheUnsetFieldsIndependently(t *testing.T) {
	tests := []struct {
		name string
		cfg  Config
		want Config
	}{
		{
			name: "only MaxPlays set",
			cfg:  Config{MaxPlays: 3},
			want: withField(func(c *Config) { c.MaxPlays = 3 }),
		},
		{
			name: "only WindowHours set",
			cfg:  Config{WindowHours: 24},
			want: withField(func(c *Config) { c.WindowHours = 24 }),
		},
		{
			name: "only MinVolumePerHour set",
			cfg:  Config{MinVolumePerHour: 0.5},
			want: withField(func(c *Config) { c.MinVolumePerHour = 0.5 }),
		},
		{
			name: "only MinEdge set",
			cfg:  Config{MinEdge: 0.5},
			want: withField(func(c *Config) { c.MinEdge = 0.5 }),
		},
		{
			name: "only MinHoursSeen set",
			cfg:  Config{MinHoursSeen: 4},
			want: withField(func(c *Config) { c.MinHoursSeen = 4 }),
		},
		{
			name: "only QuotePriority set",
			cfg:  Config{QuotePriority: []string{ChaosID}},
			want: withField(func(c *Config) { c.QuotePriority = []string{ChaosID} }),
		},
		{
			name: "a negative MinEdge is a choice, not an unset field",
			cfg:  Config{MinEdge: -0.5},
			want: withField(func(c *Config) { c.MinEdge = -0.5 }),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.cfg.withDefaults(); !reflect.DeepEqual(got, tt.want) {
				t.Errorf("withDefaults() = %+v, want %+v", got, tt.want)
			}
		})
	}
}

func TestConfigWithDefaults_nonPositiveCount_fallsBackToTheDefault(t *testing.T) {
	// Negative counts and floors have no meaning, so they read as unset — the
	// one exception being MinEdge, which the table above pins.
	cfg := Config{WindowHours: -1, MinVolumePerHour: -1, MinHoursSeen: -1, MaxPlays: -1}

	if got, want := cfg.withDefaults(), DefaultConfig(); !reflect.DeepEqual(got, want) {
		t.Errorf("withDefaults() = %+v, want %+v", got, want)
	}
}

// withField returns DefaultConfig with one field overridden.
func withField(set func(*Config)) Config {
	cfg := DefaultConfig()
	set(&cfg)
	return cfg
}

func TestResult_marshalsWithTheFieldNamesTheHandlerPublishes(t *testing.T) {
	result := Result{
		League: "Allflame",
		From:   feedHour,
		To:     feedHour.Add(time.Hour),
		Hours:  1,
		Plays: []Play{{
			Key:  "direct:" + chaosDivineMarket,
			Mode: ModeDirect,
			Legs: []Leg{
				{Action: "buy", Item: chaosID, Quote: divineID, Price: 1.0 / 201.0, Volume: 13001051, Stock: 4564191},
				{Action: "sell", Item: chaosID, Quote: divineID, Price: 1.0 / 196.0, Volume: 13001051, Stock: 4564191},
			},
			Edge:      201.0/196.0 - 1,
			Depth:     13001051,
			HoursSeen: 1,
			LastHour:  feedHour,
		}},
	}

	data, err := json.Marshal(result)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	var envelope struct {
		Plays []map[string]json.RawMessage `json:"plays"`
	}
	if err := json.Unmarshal(data, &envelope); err != nil {
		t.Fatalf("decode plays: %v", err)
	}
	if len(envelope.Plays) != 1 {
		t.Fatalf("got %d plays, want 1", len(envelope.Plays))
	}
	wantKeys(t, "result", data, "league", "from", "to", "hours", "plays")
	wantKeys(t, "play", mustMarshal(t, envelope.Plays[0]), "key", "mode", "legs", "edge", "depth", "hoursSeen", "lastHour")

	var legs []map[string]json.RawMessage
	if err := json.Unmarshal(envelope.Plays[0]["legs"], &legs); err != nil {
		t.Fatalf("decode legs: %v", err)
	}
	wantKeys(t, "leg", mustMarshal(t, legs[0]), "action", "item", "quote", "price", "volume", "stock")

	if got := string(envelope.Plays[0]["mode"]); got != `"direct"` {
		t.Errorf("mode = %s, want %q", got, "direct")
	}

	var round Result
	if err := json.Unmarshal(data, &round); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if !reflect.DeepEqual(round, result) {
		t.Errorf("round trip = %+v, want %+v", round, result)
	}
}

// wantKeys fails unless the JSON object carries exactly the named keys.
func wantKeys(t *testing.T, label string, data []byte, want ...string) {
	t.Helper()
	var object map[string]json.RawMessage
	if err := json.Unmarshal(data, &object); err != nil {
		t.Fatalf("decode %s: %v", label, err)
	}
	got := make([]string, 0, len(object))
	for key := range object {
		got = append(got, key)
	}
	sort.Strings(got)
	sort.Strings(want)
	if !reflect.DeepEqual(got, want) {
		t.Errorf("%s JSON keys = %v, want %v", label, got, want)
	}
}

// mustMarshal re-encodes a decoded object so its keys can be inspected.
func mustMarshal(t *testing.T, object map[string]json.RawMessage) []byte {
	t.Helper()
	data, err := json.Marshal(object)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	return data
}
