package exchange

import (
	"reflect"
	"testing"
)

// cardChaosSpec is the card quoted against chaos: 10 chaos each at the hour's
// cheapest, 12 at the dearest.
func cardChaosSpec() rowSpec {
	return rowSpec{
		itemA:        chaosID,
		itemB:        cardID,
		volume:       [2]int64{5000, 300},
		highestStock: [2]int64{900, 40},
		lowestRatio:  [2]int64{10, 1},
		highestRatio: [2]int64{12, 1},
	}
}

// cardDivineSpec is the same card quoted against divine: 20 cards to the divine
// at the cheapest, 16 at the dearest.
func cardDivineSpec() rowSpec {
	return rowSpec{
		itemA:        divineID,
		itemB:        cardID,
		volume:       [2]int64{80, 250},
		highestStock: [2]int64{70, 35},
		lowestRatio:  [2]int64{1, 20},
		highestRatio: [2]int64{1, 16},
	}
}

// triangle is one hour holding the card quoted against both currencies plus the
// chaos/divine market that closes the loop — the smallest input that can carry a
// one-hop play.
func triangle() []Row {
	return []Row{cardChaosSpec().row(), cardDivineSpec().row(), chaosDivineSpec().row()}
}

// scarabChaosSpec is a second traded item on the same two currencies: 20 chaos
// each at the hour's cheapest, 24 at the dearest.
func scarabChaosSpec() rowSpec {
	return rowSpec{
		itemA:        chaosID,
		itemB:        scarabID,
		volume:       [2]int64{4000, 200},
		highestStock: [2]int64{800, 30},
		lowestRatio:  [2]int64{20, 1},
		highestRatio: [2]int64{24, 1},
	}
}

// scarabDivineSpec is that second item quoted against divine: 10 scarabs to the
// divine at the cheapest, 8 at the dearest.
func scarabDivineSpec() rowSpec {
	return rowSpec{
		itemA:        divineID,
		itemB:        scarabID,
		volume:       [2]int64{60, 150},
		highestStock: [2]int64{60, 25},
		lowestRatio:  [2]int64{1, 10},
		highestRatio: [2]int64{1, 8},
	}
}

// oneHopKey spells the route key the way a reader of a play would: the traded
// item, the currency it is bought with, the currency it is sold for.
func oneHopKey(item, buyQuote, sellQuote string) string {
	return "1-hop:" + item + "|" + buyQuote + "|" + sellQuote
}

// candidateByKey returns the single candidate carrying key.
func candidateByKey(t *testing.T, candidates []candidate, key string) candidate {
	t.Helper()
	for _, c := range candidates {
		if c.key == key {
			return c
		}
	}
	t.Fatalf("no candidate keyed %q (got %v)", key, candidateKeys(candidates))
	return candidate{}
}

// candidateKeys lists the keys in the order the unit emitted them.
func candidateKeys(candidates []candidate) []string {
	keys := make([]string, 0, len(candidates))
	for _, c := range candidates {
		keys = append(keys, c.key)
	}
	return keys
}

// cardInChaos, cardInDivine and divineInChaos are the three legs the triangle's
// routes execute, as the hour observed them. They are spelled out once because
// both routes walk the same three markets in different orders and directions.
func cardInChaos(action string) candidateLeg {
	return candidateLeg{
		action: action, item: cardID, quote: chaosID,
		obs: obs{
			low: 10, high: 12,
			vwap: 5000.0 / 300.0, vwapOK: true,
			tick:        1.0 / 10.0,
			quoteVolume: 5000, volume: 300, stock: 40,
		},
	}
}

func cardInDivine(action string) candidateLeg {
	return candidateLeg{
		action: action, item: cardID, quote: divineID,
		obs: obs{
			low: 1.0 / 20.0, high: 1.0 / 16.0,
			vwap: 80.0 / 250.0, vwapOK: true,
			tick:        1.0 / 16.0,
			quoteVolume: 80, volume: 250, stock: 35,
		},
	}
}

func divineInChaos(action string) candidateLeg {
	return candidateLeg{
		action: action, item: divineID, quote: chaosID,
		obs: obs{
			low: 196, high: 201,
			vwap: 13001051.0 / 65361.0, vwapOK: true,
			tick:        1.0 / 196.0,
			quoteVolume: 13001051, volume: 65361, stock: 8878,
		},
	}
}

func chaosInDivine(action string) candidateLeg {
	return candidateLeg{
		action: action, item: chaosID, quote: divineID,
		obs: obs{
			low: 1.0 / 201.0, high: 1.0 / 196.0,
			vwap: 65361.0 / 13001051.0, vwapOK: true,
			tick:        1.0 / 196.0,
			quoteVolume: 65361, volume: 13001051, stock: 4564191,
		},
	}
}

func TestCrossQuoteCandidates_cardBoughtInChaos_observesTheThreeMarketsItWalks(t *testing.T) {
	got := crossQuoteCandidates(triangle(), DefaultConfig())

	c := candidateByKey(t, got, oneHopKey(cardID, chaosID, divineID))
	// Each leg carries the hour as ITS market saw it: the chaos market's tenths,
	// the divine market's sixteenths and the chaos/divine market's depth. The
	// route buys on one market and sells on the other two, so each leg's action
	// decides which end of its own spread the play is priced from later.
	wantLegs := []candidateLeg{
		cardInChaos("buy"),
		cardInDivine("sell"),
		divineInChaos("sell"),
	}
	if !reflect.DeepEqual(c.legs, wantLegs) {
		t.Errorf("legs = %+v, want %+v", c.legs, wantLegs)
	}
	if c.mode != ModeOneHop {
		t.Errorf("mode = %q, want %q", c.mode, ModeOneHop)
	}
}

func TestCrossQuoteCandidates_cardBoughtInChaos_edgeIsTheProductOfThreeHourlyExtremes(t *testing.T) {
	got := crossQuoteCandidates(triangle(), DefaultConfig())

	c := candidateByKey(t, got, oneHopKey(cardID, chaosID, divineID))
	// Ten chaos buys a card, the card sells for a sixteenth of a divine, and
	// the divine sells back for 201 chaos.
	wantClose(t, "edge", c.edge, (1.0/16.0)*201.0/10.0-1)
}

func TestCrossQuoteCandidates_cardBoughtInDivine_observesTheThreeMarketsItWalks(t *testing.T) {
	got := crossQuoteCandidates(triangle(), DefaultConfig())

	c := candidateByKey(t, got, oneHopKey(cardID, divineID, chaosID))
	// The mirror walks the same three markets the other way round, so the
	// closing leg is now chaos priced in divine — the same market as the route
	// above, read from its other side.
	wantLegs := []candidateLeg{
		cardInDivine("buy"),
		cardInChaos("sell"),
		chaosInDivine("sell"),
	}
	if !reflect.DeepEqual(c.legs, wantLegs) {
		t.Errorf("legs = %+v, want %+v", c.legs, wantLegs)
	}
}

func TestCrossQuoteCandidates_cardBoughtInDivine_edgeIsTheProductOfThreeHourlyExtremes(t *testing.T) {
	got := crossQuoteCandidates(triangle(), DefaultConfig())

	c := candidateByKey(t, got, oneHopKey(cardID, divineID, chaosID))
	wantClose(t, "edge", c.edge, 12.0*(1.0/196.0)/(1.0/20.0)-1)
}

func TestCrossQuoteCandidates_triangle_tradesOnlyTheItemThatIsNotAQuoteCurrency(t *testing.T) {
	got := crossQuoteCandidates(triangle(), DefaultConfig())

	// The three markets close a loop that could be walked from any of its three
	// corners, but a cross-quote play is one ITEM against two CURRENCIES: only
	// the card can be the traded item, and its two currencies give it a
	// direction and a mirror. Chaos and divine are quote currencies, so neither
	// rotation that trades them is a play.
	want := []string{
		oneHopKey(cardID, divineID, chaosID),
		oneHopKey(cardID, chaosID, divineID),
	}
	if !reflect.DeepEqual(candidateKeys(got), want) {
		t.Errorf("keys = %v, want %v", candidateKeys(got), want)
	}
	for _, c := range got {
		if len(c.legs) != 3 {
			t.Errorf("%s has %d legs, want exactly 3", c.key, len(c.legs))
		}
	}
}

func TestCrossQuoteCandidates_loopThatIsNotOneItemAgainstTwoCurrencies_producesNoRoute(t *testing.T) {
	// Both halves of the rule have to bite, and each is checked on a loop whose
	// three markets close perfectly — what disqualifies them is which side of
	// Config.QuotePriority their items sit on, nothing about their prices.
	nonCurrencyMarket := func(itemA, itemB string) Row {
		return rowSpec{
			itemA:        itemA,
			itemB:        itemB,
			volume:       [2]int64{500, 500},
			highestStock: [2]int64{50, 50},
			lowestRatio:  [2]int64{2, 1},
			highestRatio: [2]int64{3, 1},
		}.row()
	}

	tests := []struct {
		name string
		rows []Row
		cfg  Config
	}{
		{
			name: "no side of the loop is a quote currency",
			rows: []Row{
				nonCurrencyMarket(cardID, scarabID),
				nonCurrencyMarket(cardID, hellID),
				nonCurrencyMarket(scarabID, hellID),
			},
			cfg: DefaultConfig(),
		},
		{
			name: "every side of the loop is a quote currency",
			rows: triangle(),
			cfg:  withField(func(c *Config) { c.QuotePriority = []string{DivineID, ChaosID, cardID} }),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := crossQuoteCandidates(tt.rows, tt.cfg); len(got) != 0 {
				t.Errorf("got %v, want no routes", candidateKeys(got))
			}
		})
	}
}

func TestCrossQuoteCandidates_quotePriorityNamingTheCard_makesDivineTheTradedItem(t *testing.T) {
	// Currency is not a hard-coded list: the same three markets trade the card
	// under the default priority and trade divine when the priority says the
	// card and chaos are the currencies.
	cfg := withField(func(c *Config) { c.QuotePriority = []string{ChaosID, cardID} })

	got := crossQuoteCandidates(triangle(), cfg)

	want := []string{
		oneHopKey(divineID, chaosID, cardID),
		oneHopKey(divineID, cardID, chaosID),
	}
	if !reflect.DeepEqual(candidateKeys(got), want) {
		t.Errorf("keys = %v, want %v", candidateKeys(got), want)
	}
}

func TestCrossQuoteCandidates_hourWithoutTheClosingMarket_producesNoRoute(t *testing.T) {
	tests := []struct {
		name string
		rows []Row
	}{
		{
			name: "the card is quoted in both currencies but they do not trade against each other",
			rows: []Row{cardChaosSpec().row(), cardDivineSpec().row()},
		},
		{
			name: "the card has a single counterparty",
			rows: []Row{cardChaosSpec().row(), chaosDivineSpec().row()},
		},
		{
			name: "a single market cannot close anything",
			rows: []Row{chaosDivineSpec().row()},
		},
		{
			name: "no rows at all",
			rows: nil,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := crossQuoteCandidates(tt.rows, DefaultConfig()); len(got) != 0 {
				t.Errorf("got %v, want no routes", candidateKeys(got))
			}
		})
	}
}

func TestCrossQuoteCandidates_thinLegOnOneMarket_dropsOnlyTheRoutesThatTradeIt(t *testing.T) {
	// Nine cards traded against divine, one below the default floor. Both card
	// routes have to trade a card on that market and die with it; the scarab
	// routes, which are quoted against the same two currencies and never touch
	// it, survive untouched.
	thin := cardDivineSpec()
	thin.volume[1] = 9

	rows := []Row{
		cardChaosSpec().row(), thin.row(), chaosDivineSpec().row(),
		scarabChaosSpec().row(), scarabDivineSpec().row(),
	}

	got := crossQuoteCandidates(rows, DefaultConfig())

	want := []string{
		oneHopKey(scarabID, divineID, chaosID),
		oneHopKey(scarabID, chaosID, divineID),
	}
	if !reflect.DeepEqual(candidateKeys(got), want) {
		t.Errorf("keys = %v, want %v", candidateKeys(got), want)
	}
}

func TestCrossQuoteCandidates_unusableClosingMarket_dropsEveryRouteThroughIt(t *testing.T) {
	// Every route in a three-market triangle executes its last leg on the
	// chaos/divine market, so breaking that market alone empties the hour.
	tests := []struct {
		name      string
		breakSpec func(s *rowSpec)
	}{
		{
			name:      "no stock of divine to sell back",
			breakSpec: func(s *rowSpec) { s.highestStock[1] = 0 },
		},
		{
			name:      "no stock of chaos to sell back",
			breakSpec: func(s *rowSpec) { s.highestStock[0] = 0 },
		},
		{
			name:      "closing market carries no usable price",
			breakSpec: func(s *rowSpec) { s.priceInvalid = true },
		},
		{
			name:      "closing market traded almost nothing",
			breakSpec: func(s *rowSpec) { s.volume = [2]int64{1, 1} },
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			closing := chaosDivineSpec()
			tt.breakSpec(&closing)

			got := crossQuoteCandidates([]Row{cardChaosSpec().row(), cardDivineSpec().row(), closing.row()}, DefaultConfig())
			if len(got) != 0 {
				t.Errorf("got %v, want no routes", candidateKeys(got))
			}
		})
	}
}

func TestCrossQuoteCandidates_secondRowForTheSamePair_isIgnored(t *testing.T) {
	// One hour holds one row per market. If storage ever hands over two, the
	// first is the one that counts — silently mixing the two would price
	// different legs of the same route off different rows.
	second := chaosDivineSpec()
	second.lowestRatio = [2]int64{1000, 1}
	second.highestRatio = [2]int64{2000, 1}

	got := crossQuoteCandidates(append(triangle(), second.row()), DefaultConfig())

	c := candidateByKey(t, got, oneHopKey(cardID, chaosID, divineID))
	if c.legs[2].obs.high != 201 {
		t.Errorf("closing leg price = %v, want the first row's 201", c.legs[2].obs.high)
	}
}

func TestCrossQuoteCandidates_rowThatNamesNoRealPair_isNotACounterparty(t *testing.T) {
	// A market of an item against itself, or against nothing, would otherwise
	// make an item its own counterparty and produce a route that buys and sells
	// the same id.
	tests := []struct {
		name  string
		itemA string
		itemB string
	}{
		{name: "both sides are the same id", itemA: cardID, itemB: cardID},
		{name: "ItemA is empty", itemA: "", itemB: cardID},
		{name: "ItemB is empty", itemA: cardID, itemB: ""},
	}

	want := crossQuoteCandidates(triangle(), DefaultConfig())
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := chaosDivineSpec()
			spec.itemA = tt.itemA
			spec.itemB = tt.itemB

			got := crossQuoteCandidates(append(triangle(), spec.row()), DefaultConfig())
			if !reflect.DeepEqual(got, want) {
				t.Errorf("keys = %v, want the untouched triangle's %v", candidateKeys(got), candidateKeys(want))
			}
		})
	}
}

func TestCrossQuoteCandidates_shuffledRows_produceTheIdenticalOutput(t *testing.T) {
	// Rows arrive from the database in market-id order, but the unit indexes
	// them through maps, so nothing about the output may depend on the order
	// they came in.
	want := crossQuoteCandidates(triangle(), DefaultConfig())
	if len(want) != 2 {
		t.Fatalf("got %d routes from the reference order, want 2", len(want))
	}

	orders := [][3]int{{0, 2, 1}, {1, 0, 2}, {1, 2, 0}, {2, 0, 1}, {2, 1, 0}}
	for _, order := range orders {
		base := triangle()
		shuffled := []Row{base[order[0]], base[order[1]], base[order[2]]}

		got := crossQuoteCandidates(shuffled, DefaultConfig())
		if !reflect.DeepEqual(got, want) {
			t.Errorf("order %v produced %v, want %v", order, candidateKeys(got), candidateKeys(want))
		}
	}
}
