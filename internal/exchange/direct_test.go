package exchange

import (
	"reflect"
	"testing"
)

func TestDirectCandidates_chaosDivineRow_observesBothLegsQuotedInDivine(t *testing.T) {
	got := directCandidates([]Row{chaosDivineSpec().row()}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	// Divine leads the default quote priority, so the market reads as "chaos
	// priced in divine": both legs trade the chaos side, the traded volume is
	// chaos's, and the quote volume is divine's. Both legs observe the same hour
	// of the same market and differ only in which end of the spread they execute
	// on and which side of the book they take it from — both read from action.
	//
	// Each price arrives with the integer pair the feed posted it as, oriented
	// to the LEG: chaos is the item here, so the cheapest chaos is 201 of them
	// for 1 divine — the transpose of what the row's ItemA/ItemB order stores.
	hour := func(stock int64) obs {
		return obs{
			low:         pricePoint{price: 1.0 / 201.0, itemQty: 201, quoteQty: 1},
			high:        pricePoint{price: 1.0 / 196.0, itemQty: 196, quoteQty: 1},
			vwap:        65361.0 / 13001051.0,
			vwapOK:      true,
			tick:        1.0 / 196.0,
			quoteVolume: 65361,
			volume:      13001051,
			stock:       stock,
		}
	}
	// The buy takes chaos off the book (4,564,191 of it), the sell hands chaos
	// over for divine (8,878 of it) — the two sides of the one market, each
	// named by the leg that executes against it.
	wantLegs := []candidateLeg{
		{action: "buy", item: chaosID, quote: divineID, obs: hour(4564191)},
		{action: "sell", item: chaosID, quote: divineID, obs: hour(8878)},
	}
	if !reflect.DeepEqual(got[0].legs, wantLegs) {
		t.Errorf("legs = %+v, want %+v", got[0].legs, wantLegs)
	}
	if got[0].mode != ModeDirect {
		t.Errorf("mode = %q, want %q", got[0].mode, ModeDirect)
	}
}

func TestDirectCandidates_chaosPreferredAsQuote_observesBothLegsQuotedInChaos(t *testing.T) {
	cfg := DefaultConfig()
	cfg.QuotePriority = []string{ChaosID, DivineID}

	got := directCandidates([]Row{chaosDivineSpec().row()}, cfg)

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	// The same market, read the other way round: divine is the traded item, so
	// the prices are the feed's own 196 and 201, the depth is divine's, and the
	// volume-weighted price is the 198.97 chaos a divine the hour actually
	// cleared at. The tick is a property of the quantity pairs, so it does not
	// depend on which side is read as the quote.
	//
	// The posted pairs transpose with the orientation: the same market that
	// reads as "201 chaos for 1 divine" under the default priority reads as
	// "1 divine for 196 chaos" here. Nothing inverted a float — the other stored
	// quantity became the item side. The stocks transpose with it, each leg
	// still naming the side it executes against.
	hour := func(stock int64) obs {
		return obs{
			low:         pricePoint{price: 196, itemQty: 1, quoteQty: 196},
			high:        pricePoint{price: 201, itemQty: 1, quoteQty: 201},
			vwap:        13001051.0 / 65361.0,
			vwapOK:      true,
			tick:        1.0 / 196.0,
			quoteVolume: 13001051,
			volume:      65361,
			stock:       stock,
		}
	}
	wantLegs := []candidateLeg{
		{action: "buy", item: divineID, quote: chaosID, obs: hour(8878)},
		{action: "sell", item: divineID, quote: chaosID, obs: hour(4564191)},
	}
	if !reflect.DeepEqual(got[0].legs, wantLegs) {
		t.Errorf("legs = %+v, want %+v", got[0].legs, wantLegs)
	}
}

func TestDirectCandidates_edgeIsTheSameWhicheverSideIsTheQuote(t *testing.T) {
	row := chaosDivineSpec().row()
	chaosQuoted := DefaultConfig()
	chaosQuoted.QuotePriority = []string{ChaosID, DivineID}

	inDivine := directCandidates([]Row{row}, DefaultConfig())
	inChaos := directCandidates([]Row{row}, chaosQuoted)

	if len(inDivine) != 1 || len(inChaos) != 1 {
		t.Fatalf("got %d and %d candidates, want 1 each", len(inDivine), len(inChaos))
	}
	// Inverting both prices leaves their ratio alone: orientation is a
	// presentation choice, not an arithmetic one.
	wantClose(t, "edge quoted in divine", inDivine[0].edge, 201.0/196.0-1)
	wantClose(t, "edge quoted in chaos", inChaos[0].edge, inDivine[0].edge)
}

func TestDirectCandidates_key_namesTheMarketBehindTheDirectPrefix(t *testing.T) {
	spec := chaosDivineSpec()
	spec.itemA = cardID
	spec.itemB = chaosID

	got := directCandidates([]Row{spec.row()}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	if want := "direct:" + cardID + "|" + chaosID; got[0].key != want {
		t.Errorf("key = %q, want %q", got[0].key, want)
	}
}

func TestDirectCandidates_rowFailingAGate_producesNoCandidate(t *testing.T) {
	// The default floor is ONE traded unit per hour (POE-193): liveness asks
	// whether a trade happened, so the only volume that fails it is none at all.
	// The item side of this market is chaos, because divine leads the quote
	// priority.
	tests := []struct {
		name      string
		breakSpec func(s *rowSpec)
	}{
		{
			name:      "nothing traded on the item side",
			breakSpec: func(s *rowSpec) { s.volume[0] = 0 },
		},
		{
			// The buy leg executes against the item side, so an empty one
			// leaves it nothing to take and the flip dies with it.
			name:      "no stock on the item side, which the buy leg executes against",
			breakSpec: func(s *rowSpec) { s.highestStock[0] = 0 },
		},
		{
			// And the sell leg executes against the quote side. Gating the two
			// legs separately is what keeps a FLIP demanding both sides after
			// the gate started following the action (2026-08-23): neither leg
			// asks for the other's side, and the pair of them asks for each.
			name:      "no stock on the quote side, which the sell leg executes against",
			breakSpec: func(s *rowSpec) { s.highestStock[1] = 0 },
		},
		{
			name:      "row carries no usable price",
			breakSpec: func(s *rowSpec) { s.priceInvalid = true },
		},
		{
			name: "highest ratio prices the item below the lowest",
			breakSpec: func(s *rowSpec) {
				s.lowestRatio = [2]int64{201, 1}
				s.highestRatio = [2]int64{196, 1}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := chaosDivineSpec()
			tt.breakSpec(&spec)

			if got := directCandidates([]Row{spec.row()}, DefaultConfig()); len(got) != 0 {
				t.Errorf("got %d candidates, want none: %+v", len(got), got)
			}
		})
	}
}

func TestDirectCandidates_aSingleTradedUnit_keepsTheCandidateAtDefaults(t *testing.T) {
	// The shipped floor's inclusive boundary. One unit changing hands is the
	// weakest true statement about a leg, and POE-193 made it the whole default
	// demand: a market that traded once this hour is a market, and how many units
	// are worth the reader's time is the reader's call.
	spec := chaosDivineSpec()
	spec.volume[0] = 1

	got := directCandidates([]Row{spec.row()}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1: one traded unit clears the default floor", len(got))
	}
	if got[0].legs[0].obs.volume != 1 {
		t.Errorf("leg volume = %v, want 1", got[0].legs[0].obs.volume)
	}
}

func TestDirectCandidates_itemVolumeExactlyAtAnArmedFloor_keepsTheCandidate(t *testing.T) {
	// The knob's inclusive boundary. Ten units an hour is what the engine
	// enforced until POE-193 and what EXCHANGE_MIN_VOLUME_PER_HOUR is now FOR, so
	// the level is armed here rather than assumed.
	spec := chaosDivineSpec()
	spec.volume[0] = 10
	cfg := DefaultConfig()
	cfg.MinVolumePerHour = 10

	got := directCandidates([]Row{spec.row()}, cfg)

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1: the floor is inclusive", len(got))
	}
	if got[0].legs[0].obs.volume != 10 {
		t.Errorf("leg volume = %v, want 10", got[0].legs[0].obs.volume)
	}
}

func TestDirectCandidates_itemVolumeOneUnitUnderAnArmedFloor_producesNoCandidate(t *testing.T) {
	// The other side of the armed boundary: the same nine units the default now
	// serves are dropped once a reader types the old level in.
	spec := chaosDivineSpec()
	spec.volume[0] = 9
	cfg := DefaultConfig()
	cfg.MinVolumePerHour = 10

	if got := directCandidates([]Row{spec.row()}, cfg); len(got) != 0 {
		t.Errorf("got %d candidates, want none at an armed floor of 10: %+v", len(got), got)
	}
}

func TestDirectCandidates_untradedQuoteSide_stillProducesACandidate(t *testing.T) {
	// Only the traded item side is gated; the quote side's volume is not
	// consulted.
	spec := chaosDivineSpec()
	spec.volume[1] = 0

	got := directCandidates([]Row{spec.row()}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	if got[0].legs[0].item != chaosID {
		t.Errorf("traded item = %s, want %s", got[0].legs[0].item, chaosID)
	}
}

func TestDirectCandidates_servedFlip_neverMarksALegDepleted(t *testing.T) {
	// A flip's two legs execute against opposite sides of one market, so each
	// leg's "other side" is the side the other leg already had to find stock on.
	// A candidate that got this far therefore cannot carry the mark, and the
	// property is asserted rather than assumed because the flag is set in the
	// shared gatedLeg and would otherwise be one wrong comparison away from
	// appearing on every flip.
	got := directCandidates([]Row{chaosDivineSpec().row()}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	for i, leg := range got[0].legs {
		if leg.obs.depletedSide {
			t.Errorf("leg %d (%s) depletedSide = true, want false — a served flip has stock on both sides", i, leg.action)
		}
	}
}

func TestDirectCandidates_untradedQuoteSide_marksTheHourAsCarryingNoFairPrice(t *testing.T) {
	// No divine changed hands, so the hour has no volume-weighted price at all.
	// That is a MISSING reading, not a price of zero: vwapOK is what keeps the
	// aggregator from averaging the 0 into the leg's fair anchor.
	spec := chaosDivineSpec()
	spec.volume[1] = 0

	got := directCandidates([]Row{spec.row()}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	if got[0].legs[0].obs.vwapOK {
		t.Errorf("vwapOK = true, want false when the quote side traded nothing")
	}
	if got[0].legs[0].obs.vwap != 0 {
		t.Errorf("vwap = %v, want 0 beside vwapOK false", got[0].legs[0].obs.vwap)
	}
}
