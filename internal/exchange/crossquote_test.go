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
//
// Each price carries the integer pair the feed posted it as, oriented to the
// leg: the card quoted in chaos posts "1 card for 10 chaos", and the same card
// quoted in divine posts "20 cards for 1 divine" — a route that read the pair
// off the row's A/B order instead of off the leg would render the second one
// upside down.
//
// The stock is oriented to the leg as well, and unlike the pair it depends on
// the ACTION: a buy leg executes against the item side of the book and a sell
// leg against the quote side, so the same market observed by the two routes
// reports two different numbers.
func cardInChaos(action string) candidateLeg {
	return candidateLeg{
		action: action, item: cardID, quote: chaosID,
		obs: hourChannels(obs{
			low:  pricePoint{price: 10, itemQty: 1, quoteQty: 10},
			high: pricePoint{price: 12, itemQty: 1, quoteQty: 12},
			vwap: 5000.0 / 300.0, vwapOK: true,
			tick:        1.0 / 10.0,
			quoteVolume: 5000, volume: 300, stock: executedStock(action, 40, 900),
		}),
	}
}

func cardInDivine(action string) candidateLeg {
	return candidateLeg{
		action: action, item: cardID, quote: divineID,
		obs: hourChannels(obs{
			low:  pricePoint{price: 1.0 / 20.0, itemQty: 20, quoteQty: 1},
			high: pricePoint{price: 1.0 / 16.0, itemQty: 16, quoteQty: 1},
			vwap: 80.0 / 250.0, vwapOK: true,
			tick:        1.0 / 16.0,
			quoteVolume: 80, volume: 250, stock: executedStock(action, 35, 70),
		}),
	}
}

func divineInChaos(action string) candidateLeg {
	return candidateLeg{
		action: action, item: divineID, quote: chaosID,
		obs: hourChannels(obs{
			low:  pricePoint{price: 196, itemQty: 1, quoteQty: 196},
			high: pricePoint{price: 201, itemQty: 1, quoteQty: 201},
			vwap: 13001051.0 / 65361.0, vwapOK: true,
			tick:        1.0 / 196.0,
			quoteVolume: 13001051, volume: 65361, stock: executedStock(action, 8878, 4564191),
		}),
	}
}

func chaosInDivine(action string) candidateLeg {
	return candidateLeg{
		action: action, item: chaosID, quote: divineID,
		obs: hourChannels(obs{
			low:  pricePoint{price: 1.0 / 201.0, itemQty: 201, quoteQty: 1},
			high: pricePoint{price: 1.0 / 196.0, itemQty: 196, quoteQty: 1},
			vwap: 65361.0 / 13001051.0, vwapOK: true,
			tick:        1.0 / 196.0,
			quoteVolume: 65361, volume: 13001051, stock: executedStock(action, 4564191, 8878),
		}),
	}
}

// executedStock picks the book side an action executes against, so the leg
// fixtures above spell the market's two stock numbers once and let the action
// choose — the same choice gatedLeg makes.
func executedStock(action string, itemSide, quoteSide int64) int64 {
	if action == "sell" {
		return quoteSide
	}
	return itemSide
}

func TestCrossQuoteCandidates_cardBoughtInChaos_observesTheThreeMarketsItWalks(t *testing.T) {
	got := crossQuoteCandidates(triangle(), windowView{}, DefaultConfig())

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
	got := crossQuoteCandidates(triangle(), windowView{}, DefaultConfig())

	c := candidateByKey(t, got, oneHopKey(cardID, chaosID, divineID))
	// Ten chaos buys a card, the card sells for a sixteenth of a divine, and
	// the divine sells back for 201 chaos.
	wantClose(t, "edge", c.edge, (1.0/16.0)*201.0/10.0-1)
}

func TestCrossQuoteCandidates_cardBoughtInDivine_observesTheThreeMarketsItWalks(t *testing.T) {
	got := crossQuoteCandidates(triangle(), windowView{}, DefaultConfig())

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
	got := crossQuoteCandidates(triangle(), windowView{}, DefaultConfig())

	c := candidateByKey(t, got, oneHopKey(cardID, divineID, chaosID))
	wantClose(t, "edge", c.edge, 12.0*(1.0/196.0)/(1.0/20.0)-1)
}

func TestCrossQuoteCandidates_triangle_tradesOnlyTheItemThatIsNotAQuoteCurrency(t *testing.T) {
	got := crossQuoteCandidates(triangle(), windowView{}, DefaultConfig())

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
			if got := crossQuoteCandidates(tt.rows, windowView{}, tt.cfg); len(got) != 0 {
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

	got := crossQuoteCandidates(triangle(), windowView{}, cfg)

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
			if got := crossQuoteCandidates(tt.rows, windowView{}, DefaultConfig()); len(got) != 0 {
				t.Errorf("got %v, want no routes", candidateKeys(got))
			}
		})
	}
}

func TestCrossQuoteCandidates_untradedLegOnOneMarket_dropsOnlyTheRoutesThatTradeIt(t *testing.T) {
	// No card changed hands against divine this hour, so that leg fails the
	// liveness floor. Both card routes have to trade a card on that market and
	// die with it; the scarab routes, which are quoted against the same two
	// currencies and never touch it, survive untouched.
	dead := cardDivineSpec()
	dead.volume[1] = 0

	rows := []Row{
		cardChaosSpec().row(), dead.row(), chaosDivineSpec().row(),
		scarabChaosSpec().row(), scarabDivineSpec().row(),
	}

	got := crossQuoteCandidates(rows, windowView{}, DefaultConfig())

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
			name:      "closing market carries no usable price",
			breakSpec: func(s *rowSpec) { s.priceInvalid = true },
		},
		{
			name:      "closing market traded nothing",
			breakSpec: func(s *rowSpec) { s.volume = [2]int64{0, 0} },
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			closing := chaosDivineSpec()
			tt.breakSpec(&closing)

			got := crossQuoteCandidates([]Row{cardChaosSpec().row(), cardDivineSpec().row(), closing.row()}, windowView{}, DefaultConfig())
			if len(got) != 0 {
				t.Errorf("got %v, want no routes", candidateKeys(got))
			}
		})
	}
}

func TestCrossQuoteCandidates_closingMarketWithNoStockOfTheCurrencyBeingSold_dropsOnlyThatDirection(t *testing.T) {
	// The closing leg SELLS one currency for the other, so what it needs on the
	// book is stock of the currency it is paid in — the quote side. The chaos
	// side of the chaos/divine market standing empty therefore kills only the
	// route whose last leg is paid in chaos, and leaves its mirror, whose last
	// leg sells chaos and is paid in divine, executable.
	closing := chaosDivineSpec()
	closing.highestStock[0] = 0

	got := crossQuoteCandidates([]Row{cardChaosSpec().row(), cardDivineSpec().row(), closing.row()}, windowView{}, DefaultConfig())

	want := []string{oneHopKey(cardID, divineID, chaosID)}
	if !reflect.DeepEqual(candidateKeys(got), want) {
		t.Errorf("keys = %v, want %v", candidateKeys(got), want)
	}
}

func TestCrossQuoteCandidates_closingMarketWithNoStockOfTheCurrencyBeingBought_dropsTheMirrorDirection(t *testing.T) {
	// The other half of the same rule, so neither direction can be passing by
	// accident: with the divine side of the closing market empty, the route paid
	// in divine dies and the one paid in chaos survives.
	closing := chaosDivineSpec()
	closing.highestStock[1] = 0

	got := crossQuoteCandidates([]Row{cardChaosSpec().row(), cardDivineSpec().row(), closing.row()}, windowView{}, DefaultConfig())

	want := []string{oneHopKey(cardID, chaosID, divineID)}
	if !reflect.DeepEqual(candidateKeys(got), want) {
		t.Errorf("keys = %v, want %v", candidateKeys(got), want)
	}
}

func TestCrossQuoteCandidates_sellLegIntoAMarketWithNoAsksStanding_isGatedAndMarkedDepleted(t *testing.T) {
	// The Journey Tattoo shape (2026-08-23), in miniature: the card's chaos
	// market carries bids and no asks — nobody offering a card, 900 chaos
	// standing behind the ones that are wanted. That is the one-sided book a
	// SELLER wants, and the gate used to drop it for the side the sell leg was
	// never going to trade on, which took the whole recipe with it.
	//
	// The route that BUYS the card in divine and sells it into those bids is
	// therefore constructed, and its sell leg reports the executable depth (900
	// chaos) with depletedSide marking the empty other half.
	oneSided := cardChaosSpec()
	oneSided.highestStock[1] = 0

	got := crossQuoteCandidates([]Row{oneSided.row(), cardDivineSpec().row(), chaosDivineSpec().row()}, windowView{}, DefaultConfig())

	c := candidateByKey(t, got, oneHopKey(cardID, divineID, chaosID))
	sell := c.legs[1]
	if sell.action != "sell" || sell.quote != chaosID {
		t.Fatalf("leg 1 = %s %s in %s, want the sell into chaos", sell.action, sell.item, sell.quote)
	}
	if sell.obs.stock != 900 {
		t.Errorf("sell leg stock = %d, want the 900 chaos it executes against", sell.obs.stock)
	}
	if !sell.obs.depletedSide {
		t.Errorf("sell leg depletedSide = false, want true — no card was on offer this hour")
	}
}

func TestCrossQuoteCandidates_buyLegOnTheSameOneSidedMarket_isStillDropped(t *testing.T) {
	// The mirror of the case above, and the boundary that says the gate followed
	// the action rather than being deleted: the route that would BUY the card in
	// chaos needs a card on offer, there is none, and no depth on the chaos side
	// substitutes for it.
	oneSided := cardChaosSpec()
	oneSided.highestStock[1] = 0

	got := crossQuoteCandidates([]Row{oneSided.row(), cardDivineSpec().row(), chaosDivineSpec().row()}, windowView{}, DefaultConfig())

	if key := oneHopKey(cardID, chaosID, divineID); indexOf(candidateKeys(got), key) >= 0 {
		t.Errorf("keys = %v, want no route buying the card off an empty ask side", candidateKeys(got))
	}
}

func TestCrossQuoteCandidates_secondRowForTheSamePair_isIgnored(t *testing.T) {
	// One hour holds one row per market. If storage ever hands over two, the
	// first is the one that counts — silently mixing the two would price
	// different legs of the same route off different rows.
	second := chaosDivineSpec()
	second.lowestRatio = [2]int64{1000, 1}
	second.highestRatio = [2]int64{2000, 1}

	got := crossQuoteCandidates(append(triangle(), second.row()), windowView{}, DefaultConfig())

	c := candidateByKey(t, got, oneHopKey(cardID, chaosID, divineID))
	if c.legs[2].obs.high.price != 201 {
		t.Errorf("closing leg price = %v, want the first row's 201", c.legs[2].obs.high.price)
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

	want := crossQuoteCandidates(triangle(), windowView{}, DefaultConfig())
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := chaosDivineSpec()
			spec.itemA = tt.itemA
			spec.itemB = tt.itemB

			got := crossQuoteCandidates(append(triangle(), spec.row()), windowView{}, DefaultConfig())
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
	want := crossQuoteCandidates(triangle(), windowView{}, DefaultConfig())
	if len(want) != 2 {
		t.Fatalf("got %d routes from the reference order, want 2", len(want))
	}

	orders := [][3]int{{0, 2, 1}, {1, 0, 2}, {1, 2, 0}, {2, 0, 1}, {2, 1, 0}}
	for _, order := range orders {
		base := triangle()
		shuffled := []Row{base[order[0]], base[order[1]], base[order[2]]}

		got := crossQuoteCandidates(shuffled, windowView{}, DefaultConfig())
		if !reflect.DeepEqual(got, want) {
			t.Errorf("order %v produced %v, want %v", order, candidateKeys(got), candidateKeys(want))
		}
	}
}

func TestCrossQuoteCandidates_untradedClosingMarket_isCarriedByItsOwnWindow(t *testing.T) {
	// POE-252 reaches a triangle one LEG at a time. The chaos/divine market that
	// closes every route here published a row this hour and nobody traded in it,
	// so leg 2 has no liveness of its own and the hours behind that market carry
	// it. The identical row against an empty window empties the hour —
	// TestCrossQuoteCandidates_unusableClosingMarket_dropsEveryRouteThroughIt is
	// that case — so what is read here is the window and not some other survival.
	untraded := chaosDivineSpec()
	untraded.volume = [2]int64{0, 0}
	rows := []Row{cardChaosSpec().row(), cardDivineSpec().row(), untraded.row()}
	window := []StoredRow{
		storedBack(0, untraded),
		storedBack(1, chaosDivineSpec()),
		storedBack(2, chaosDivineSpec()),
	}

	got := crossQuoteCandidates(rows, viewAt(feedHour, window), DefaultConfig())

	c := candidateByKey(t, got, oneHopKey(cardID, chaosID, divineID))
	if !c.legs[2].obs.rescued {
		t.Errorf("closing leg rescued = false, want the window to have carried the leg whose own hour traded nothing")
	}
	// Its two siblings traded in the scored hour and must be untouched: a rescue
	// is per MARKET, so one silent leg prices one leg's window and no more.
	if c.legs[0].obs.rescued || c.legs[1].obs.rescued {
		t.Errorf("legs 0/1 rescued = %v/%v, want both false — each read its own live hour", c.legs[0].obs.rescued, c.legs[1].obs.rescued)
	}
}
