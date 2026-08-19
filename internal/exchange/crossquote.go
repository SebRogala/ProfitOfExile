package exchange

import "sort"

// crossQuoteCandidates finds every one-hop route in an hour's rows.
//
// A cross-quote play is ONE item traded against TWO quote currencies: it buys
// item X against currency A, sells it against currency B, and converts the B
// proceeds back to A on the A/B market — three executed legs, so three ways to
// be wrong:
//
//	edge = highXinB * highBinA / lowXinA - 1
//
// Currency means exactly one thing here: an id listed in Config.QuotePriority.
// A and B must both be listed, must differ, and must trade against each other
// this hour; X must NOT be listed. A loop of three items none of which is a
// quote currency — three scarabs with a scarab/scarab market closing them — is
// therefore not a one-hop play, and neither is a route that would trade a quote
// currency as its item. With the default priority an item quoted in both
// divine and chaos yields exactly two routes, not the three rotations of its
// triangle: A -> X -> B -> A keys as "1-hop:X|A|B" and the mirror
// B -> X -> A -> B keys as "1-hop:X|B|A". Both are emitted because the
// profitable direction is not symmetric.
//
// Every price is one of the hour's realized extremes rather than a quote that
// stood at the moment of the trade (see priceIn), so the edge is an upper bound
// and the tighter the tick granularity of the pair, the more of it is fiction.
//
// Each leg is gated on its own market's depth (gatedLeg) and every price must
// come back ok, so one thin or unpriced side drops the whole route.
//
// Output order does not depend on the order of rows: markets are indexed by
// unordered pair, and both the items and their currency sides are sorted before
// they are walked.
func crossQuoteCandidates(rows []Row, cfg Config) []candidate {
	byPair := make(map[string]Row, len(rows))
	counterparties := make(map[string]map[string]bool)

	for _, r := range rows {
		if r.ItemA == "" || r.ItemB == "" || r.ItemA == r.ItemB {
			continue
		}
		key := pairKey(r.ItemA, r.ItemB)
		if _, duplicate := byPair[key]; duplicate {
			continue
		}
		byPair[key] = r
		addCounterparty(counterparties, r.ItemA, r.ItemB)
		addCounterparty(counterparties, r.ItemB, r.ItemA)
	}

	var candidates []candidate
	for _, x := range sortedIDs(counterparties) {
		if priorityRank(x, cfg.QuotePriority) >= 0 {
			continue
		}
		quotes := currencySides(counterparties[x], cfg.QuotePriority)
		for i := 0; i < len(quotes); i++ {
			for j := i + 1; j < len(quotes); j++ {
				a, b := quotes[i], quotes[j]
				rowAB, ok := byPair[pairKey(a, b)]
				if !ok {
					continue
				}
				rowXA := byPair[pairKey(x, a)]
				rowXB := byPair[pairKey(x, b)]

				if c, ok := oneHopCandidate(x, a, b, rowXA, rowXB, rowAB, cfg); ok {
					candidates = append(candidates, c)
				}
				if c, ok := oneHopCandidate(x, b, a, rowXB, rowXA, rowAB, cfg); ok {
					candidates = append(candidates, c)
				}
			}
		}
	}
	return candidates
}

// oneHopCandidate builds the route A -> X -> B -> A: buy the item X against
// currency A at the hour's cheapest price, sell X against currency B at the
// dearest, then sell the B back into A at the dearest. rowXA prices X in A,
// rowXB prices X in B, and rowAB prices B in A.
//
// It returns ok == false when any of the three prices is unavailable or any leg
// fails its depth gate. The mirror route is the same call with a and b (and the
// two X rows) swapped.
func oneHopCandidate(x, a, b string, rowXA, rowXB, rowAB Row, cfg Config) (candidate, bool) {
	buyX, ok := gatedLeg("buy", x, a, rowXA, cfg)
	if !ok {
		return candidate{}, false
	}
	sellX, ok := gatedLeg("sell", x, b, rowXB, cfg)
	if !ok {
		return candidate{}, false
	}
	sellB, ok := gatedLeg("sell", b, a, rowAB, cfg)
	if !ok {
		return candidate{}, false
	}

	return candidate{
		key:  "1-hop:" + x + "|" + a + "|" + b,
		mode: ModeOneHop,
		legs: []candidateLeg{buyX, sellX, sellB},
		edge: sellX.obs.high*sellB.obs.high/buyX.obs.low - 1,
	}, true
}

// pairKey names an unordered market pair, so the A/B market is found under the
// same key whichever side the caller happens to hold.
func pairKey(a, b string) string {
	if a <= b {
		return a + "|" + b
	}
	return b + "|" + a
}

// addCounterparty records that item is quoted against counterparty this hour.
func addCounterparty(index map[string]map[string]bool, item, counterparty string) {
	if index[item] == nil {
		index[item] = make(map[string]bool)
	}
	index[item][counterparty] = true
}

// currencySides returns the counterparties an item is quoted against that
// Config.QuotePriority names as currencies, in ascending id order. They are the
// only sides a one-hop route may use as its two quotes.
func currencySides(counterparties map[string]bool, priority []string) []string {
	sides := make([]string, 0, len(counterparties))
	for _, id := range sortedIDs(counterparties) {
		if priorityRank(id, priority) >= 0 {
			sides = append(sides, id)
		}
	}
	return sides
}

// sortedIDs returns a map's item ids in ascending order, so no map iteration
// order reaches the engine's output.
func sortedIDs[V any](m map[string]V) []string {
	ids := make([]string, 0, len(m))
	for id := range m {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	return ids
}
