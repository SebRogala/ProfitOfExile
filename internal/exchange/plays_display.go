package exchange

import (
	"encoding/json"
	"math"
)

// ScaleTargetChaos is the chaos a play is scaled up to before it is worth the
// reader's attention. It mirrors SCALE_TARGET_CHAOS in
// desktop/src/lib/exchange/view.ts, the owner constant of POE-192.
const ScaleTargetChaos = 100

// DisplayBasis is how a row's displayed quantity was chosen.
type DisplayBasis string

const (
	// BasisPosting counts one minimal order of the buy market.
	BasisPosting DisplayBasis = "posting"
	// BasisSingle counts one item, because the buy market posted no usable pair.
	BasisSingle DisplayBasis = "single"
)

// DisplayNumber is a float64 that encodes a non-finite value as JSON null and
// decodes null as NaN. JSON has no NaN or ±Inf, and a client sorts a null key
// last (ADR-030 exchange API contract).
type DisplayNumber float64

// MarshalJSON writes null for NaN and ±Inf.
func (n DisplayNumber) MarshalJSON() ([]byte, error) {
	f := float64(n)
	if math.IsNaN(f) || math.IsInf(f, 0) {
		return []byte("null"), nil
	}
	return json.Marshal(f)
}

// UnmarshalJSON reads null as NaN.
func (n *DisplayNumber) UnmarshalJSON(raw []byte) error {
	if string(raw) == "null" {
		*n = DisplayNumber(math.NaN())
		return nil
	}
	var f float64
	if err := json.Unmarshal(raw, &f); err != nil {
		return err
	}
	*n = DisplayNumber(f)
	return nil
}

// PlayMoney is a row's three money columns at its display scale.
type PlayMoney struct {
	Investment  DisplayNumber `json:"investment"`
	Roi         DisplayNumber `json:"roi"`
	ExpectedRoi DisplayNumber `json:"expectedRoi"`
}

// PlayScale is a play scaled to ScaleTargetChaos: the Scale column.
type PlayScale struct {
	Flips      int64         `json:"flips"`
	Gain       DisplayNumber `json:"gain"`
	Investment DisplayNumber `json:"investment"`
	// Hours is nil when the play's depth is not a positive finite number.
	Hours *int64 `json:"hours"`
}

// PlaySortKeys are the keys the three client sorts order by; NaN is a missing
// key and sorts last.
type PlaySortKeys struct {
	Expected     DisplayNumber `json:"expected"`
	Roi          DisplayNumber `json:"roi"`
	FastestHours DisplayNumber `json:"fastestHours"`
}

// PlayDisplay is the display object of one served play: the whole output of the
// desktop helpers displayScale, moneyColumns, printsExpectedRoi,
// worthwhileScale, runInvestment and the sortPlays keys (view.ts), ported one
// for one so the server can filter, sort and page on them (ADR-030).
type PlayDisplay struct {
	Units           int64         `json:"units"`
	Basis           DisplayBasis  `json:"basis"`
	Money           PlayMoney     `json:"money"`
	ExpectedPrinted bool          `json:"expectedPrinted"`
	Scale           *PlayScale    `json:"scale"`
	RunInvestment   DisplayNumber `json:"runInvestment"`
	SortKeys        PlaySortKeys  `json:"sortKeys"`
}

// Decorate returns a copy of play with its Display filled.
func Decorate(play Play) Play {
	units, basis := int64(1), BasisSingle
	if len(play.Legs) > 0 && play.Legs[0].PriceItemQty > 0 && play.Legs[0].PriceQuoteQty > 0 {
		units, basis = play.Legs[0].PriceItemQty, BasisPosting
	}
	u := float64(units)
	money := PlayMoney{
		Investment:  DisplayNumber(play.Investment * u),
		Roi:         DisplayNumber(play.Roi * u),
		ExpectedRoi: DisplayNumber(play.ExpectedRoi * u),
	}
	printed := play.SimEntries != 0
	scale := worthwhileScale(play)

	d := PlayDisplay{
		Units:           units,
		Basis:           basis,
		Money:           money,
		ExpectedPrinted: printed,
		Scale:           scale,
		RunInvestment:   DisplayNumber(play.Investment),
		SortKeys: PlaySortKeys{
			Expected:     DisplayNumber(math.NaN()),
			Roi:          money.Roi,
			FastestHours: DisplayNumber(math.NaN()),
		},
	}
	if printed {
		d.SortKeys.Expected = money.ExpectedRoi
	}
	if scale != nil {
		d.RunInvestment = scale.Investment
		if scale.Hours != nil {
			d.SortKeys.FastestHours = DisplayNumber(*scale.Hours)
		}
	}
	play.Display = &d
	return play
}

// worthwhileScale ports view.ts worthwhileScale: nil when the expectation is
// not a positive finite number.
func worthwhileScale(play Play) *PlayScale {
	if math.IsNaN(play.ExpectedRoi) || math.IsInf(play.ExpectedRoi, 0) || play.ExpectedRoi <= 0 {
		return nil
	}
	flips := math.Ceil(ScaleTargetChaos / play.ExpectedRoi)
	scale := &PlayScale{
		Flips:      int64(flips),
		Gain:       DisplayNumber(play.ExpectedRoi * flips),
		Investment: DisplayNumber(play.Investment * flips),
	}
	if !math.IsNaN(play.Depth) && !math.IsInf(play.Depth, 0) && play.Depth > 0 {
		hours := int64(math.Ceil(flips / play.Depth))
		scale.Hours = &hours
	}
	return scale
}
