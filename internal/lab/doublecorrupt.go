package lab

import (
	"fmt"
	"sort"
	"strconv"
	"strings"
	"time"
)

// Double-corruption calculator (POE-125): expected value of handing a gem to
// the Doryani's Institute altar (temple "gem" line, tier 3), which corrupts it
// twice. Outcomes are priced from the corrupted rows the collector already
// stores — 21/20c, 20/23c, 21/23c and the Vaal market identities are collected
// today; the calculator is a probability model over cells the market prices,
// not new collection work.

// DoubleCorruptVariants are the uncorrupted input variants the calculator
// models. v1 models the 20/20 input only — the task's own framing ("a 20/20
// gem + the altar"). Each variant is a separate market with its own input cost
// and its own outcome distribution; they are never merged into one EV
// (per-variant rule). Adding an input variant here is a data change, not a
// rewrite: the whole analysis loops over this list.
var DoubleCorruptVariants = []string{"20/20"}

// DefaultDoubleCorruptVariant is the input variant served when a caller names
// none.
const DefaultDoubleCorruptVariant = "20/20"

// DoubleCorruptModelEstimated is the model marker every result carries. The
// outcome weights below are sourced from community documentation, not from GGG
// — the UI must badge EV numbers as estimated rather than present them as
// confirmed odds. If the model is ever confirmed or corrected, introduce a new
// marker value rather than silently upgrading this one.
const DoubleCorruptModelEstimated = "estimated"

// DefaultTempleOverheadChaos is the flat chaos cost attributed to reaching the
// Doryani's Institute room. The room is treated as sunk by default (0): the
// temple builder scores reaching the gem line as a pathing decision, not a
// chaos price, and inventing a number here would fabricate a cost. The
// parameter exists so the task's "room cost" input is plumbed and a real
// amortized figure can be supplied later without an API change.
const DefaultTempleOverheadChaos = 0.0

// THE OUTCOME MODEL — single source of truth for every probability the
// calculator uses. Nothing else in the codebase may hard-code an outcome
// weight; a correction to the model is an edit to this block only.
//
// Per-roll outcomes of one corruption:
//
//	no-change       25%
//	level +1        12.5%  (capped at 21)
//	level -1        12.5%  (floored at 1)
//	quality reroll  25%    (uniform 0..23, replaces current quality)
//	Vaal transform  25%    (only when a Vaal version of the gem exists;
//	                        keeps level and quality; idempotent — a second
//	                        transform roll changes nothing)
//
// Doryani's Institute applies TWO such rolls, modeled as independent draws
// WITH replacement — the same outcome category can hit twice, and the exact
// "nothing happened, twice" pair (p = 1/16) is possible; the unchanged-gem
// cell as a whole is the modal outcome for gems with no Vaal version.
//
// Sources and open questions (all of which is why every result is marked
// DoubleCorruptModelEstimated):
//
//   - maxroll.gg/poe/resources/corruption: four outcome categories "weighted
//     equally", level ±1 capped at 21, and the Institute "corrupts the gem
//     twice successively, with exactly the same options". It states the quality
//     outcome as "quality ±1-10%, capped at 23%".
//   - poegems.net states the quality outcome differently: "quality is replaced
//     with a uniformly-random value between 0 and 23%". The two sources
//     disagree; the uniform-reroll model is the one implemented here (primary
//     model by decision). Under it a quality landing below 20 maps onto the
//     level-only market cell ("20c") and a landing of exactly 23 onto the /23
//     cell; landings 20-22 are approximated by the /20 cell, since only /20 and
//     /23 shapes are reliably priced.
//   - Within "level ±1" the up/down split is not given by either source; an
//     even 12.5/12.5 split is assumed.
//   - Independent-with-replacement across the two rolls is NOT GGG-confirmed.
//     One lower-quality source claims the same outcome cannot hit twice
//     (without replacement), which would change the distribution materially.
//   - When no Vaal version exists, this model folds the transform's 25% into
//     no-change. That redistribution is assumed, not sourced — GGG may instead
//     redistribute across all remaining outcomes.
//
// A consistency point in the model's favor: two rolls can never produce a Vaal
// gem at 21/23 (that would take three outcomes), which matches the observed
// market and the game rule recorded at isDedicationFeed (eligibility.go).
const (
	dcWeightNoChange      = 0.25
	dcWeightLevelUp       = 0.125
	dcWeightLevelDown     = 0.125
	dcWeightQualityReroll = 0.25
	dcWeightVaalTransform = 0.25 // folded into no-change when no Vaal version exists

	dcQualityRerollMin = 0
	dcQualityRerollMax = 23
	dcLevelCap         = 21
	dcLevelFloor       = 1
	dcRolls            = 2
)

// DoubleCorruptOutcome is one market-priceable cell of the outcome
// distribution: a (gem name, corrupted variant) pair with the probability mass
// that lands on it and the price the market currently puts on it.
type DoubleCorruptOutcome struct {
	Name        string  `json:"name"`        // market identity ("Arc of X" or "Vaal Arc (Arc of X)")
	Variant     string  `json:"variant"`     // DB-format corrupted variant, e.g. "21/20c"
	Probability float64 `json:"probability"` // mass of the outcome distribution on this cell
	Chaos       float64 `json:"chaos"`       // raw listed price; 0 when unpriced
	Adjusted    float64 `json:"adjusted"`    // risk-adjusted price (sell probability × stability)
	Listings    int     `json:"listings"`
	Priced      bool    `json:"priced"` // false = no usable price; see LowConfidence for the reason
	Thin        bool    `json:"thin"`   // priced but < 5 listings
	// LowConfidence marks a cell the market carries a row for that the
	// confidence gate refused to price: its listing depth is under 40% of the
	// median depth of its own corrupted variant. Priced is false on such a cell
	// and its mass lands in UnpricedProbability — the field exists so the
	// breakdown can say "a row exists but is too thin to believe" rather than
	// leaving it indistinguishable from a cell nobody lists at all.
	LowConfidence bool `json:"lowConfidence,omitempty"`
}

// DoubleCorruptResult holds the computed double-corruption EV for one gem at
// one input variant.
type DoubleCorruptResult struct {
	Time                time.Time              `json:"time"`
	Name                string                 `json:"name"`
	Color               string                 `json:"color"` // RED, GREEN, BLUE, WHITE or "" (informational; no pool partition)
	InputVariant        string                 `json:"inputVariant"`
	InputCost           float64                `json:"inputCost"` // the gem's own uncorrupted price at InputVariant
	TempleOverheadChaos float64                `json:"templeOverheadChaos"`
	HasVaalVersion      bool                   `json:"hasVaalVersion"`
	EV                  float64                `json:"ev"`                  // risk-adjusted expected outcome value
	EVRaw               float64                `json:"evRaw"`               // raw-listed-price expected outcome value
	Profit              float64                `json:"profit"`              // EV - InputCost - TempleOverheadChaos (risk-adjusted, never EVRaw)
	PricedProbability   float64                `json:"pricedProbability"`   // mass of the distribution the market prices
	UnpricedProbability float64                `json:"unpricedProbability"` // mass contributing 0 to EV for lack of a priced row
	ThinOutcomeCells    int                    `json:"thinOutcomeCells"`    // priced cells with < 5 listings
	LiquidityRisk       string                 `json:"liquidityRisk"`       // LOW, MEDIUM, HIGH
	Outcomes            []DoubleCorruptOutcome `json:"outcomes"`
	Model               string                 `json:"model"` // DoubleCorruptModelEstimated
}

// IsDoubleCorruptVariant reports whether variant is one of the analyzed input
// variants.
func IsDoubleCorruptVariant(variant string) bool {
	for _, v := range DoubleCorruptVariants {
		if v == variant {
			return true
		}
	}
	return false
}

// SelectDoubleCorruptByNames indexes results by gem name for the compare path,
// which asks about the two or three gems a Font offered rather than the whole
// corpus. results must already be narrowed to one input variant — the map is
// keyed by name alone, so mixing input variants here would let one market's EV
// answer for another's.
func SelectDoubleCorruptByNames(results []DoubleCorruptResult, names []string) map[string]DoubleCorruptResult {
	if len(results) == 0 || len(names) == 0 {
		return nil
	}
	wanted := make(map[string]struct{}, len(names))
	for _, n := range names {
		wanted[n] = struct{}{}
	}
	out := make(map[string]DoubleCorruptResult, len(names))
	for _, dc := range results {
		if _, ok := wanted[dc.Name]; ok {
			out[dc.Name] = dc
		}
	}
	return out
}

// FilterDoubleCorruptVariant returns the subset of results computed for one
// input variant.
func FilterDoubleCorruptVariant(results []DoubleCorruptResult, inputVariant string) []DoubleCorruptResult {
	out := make([]DoubleCorruptResult, 0, len(results))
	for _, r := range results {
		if r.InputVariant == inputVariant {
			out = append(out, r)
		}
	}
	return out
}

// The double-corrupt input predicate, isDoubleCorruptInput, lives in
// eligibility.go with the rest of the gem-eligibility rules.

// buildVaalIdentityIndex maps a gem's own market name to the market name of its
// Vaal version, read out of the snapshot rather than derived by string surgery
// on the gem's name.
//
// The lookup has to be data-driven because the naming is not a rule. Measured
// against the local gem_snapshots on 2026-08-24, three shapes coexist:
//
//   - regular base gem: "Arc" → "Vaal Arc"
//   - a base gem whose own name contains " of ": "Rain of Arrows" → "Vaal Rain
//     of Arrows" (cutting at the first " of " would ask for "Vaal Rain")
//   - an irregular pair the name cannot produce at all: "Dominating Blow" →
//     "Vaal Domination" (1 of the 46 parenthesised Vaal identities in the
//     snapshot; the other 45 follow the last-" of " shape)
//
// Two entries come out of each parenthesised row "Vaal V (B of Suffix)": the
// transfigured gem "B of Suffix" maps to the full row name, and its base gem
// "B" maps to "Vaal V" — which is how the irregular pair is recovered without
// an exception list that would rot as GGG adds gems.
//
// Market presence is therefore the proxy for existence: a gem whose Vaal
// version nobody lists reads as having none, which folds the transform weight
// into no-change rather than inventing a price. That is an approximation, not a
// game rule.
//
// listedCorrupted is the set of names the corrupted market actually carries a
// row for, and it gates the DERIVED base-gem entry only. The base gem's Vaal
// name is the one entry no row states outright — it is cut out of a
// parenthesised row by string surgery — so without the gate a base gem inherits
// a Vaal identity the market has never listed, and the result reports
// HasVaalVersion for a name it can never price. The direct entries are not
// gated: those names came off a row, which is the market presence the paragraph
// above describes.
func buildVaalIdentityIndex(gems []GemPrice, listedCorrupted map[string]bool) map[string]string {
	index := make(map[string]string)
	for i := range gems {
		name := gems[i].Name
		if !isVaalGemName(name) {
			continue
		}
		open := strings.Index(name, " (")
		if open < 0 || !strings.HasSuffix(name, ")") {
			// "Vaal Arc" — the Vaal identity of the base gem "Arc".
			index[strings.TrimPrefix(name, "Vaal ")] = name
			continue
		}
		vaalBase := name[:open]             // "Vaal Domination"
		inner := name[open+2 : len(name)-1] // "Dominating Blow of Inspiring"
		index[inner] = name
		if cut := strings.LastIndex(inner, " of "); cut >= 0 && listedCorrupted[vaalBase] {
			// Only fill in the base gem's identity if a "Vaal <base>" row has
			// not already stated it directly — the direct row is the stronger
			// source.
			if _, direct := index[inner[:cut]]; !direct {
				index[inner[:cut]] = vaalBase
			}
		}
	}
	return index
}

// dcState is one point of the gem-state space the two rolls walk over.
type dcState struct {
	vaal    bool
	level   int
	quality int
}

// dcCell is a market-priceable aggregation of states: the corrupted variant
// string the exact (level, quality) maps onto, plus whether the gem was
// Vaal-transformed (which changes its market name, not its variant format).
type dcCell struct {
	vaal    bool
	variant string
}

// dcCellVariant maps an exact post-corruption (level, quality) onto the
// variant shape the market prices. Only /20 and /23 quality shapes carry
// reliable prices, so: exactly 23 → the /23 cell, 20-22 → the /20 cell, below
// 20 → the level-only cell ("20c"), which is the nearest lower-quality market
// shape and usually unpriced for transfigured gems (the unpriced path).
func dcCellVariant(level, quality int) string {
	switch {
	case quality >= 23:
		return fmt.Sprintf("%d/23c", level)
	case quality >= 20:
		return fmt.Sprintf("%d/20c", level)
	default:
		return fmt.Sprintf("%dc", level)
	}
}

// doubleCorruptCellDistribution computes the exact outcome-cell distribution
// for an input gem at (level, quality), by applying dcRolls sequential rolls
// of the outcome model and aggregating the resulting states into market cells.
// hasVaal folds the transform weight into no-change when false.
func doubleCorruptCellDistribution(hasVaal bool, level, quality int) map[dcCell]float64 {
	dist := map[dcState]float64{{vaal: false, level: level, quality: quality}: 1.0}

	noChange := dcWeightNoChange
	if !hasVaal {
		// Assumed redistribution — see the model comment above.
		noChange += dcWeightVaalTransform
	}

	for roll := 0; roll < dcRolls; roll++ {
		next := make(map[dcState]float64, len(dist)*4)
		for st, p := range dist {
			next[st] += p * noChange

			up := st
			up.level = min(st.level+1, dcLevelCap)
			next[up] += p * dcWeightLevelUp

			down := st
			down.level = max(st.level-1, dcLevelFloor)
			next[down] += p * dcWeightLevelDown

			span := dcQualityRerollMax - dcQualityRerollMin + 1
			qw := p * dcWeightQualityReroll / float64(span)
			for q := dcQualityRerollMin; q <= dcQualityRerollMax; q++ {
				reroll := st
				reroll.quality = q
				next[reroll] += qw
			}

			if hasVaal {
				vaal := st
				vaal.vaal = true
				next[vaal] += p * dcWeightVaalTransform
			}
		}
		dist = next
	}

	cells := make(map[dcCell]float64)
	for st, p := range dist {
		cells[dcCell{vaal: st.vaal, variant: dcCellVariant(st.level, st.quality)}] += p
	}
	return cells
}

// dcInputLevelQuality parses an uncorrupted input variant ("20/20", or a bare
// "20" for a 0-quality gem) into the starting level and quality of the model
// walk. Parsing is strict on purpose: a corrupted variant ("20/20c") is not a
// legal input and must fail here rather than silently start the walk from a
// half-parsed state.
func dcInputLevelQuality(variant string) (level, quality int, ok bool) {
	lvl, qual, found := strings.Cut(variant, "/")
	if !found {
		qual = "0"
	}
	level, err := strconv.Atoi(lvl)
	if err != nil {
		return 0, 0, false
	}
	quality, err = strconv.Atoi(qual)
	if err != nil {
		return 0, 0, false
	}
	return level, quality, true
}

// AnalyzeDoubleCorrupt computes the double-corruption EV for every priceable
// gem at every input variant in DoubleCorruptVariants. gems must be the full
// latest snapshot (both corruption sides: the uncorrupted rows select and
// price the inputs, the corrupted rows price the outcome cells).
// templeOverheadChaos is a flat per-run cost subtracted from profit;
// DefaultTempleOverheadChaos treats the room as sunk.
//
// features are used for outcome risk-adjustment where a feature row exists for
// an outcome cell; corrupted variants carry no features today (the v2 pipeline
// computes them for uncorrupted transfigured gems only), so cells fall back to
// the same defaults Dedication uses: a listings-only sell probability and no
// stability discount.
//
// Every result is per (name, input variant): outcome cells are looked up at
// the gem's own name (or its Vaal identity) at exact corrupted variants, so no
// other gem's and no other variant's market can leak into an EV.
//
// THE CONFIDENCE GATE. A listed price standing on a couple of listings is not a
// price, and at the top of a profit ranking it is the *only* thing there: the
// gems that rank first are exactly the ones carrying one freakish outlier cell.
// Both sides of the subtraction are therefore gated by the same rule the Font
// and Dedication classifiers use — detectLowConfidenceFiltered, per variant,
// flagging any row whose listing depth is under 40% of its variant's median.
//
//   - An outcome cell whose corrupted row is low-confidence is treated as
//     UNPRICED. Its mass flows into UnpricedProbability instead of being
//     discounted, because a discount still lets an outlier dominate an EV once
//     it is large enough. Exclusion is the only gate an outlier cannot outgrow.
//   - An input whose own uncorrupted row is low-confidence produces no result at
//     all. InputCost is subtracted from EV, so a thin input price is a thin
//     profit number, and there is no half-answer to serve for it.
//
// The two pools are separate on purpose (per-variant rule): a corrupted 21/23c
// row's depth is judged against other corrupted 21/23c rows, never against the
// uncorrupted 20/20 market it came from.
//
// Knock-on: ThinOutcomeCells and LiquidityRisk now count only cells that
// cleared the gate, so a variant whose median depth is above ~12 listings can
// no longer report a thin cell at all — the gate removes those rows before the
// < 5 listings flag sees them. The flag still marks a shallow cell in a shallow
// variant, which is the case it was reading for.
func AnalyzeDoubleCorrupt(snapTime time.Time, gems []GemPrice, features []GemFeature, templeOverheadChaos float64) []DoubleCorruptResult {
	type priceKey struct{ name, variant string }

	// Corrupted rows index — the outcome cells' price source.
	corrupted := make(map[priceKey]*GemPrice)
	listedCorrupted := make(map[string]bool)
	for i := range gems {
		g := &gems[i]
		if g.IsCorrupted {
			corrupted[priceKey{g.Name, g.Variant}] = g
			listedCorrupted[g.Name] = true
		}
	}
	vaalIdentity := buildVaalIdentityIndex(gems, listedCorrupted)

	lowConfOutcome := detectLowConfidenceFiltered(gems, func(g GemPrice) bool { return g.IsCorrupted })
	lowConfInput := detectLowConfidenceFiltered(gems, isDoubleCorruptInput)

	featureLookup := make(map[priceKey]*GemFeature, len(features))
	for i := range features {
		f := &features[i]
		featureLookup[priceKey{f.Name, f.Variant}] = f
	}

	var results []DoubleCorruptResult

	for _, inputVariant := range DoubleCorruptVariants {
		level, quality, ok := dcInputLevelQuality(inputVariant)
		if !ok {
			continue
		}

		// The cell distribution depends only on (hasVaal, level, quality), so
		// the walk runs twice per input variant rather than once per gem.
		distByVaal := map[bool]map[dcCell]float64{
			false: doubleCorruptCellDistribution(false, level, quality),
			true:  doubleCorruptCellDistribution(true, level, quality),
		}

		for i := range gems {
			g := &gems[i]
			if g.Variant != inputVariant || !isDoubleCorruptInput(*g) || g.Chaos <= 0 {
				continue
			}
			if lowConfInput[g.Name+"|"+g.Variant] {
				continue
			}

			vaalName, hasVaal := vaalIdentity[g.Name]
			cellDist := distByVaal[hasVaal]

			outcomes := make([]DoubleCorruptOutcome, 0, len(cellDist))

			for cell, prob := range cellDist {
				name := g.Name
				if cell.vaal {
					name = vaalName
				}
				out := DoubleCorruptOutcome{
					Name:        name,
					Variant:     cell.variant,
					Probability: prob,
				}

				if row, listed := corrupted[priceKey{name, cell.variant}]; listed && row.Chaos > 0 {
					out.Listings = row.Listings
					out.LowConfidence = lowConfOutcome[name+"|"+cell.variant]
					if !out.LowConfidence {
						out.Priced = true
						out.Chaos = row.Chaos
						out.Thin = row.Listings < 5

						// Risk adjustment, mirroring Dedication's no-feature
						// defaults for corrupted rows.
						sellProb := sellProbabilityFactor(row.Listings, 0, row.Chaos)
						stabDisc := 1.0
						if feat := featureLookup[priceKey{name, cell.variant}]; feat != nil {
							sellProb = sellProbabilityFactor(row.Listings, feat.Low7Days, row.Chaos)
							stabDisc = stabilityDiscount(feat.CVShort)
						}
						out.Adjusted = row.Chaos * sellProb * stabDisc
					}
				}

				outcomes = append(outcomes, out)
			}

			// Deterministic order: probability descending, then name/variant.
			// The accumulation below walks the SORTED slice rather than the
			// cell map: float addition is not associative, so summing in Go's
			// randomized map order would hand two runs over identical data
			// EVs that differ in the last bits — and the ranking those EVs
			// feed is served from a cache one run filled and another compares
			// against.
			sort.Slice(outcomes, func(a, b int) bool {
				if outcomes[a].Probability != outcomes[b].Probability {
					return outcomes[a].Probability > outcomes[b].Probability
				}
				if outcomes[a].Name != outcomes[b].Name {
					return outcomes[a].Name < outcomes[b].Name
				}
				return outcomes[a].Variant < outcomes[b].Variant
			})

			var ev, evRaw, pricedProb, unpricedProb float64
			var thinCells, pricedCells int
			for _, out := range outcomes {
				if !out.Priced {
					// Unpriced cell — nobody lists it, or the confidence gate
					// refused the row it has. Either way it contributes 0 to
					// EV, but never silently: the mass is reported so the EV
					// reads as a floor over the priced share of the
					// distribution, not as the full expectation.
					unpricedProb += out.Probability
					continue
				}
				ev += out.Probability * out.Adjusted
				evRaw += out.Probability * out.Chaos
				pricedProb += out.Probability
				pricedCells++
				if out.Thin {
					thinCells++
				}
			}

			results = append(results, DoubleCorruptResult{
				Time:                snapTime,
				Name:                g.Name,
				Color:               g.GemColor,
				InputVariant:        inputVariant,
				InputCost:           g.Chaos,
				TempleOverheadChaos: templeOverheadChaos,
				HasVaalVersion:      hasVaal,
				EV:                  ev,
				EVRaw:               evRaw,
				// Profit is measured off EV, not EVRaw. Both numbers reach the
				// same card in the comparator ("est. ~X corrupted · +Y vs
				// selling here"), so a raw profit beside a risk-adjusted
				// estimate would let Y exceed X and read as two claims about
				// one craft. EVRaw stays on the result as the undiscounted
				// reference, and nothing ranks or gates on it.
				Profit:              ev - g.Chaos - templeOverheadChaos,
				PricedProbability:   pricedProb,
				UnpricedProbability: unpricedProb,
				ThinOutcomeCells:    thinCells,
				LiquidityRisk:       computeLiquidityRisk(thinCells, pricedCells),
				Outcomes:            outcomes,
				Model:               DoubleCorruptModelEstimated,
			})
		}
	}

	// Deterministic order across the whole result set: most profitable first,
	// then by name — this is also the serving order of the ranking endpoint.
	sort.Slice(results, func(a, b int) bool {
		if results[a].Profit != results[b].Profit {
			return results[a].Profit > results[b].Profit
		}
		return results[a].Name < results[b].Name
	})

	return results
}
