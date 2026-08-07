package lab

import (
	"sort"
	"strings"
)

// Gem eligibility — the one place that records which priced gems a Labyrinth
// gem craft can actually hand out, and which a player can actually feed one.
//
// The question "can the lab produce this gem" was previously answered six times
// with six slightly different rules (POE-142): the Dedication pool, the
// Font/analysis filter, two bare `strings.Contains(name, "Trarthus")` checks,
// the OCR gem dictionary, and the Dedication autocomplete SQL. They overlapped
// but were not the same, so an exclusion added to one silently missed the
// others. What the merge changed in behaviour, measured rather than inferred:
//
//   - isFontOutcome gained the WHITE and support rules. The isAnalyzableGem it
//     replaced carried neither, so tier classification, the gem features, the
//     market-context percentiles, the temporal depth gate and the tier price
//     collection ran over a wider universe than AnalyzeFont, which has always
//     narrowed its own pool to RED/GREEN/BLUE. Measured 2026-08-05 on the local
//     DB: the latest Allflame snapshot holds 0 uncorrupted transfigured gems
//     that are WHITE or " Support"-suffixed, so no number moves today. It closes
//     the shape of a divergence, not an open instance.
//   - The Dedication autocomplete SQL gained the WHITE rule, which it had never
//     applied. Measured 2026-08-05 against the latest local Allflame snapshot:
//     the non-transfigured half offered 339 names and now offers 335, the four
//     dropped being the WHITE 3.29 Pact gems (Beidat, Ghorr, K'Tash, Lycia) —
//     names isDedicationOutcome already keeps out of the pool, the tiers and the
//     rankings, and that the compare path can only ever render as AVOID.
//   - The transfigured OCR dictionary carried 12 "of Trarthus" names — the
//     collector refuses to store one, every analysis path excludes one, and
//     gem_colors seeds them anyway.
//   - The Font picker's own SQL (GemNamesAutocomplete) answered the question
//     zero times rather than differently: it filtered on is_transfigured and
//     nothing else, which made it the widest unfiltered name source feeding a
//     gem icon. Measured 2026-08-05 on the latest local Allflame snapshot it
//     offered 247 names against the warm cache's 202, the 45 extra being
//     poe.ninja's corrupted "Vaal <Base> (<Transfigured>)" market identities.
//     It now applies the SQL half of isFontOutcome and returns 202.
//   - The Font picker's warm half is no longer a seventh answer. It used to be
//     derived from the transfigure results, whose gate is
//     isTransfigurePairCandidate — no support rule, no WHITE rule — and it is
//     the half that serves nearly every keystroke, so a divergence there would
//     have hidden behind a green cold-path test. RunTransfigure now fills it
//     from GemNamesAutocomplete itself; see Cache.SetGemNamePool.
//
// The Dedication predicates are a rename and nothing else: isDedicationFeed and
// isDedicationOutcome apply exactly the rules isDedicationFeed and
// isDedicationGem applied before the merge, colour rule included.
//
// The file is organised in two layers:
//
//   - Exclusion rules — one function per *game rule*, each documenting the rule
//     and the evidence for it. A new league's new item type is added here, once.
//   - Surface predicates — one per consumer, composed from those rules. The
//     surfaces differ for real reasons (Dedication draws from corrupted gems,
//     the Font of Divine Skill from uncorrupted transfigured ones, the quality
//     roll enhances whatever you feed it) and are kept separate rather than
//     collapsed into one boolean.
//
// Not covered here: the collector's own Trarthus drop in
// internal/collector/ninja.go. That is a different decision — do not *store*
// the row — taken before any lab concept exists, and package collector does not
// import package lab. Keep the two in step by hand.

// The colour rules. There are two, and the difference between them is the one
// place where "colourless" has to be read carefully.
//
// A gem's colour comes from the gemcolor resolver, which upserts names into
// gem_colors as it meets them (ADR-006). It yields RED, GREEN, BLUE, WHITE — or
// nothing at all, when the name is new to it. WHITE and "" both read as "not one
// of the three", and collapsing them is how a data gap gets mistaken for a game
// rule.
//
//   - WHITE is a game rule. A white gem has no attribute requirement, and both
//     lab gem crafts hand out a gem "of the same colour", so a white gem is
//     never an outcome. Observed at the latest local snapshot (Allflame,
//     2026-07-30) the WHITE names are Portal, Eclipse Support, and the four 3.29
//     Pact gems (Beidat, Ghorr, K'Tash, Lycia) — none lab-obtainable.
//   - "" is a data gap. The resolver has not seen the name yet. Dropping a gem
//     for it states something about the gem that is not true of it, and at a
//     league start — when the resolver is coldest and players run the most
//     labs — it would drop exactly the new gems.
//
// So the two rules are applied to different surfaces, deliberately:
// isColorlessByRule (WHITE only) everywhere, hasAttributeColor (all three
// required) only where the surface partitions the market by colour and a gem
// with no colour has no pool to sit in.

// isColorlessByRule reports whether the gem is white — no attribute
// requirement, and therefore never an outcome of either lab gem craft.
func isColorlessByRule(g GemPrice) bool {
	return g.GemColor == "WHITE"
}

// hasAttributeColor reports whether the gem resolved to one of the three
// attribute colours the Dedication pools are partitioned by. An unresolved
// colour fails this too — see the colour-rules comment above for why that is a
// cost this surface accepts and the Font surface does not.
//
// What that cost amounts to is a measurement per tick, not a list kept by hand.
// unresolvedDedicationColorNames counts the gems this drops for the data gap
// rather than for a rule, and RunDedication reports the count on every
// Dedication run. A hand-maintained list of open instances rots silently — the
// one this replaces did: it named Dark Bargain and Mana-Infused Staff, and
// measured 2026-08-05 against the local DB both now carry BLUE in gem_colors
// and on every row of the latest Allflame snapshot. The resolver had caught up
// and nothing said so.
//
// Measured 2026-08-05 on that same snapshot the count is zero. The only two
// unresolved names left in it are Communion Support and Crystalfall Support,
// which isLabOutcomeName rejects for the support rule whatever their colour, so
// they are not instances of this cost. Read the tick's count rather than this
// paragraph — the date is here to say when the paragraph was last true, not to
// stand in for the signal.
func hasAttributeColor(g GemPrice) bool {
	switch g.GemColor {
	case "RED", "GREEN", "BLUE":
		return true
	}
	return false
}

// unresolvedDedicationColorNames returns the distinct names of gems that
// isDedicationFeed drops for an unresolved colour ALONE — every other feed rule
// passes, so each name is a gem the Dedication pools should have held and did
// not. Sorted, so the log line is stable between ticks.
//
// This is the observable for the single exclusion in this file that is a data
// gap rather than a game rule, and it covers both Dedication predicates at once:
// isDedicationOutcome is isDedicationFeed plus the Vaal rule, so a gem this
// counts is missing from the outcome pool too. Downstream that is a Dedication
// EV, a pWin and an input cost computed over a quietly short pool — worst at a
// league start, when the resolver is coldest and players run the most labs.
//
// WHITE is deliberately not counted. A white gem is dropped by
// isColorlessByRule, a game rule that would drop it with the resolver fully
// warm; folding it in here would report a permanent non-zero floor and train
// the reader to ignore the number.
//
// The eligibility test is the real predicate applied to a probe copy carrying a
// resolved colour, rather than a second transcription of the feed rules. A rule
// added to isDedicationFeed has to reach this counter, and the six-answers
// history at the top of this file is what a copy costs.
//
// The same gap reaches the SQL surfaces this file backs — the Dedication
// autocomplete's sqlHasAttributeColor fragment in repository.go, and through it
// the handler that turns an empty name list into [] at HTTP 200, where a colour
// gap and a typo'd query term are byte-identical responses. A query cannot call
// a Go function, but it reads the same gem_color column of the same snapshot, so
// a non-zero count here is the signal for those too.
func unresolvedDedicationColorNames(gems []GemPrice) []string {
	seen := make(map[string]struct{})
	for _, g := range gems {
		if hasAttributeColor(g) || isColorlessByRule(g) {
			continue
		}
		probe := g
		probe.GemColor = "RED"
		if !isDedicationFeed(probe) {
			continue
		}
		seen[g.Name] = struct{}{}
	}
	if len(seen) == 0 {
		return nil
	}
	out := make([]string, 0, len(seen))
	for name := range seen {
		out = append(out, name)
	}
	sort.Strings(out)
	return out
}

// isSupportGemName reports whether a gem name is a support gem.
//
// Game rule: the Font of Divine Skill and the Dedication to the Goddess hand out
// skill gems only (confirmed with the domain owner), so a support gem is never a
// possible outcome of either craft. It remains a perfectly legal *input* to the
// Merciless quality roll, which enhances whatever gem you feed it — see
// isQualityRollCandidate.
//
// The suffix test is exhaustive over our data: gem_snapshots is written only by
// the collector from poe.ninja's type=SkillGem endpoint
// (internal/collector/ninja.go), whose only non-active-skill category is support
// gems, and every support gem's name ends in " Support". Measured 2026-08-05 on
// the local DB: 225 distinct " Support"-suffixed names in gem_snapshots and 0
// names carrying "Support" anywhere else, so the `strings.Contains` and SQL
// `LIKE '%Support%'` forms this replaced agreed with it on every row.
//
// Observed 2026-08-04 on prod: the transfigured=false OCR dictionary carried 224
// " Support" names out of 571, because the gem_snapshots half of
// GemNameDictionary was merged unfiltered. POE-145 covers the other half of that
// failure — deleting a name from a fuzzy matcher's vocabulary redirects the
// match rather than rejecting it — with a reject gate in the desktop matcher
// (desktop/src-tauri/src/gem_matcher.rs). That gate is Rust and cannot call this
// function; it is keyed on the same " Support" name shape by design.
func isSupportGemName(name string) bool {
	return strings.HasSuffix(name, " Support")
}

// isHeistOnlyGemName reports whether a gem is Heist-exclusive and therefore
// unobtainable in standard league play.
//
// Game rule: the "of Trarthus" transfigured gems drop only from Heist
// blueprints, so the lab can neither hand one out nor be fed one. The collector
// refuses to store them at all (internal/collector/ninja.go), and measured
// 2026-08-05 the local gem_snapshots holds 0 Trarthus rows — but gem_colors
// still carries 12 seeded Trarthus names, so any surface reading gem_colors
// rather than snapshots has to apply this rule itself.
func isHeistOnlyGemName(name string) bool {
	return strings.Contains(name, "Trarthus")
}

// isVaalGemName reports whether a gem is a Vaal gem.
//
// Game rule, measured in game 2026-07-30: a Vaal gem is a legal Dedication
// *input* but never an *outcome*. It therefore prices into what a run costs and
// never into what it returns.
func isVaalGemName(name string) bool {
	return strings.HasPrefix(name, "Vaal ")
}

// isLabOutcomeName reports whether a name could name a gem some lab craft hands
// out, using the name alone.
//
// This is the weakest of the surface predicates and exists for the surfaces that
// have no price row to inspect — the OCR gem dictionary, which unions gem_colors
// (no league, no prices, no flags) with the league's snapshot names. Colour,
// corruption and transfigured-ness are unavailable there, so only the two
// name-shaped rules apply.
func isLabOutcomeName(name string) bool {
	return !isSupportGemName(name) && !isHeistOnlyGemName(name)
}

// isFontOutcome reports whether a gem can come out of the Font of Divine Skill.
//
// The craft rerolls a transfigured gem into another transfigured gem of the same
// colour, so the pool is the uncorrupted transfigured market, minus the support
// and Heist rules, minus anything the colour resolver cannot place on the
// three-colour wheel.
//
// The support and WHITE rules are what POE-142 added here. AnalyzeFont has
// always narrowed its own pool to RED/GREEN/BLUE, but tier classification, the
// gem features, the market-context percentiles and the temporal depth gate ran
// over a wider universe. Measured 2026-08-05 on the local DB, the latest
// snapshot holds 0 uncorrupted transfigured gems that are WHITE or
// support-suffixed, so this changes no number today; it closes the shape of the
// divergence, not an open instance.
//
// It stops short of requiring a resolved colour, unlike isDedicationFeed — see
// the colour-rules comment above. Nothing on this path partitions by colour
// except AnalyzeFont itself, which already applies its own RGB narrowing, so
// there is no reason to pay the league-start cost here.
//
// Call sites layer their own additional filters on top (a minimum chaos, a
// variant set, a listings floor) — those are analysis policy, not eligibility.
func isFontOutcome(g GemPrice) bool {
	return g.IsTransfigured &&
		!g.IsCorrupted &&
		!isColorlessByRule(g) &&
		isLabOutcomeName(g.Name)
}

// isDedicationFeed reports whether a gem can be fed INTO the Dedication craft:
// corrupted, on the three-colour wheel, not a support gem, not Heist-only.
//
// A Vaal gem at 21/23c cannot exist: it would take three corruption outcomes —
// the Vaal transform, the extra level and the extra quality — and the Temple's
// double corrupt grants two. At 21/20c it is two outcomes and does exist (25
// such listings on 2026-07-30), so it stays a legal feed there. The price feed
// carries none today; this keeps a bad listing from pricing into what a run
// costs.
func isDedicationFeed(g GemPrice) bool {
	if !hasAttributeColor(g) {
		return false
	}
	if isVaalGemName(g.Name) && g.Variant == "21/23c" {
		return false
	}
	return g.IsCorrupted && isLabOutcomeName(g.Name)
}

// isDedicationOutcome reports whether a gem can come OUT of the Dedication
// craft. That is every feed gem except a Vaal gem — see isVaalGemName.
//
// The distinction has to live in the predicate rather than in the pool loop:
// tier classification and the rankings read this directly, and a colourless 36k
// listing was setting the TOP boundary for every colour before it did.
func isDedicationOutcome(g GemPrice) bool {
	return isDedicationFeed(g) && !isVaalGemName(g.Name)
}

// isQualityRollCandidate reports whether a gem can take part in the Merciless
// quality-roll analysis.
//
// Different surface, different rule: the quality font enhances the gem you feed
// it rather than handing you a random one, so support gems and white gems are
// legitimate subjects here and are deliberately NOT excluded — the analysis
// prices buying a 0% gem and selling it at 20%, which works for any gem a player
// can own. Only the Heist rule and corruption apply: a corrupted gem cannot take
// quality.
func isQualityRollCandidate(g GemPrice) bool {
	return !g.IsCorrupted && !isHeistOnlyGemName(g.Name)
}

// isTransfigurePairCandidate reports whether a gem may take either side of a
// base→transfigured ROI pair.
//
// Wider than isFontOutcome on purpose: the pair needs the *base* gem, which is
// by definition not transfigured, so this cannot gate on IsTransfigured. It also
// does not gate on colour — an unpriceable colour costs the pair nothing, and
// the ROI is a straight price difference that does not touch the colour wheel.
func isTransfigurePairCandidate(g GemPrice) bool {
	return !g.IsCorrupted && !isHeistOnlyGemName(g.Name)
}

// SQL forms of the rules above, for the repository queries that must apply
// eligibility in the database rather than in Go.
//
// They are string constants rather than a shared predicate because a query
// cannot call a Go function, and they are here rather than inline in
// repository.go so that a rule change lands in one file. Each must stay
// equivalent to the Go rule named in its comment;
// TestSQLExclusionsMatchGoPredicates pins the pairing.
const (
	// sqlNotHeistOnly matches isHeistOnlyGemName.
	sqlNotHeistOnly = `name NOT LIKE '%Trarthus%'`

	// sqlNotSupportGem matches isSupportGemName. The leading space is
	// load-bearing: the rule is a suffix, not a substring.
	sqlNotSupportGem = `name NOT LIKE '% Support'`

	// sqlNotColorlessByRule matches !isColorlessByRule — the WHITE-only rule,
	// which is the one a surface that does not partition by colour wants. The
	// COALESCE is what keeps an unresolved colour on the keeping side: a bare
	// `gem_color <> 'WHITE'` evaluates to NULL for a NULL colour and a NULL
	// WHERE term drops the row, turning the data gap into an exclusion.
	sqlNotColorlessByRule = `COALESCE(gem_color, '') <> 'WHITE'`

	// sqlHasAttributeColor matches hasAttributeColor. Reach for
	// sqlNotColorlessByRule instead unless the query really does partition the
	// market by colour — this fragment is false for an unresolved colour too.
	sqlHasAttributeColor = `COALESCE(gem_color, '') IN ('RED', 'GREEN', 'BLUE')`
)
