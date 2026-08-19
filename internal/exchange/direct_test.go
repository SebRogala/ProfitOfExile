package exchange

import (
	"reflect"
	"testing"
)

func TestDirectCandidates_chaosDivineRow_quotesTheFlipInDivine(t *testing.T) {
	got := directCandidates([]Row{chaosDivineSpec().row()}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	// Divine leads the default quote priority, so the market reads as "chaos
	// priced in divine": both legs trade the chaos side, and the volume and
	// stock quoted are chaos's.
	wantLegs := []Leg{
		{Action: "buy", Item: chaosID, Quote: divineID, Price: 1.0 / 201.0, Volume: 13001051, Stock: 4564191},
		{Action: "sell", Item: chaosID, Quote: divineID, Price: 1.0 / 196.0, Volume: 13001051, Stock: 4564191},
	}
	if !reflect.DeepEqual(got[0].legs, wantLegs) {
		t.Errorf("legs = %+v, want %+v", got[0].legs, wantLegs)
	}
	if got[0].mode != ModeDirect {
		t.Errorf("mode = %q, want %q", got[0].mode, ModeDirect)
	}
	wantClose(t, "edge", got[0].edge, 201.0/196.0-1)
}

func TestDirectCandidates_chaosPreferredAsQuote_pricesTheFlipInChaos(t *testing.T) {
	cfg := DefaultConfig()
	cfg.QuotePriority = []string{ChaosID, DivineID}

	got := directCandidates([]Row{chaosDivineSpec().row()}, cfg)

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	// The same market, read the other way round: divine is the traded item, so
	// the prices are the feed's own 196 and 201 and the depth is divine's.
	wantLegs := []Leg{
		{Action: "buy", Item: divineID, Quote: chaosID, Price: 196, Volume: 65361, Stock: 8878},
		{Action: "sell", Item: divineID, Quote: chaosID, Price: 201, Volume: 65361, Stock: 8878},
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
	// The default floor is ten traded units per hour; the item side of this
	// market is chaos, because divine leads the quote priority.
	tests := []struct {
		name      string
		breakSpec func(s *rowSpec)
	}{
		{
			name:      "traded item volume one unit below the floor",
			breakSpec: func(s *rowSpec) { s.volume[0] = 9 },
		},
		{
			name:      "nothing traded on the item side",
			breakSpec: func(s *rowSpec) { s.volume[0] = 0 },
		},
		{
			name:      "no stock on the item side",
			breakSpec: func(s *rowSpec) { s.highestStock[0] = 0 },
		},
		{
			name:      "no stock on the quote side",
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

func TestDirectCandidates_itemVolumeExactlyAtTheFloor_keepsTheCandidate(t *testing.T) {
	spec := chaosDivineSpec()
	spec.volume[0] = 10
	cfg := DefaultConfig()
	cfg.MinVolumePerHour = 10

	got := directCandidates([]Row{spec.row()}, cfg)

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1: the floor is inclusive", len(got))
	}
	if got[0].legs[0].Volume != 10 {
		t.Errorf("leg volume = %v, want 10", got[0].legs[0].Volume)
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
	if got[0].legs[0].Item != chaosID {
		t.Errorf("traded item = %s, want %s", got[0].legs[0].Item, chaosID)
	}
}
