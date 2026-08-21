import { describe, it, expect } from 'vitest';
import {
	applyGates,
	applyNumericFilters,
	applyRules,
	cycleCategoryRule,
	effectiveRule,
	gateDefaults,
	itemUniverse,
	matchesSearch,
	overridesCategory,
	parseCategoryRules,
	parseGate,
	parseGates,
	parseItemRules,
	playSides,
	serializeCategoryRules,
	serializeItemRules
} from './filters';
import type { CategoryRules, GateInputs, Gates, ItemRule, NumericFilters } from './filters';
import type { CurrencyExchangeLeg, CurrencyExchangePlay } from '$lib/api';

/**
 * A clean Currency leg: buy Divine Orbs with Chaos Orbs. Every rule case
 * overrides only the side it is about, so a test that reads as "one hidden
 * category" cannot be quietly carrying a second difference.
 */
function leg(overrides: Partial<CurrencyExchangeLeg> = {}): CurrencyExchangeLeg {
	return {
		action: 'buy',
		item: 'divine',
		quote: 'chaos',
		price: 196,
		fair: 197.4,
		fairOk: true,
		tick: 0.005,
		volume: 1200,
		stock: 40,
		suspect: false,
		itemName: 'Divine Orb',
		itemIcon: '/currency-exchange/icon/divine',
		itemCategory: 'Currency',
		quoteName: 'Chaos Orb',
		quoteIcon: '/currency-exchange/icon/chaos',
		quoteCategory: 'Currency',
		...overrides
	};
}

/**
 * A ranked play over one leg.
 *
 * The optimistic pair (`roi`/`roiPct`) and the simulated pair
 * (`expectedRoi`/`expectedRoiPct`) carry deliberately DIFFERENT values, and the
 * expectation is the smaller of the two, as the calibration found it to be
 * (POE-193). Every gate in this file judges the optimistic pair, so a gate case
 * that overrides `roi` or `roiPct` leaves an expectation that would answer the
 * same gate differently — which is what makes those cases able to fail if a
 * gate is ever re-pointed at the measured number.
 */
function play(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
	return {
		key: 'direct:divine:chaos',
		mode: 'direct',
		legs: [leg()],
		roiPct: 0.05,
		edge: 0.05,
		roiPctRaw: 0.08,
		roi: 10,
		investment: 200,
		expectedRoi: 4,
		expectedRoiPct: 0.015,
		simEntries: 22,
		lowCoverage: false,
		turnover: 5000,
		tick: 0.005,
		depth: 40,
		suspect: false,
		hoursSeen: 6,
		lastHour: '2026-08-19T11:00:00.000Z',
		...overrides
	};
}

/** Every numeric filter off, which is the state the bar starts in. */
function filters(overrides: Partial<NumericFilters> = {}): NumericFilters {
	return {
		investMin: '',
		investMax: '',
		unit: 'chaos',
		divineChaosRate: 198.97,
		...overrides
	};
}

/**
 * Every gate off, so a gate test says which single gate it is about.
 *
 * Written out rather than spread from `gateDefaults`, even though POE-193 made
 * the two agree: a per-gate test wants a stated baseline of nothing, and reading
 * it from the shipped defaults would make every one of these cases silently
 * change meaning the day a default is armed again.
 */
function gates(overrides: Partial<Gates> = {}): Gates {
	return {
		minRoiChaos: 0,
		minTurnover: 0,
		maxTickPct: 0,
		minEdgeTickRatio: 0,
		minRoiPct: 0,
		...overrides
	};
}

/** Every gate knob unset, which is the state a fresh install stores. */
function gateInputs(overrides: Partial<GateInputs> = {}): GateInputs {
	return {
		minRoiChaos: '',
		minTurnover: '',
		maxTickPct: '',
		minEdgeTickRatio: '',
		minRoiPct: '',
		...overrides
	};
}

function keys(plays: CurrencyExchangePlay[]): string[] {
	return plays.map((p) => p.key);
}

describe('parseCategoryRules', () => {
	it('reads back the rules it stored', () => {
		const rules: CategoryRules = { Currency: 'hide', Scarabs: 'only' };

		expect(parseCategoryRules(serializeCategoryRules(rules))).toEqual(rules);
	});

	it('keeps a rule on a category this build has never heard of', () => {
		// The taxonomy is the server's — a category GGG adds is a real category,
		// and dropping the rule would silently un-hide a group the reader hid.
		expect(parseCategoryRules('{"Vaults":"hide"}')).toEqual({ Vaults: 'hide' });
	});

	it('drops a category whose stored state is not a rule', () => {
		expect(parseCategoryRules('{"Currency":"maybe","Scarabs":"hide"}')).toEqual({
			Scarabs: 'hide'
		});
	});

	it('drops a rule keyed on the empty category, which is the uncategorised marker', () => {
		expect(parseCategoryRules('{"":"hide"}')).toEqual({});
	});

	it('answers no rules for an unset preference', () => {
		expect(parseCategoryRules('')).toEqual({});
	});

	it('answers no rules for a half-written preference', () => {
		expect(parseCategoryRules('{"Currency":')).toEqual({});
	});

	it('answers no rules for an array where the rule object belongs', () => {
		expect(parseCategoryRules('["Currency"]')).toEqual({});
	});

	it('answers no rules for a stored null', () => {
		expect(parseCategoryRules('null')).toEqual({});
	});
});

describe('parseItemRules', () => {
	it('reads back the rules it stored', () => {
		const rules: ItemRule[] = [{ id: 'divine', name: 'Divine Orb', state: 'only' }];

		expect(parseItemRules(serializeItemRules(rules))).toEqual(rules);
	});

	it('drops one malformed entry without losing the rest', () => {
		// A chip written by an older build should cost the reader that chip, not
		// every rule they have set.
		expect(
			parseItemRules('[{"id":"divine","name":"Divine Orb","state":"only"},{"id":"chaos"}]')
		).toEqual([{ id: 'divine', name: 'Divine Orb', state: 'only' }]);
	});

	it('drops an entry with no id, which could never match a side', () => {
		expect(parseItemRules('[{"id":"","name":"Nothing","state":"hide"}]')).toEqual([]);
	});

	it('labels a rule with its id when the stored name is missing', () => {
		// The chip has to render something; the id is ugly but it is the truth.
		expect(parseItemRules('[{"id":"divine","state":"hide"}]')).toEqual([
			{ id: 'divine', name: 'divine', state: 'hide' }
		]);
	});

	it('answers no rules for an unset preference', () => {
		expect(parseItemRules('')).toEqual([]);
	});

	it('answers no rules for an object where the rule array belongs', () => {
		expect(parseItemRules('{"divine":"hide"}')).toEqual([]);
	});
});

describe('cycleCategoryRule', () => {
	it('turns a neutral pill into only', () => {
		expect(cycleCategoryRule(undefined)).toBe('only');
	});

	it('turns an only pill into hide', () => {
		expect(cycleCategoryRule('only')).toBe('hide');
	});

	it('turns a hide pill back to neutral', () => {
		expect(cycleCategoryRule('hide')).toBeUndefined();
	});
});

describe('playSides', () => {
	it('lists both halves of every leg', () => {
		const hop = play({
			legs: [
				leg({ item: 'divine', quote: 'chaos' }),
				leg({ action: 'sell', item: 'divine', quote: 'exalted', quoteName: 'Exalted Orb' })
			]
		});

		expect(playSides(hop).map((side) => `${side.role}:${side.id}`)).toEqual([
			'item:divine',
			'quote:chaos',
			'quote:exalted'
		]);
	});

	it('lists a currency that fills both roles once for each role it fills', () => {
		// The picker shows the role a side was found in, and a round trip through
		// one currency is genuinely two facts about the play.
		const round = play({
			legs: [
				leg({ item: 'divine', quote: 'chaos' }),
				leg({
					action: 'sell',
					item: 'chaos',
					itemName: 'Chaos Orb',
					quote: 'divine',
					quoteName: 'Divine Orb'
				})
			]
		});

		expect(playSides(round).map((side) => `${side.role}:${side.id}`)).toEqual([
			'item:divine',
			'quote:chaos',
			'item:chaos',
			'quote:divine'
		]);
	});

	it('carries the display name, icon and category of the side it found', () => {
		const sides = playSides(play({ legs: [leg({ itemCategory: 'Fragments' })] }));

		expect(sides[0]).toEqual({
			id: 'divine',
			name: 'Divine Orb',
			icon: '/currency-exchange/icon/divine',
			category: 'Fragments',
			role: 'item'
		});
	});
});

describe('effectiveRule', () => {
	it('lets an item rule override the category it belongs to', () => {
		// "Hide the divination cards, but keep the one card I actually farm."
		expect(effectiveRule('only', 'Divination Cards', { 'Divination Cards': 'hide' })).toBe('only');
	});

	it('falls through to the category rule when the item has none', () => {
		expect(effectiveRule(undefined, 'Divination Cards', { 'Divination Cards': 'hide' })).toBe(
			'hide'
		);
	});

	it('leaves an uncategorised side unruled, whatever the empty key says', () => {
		// `""` is the wire's "the item asset does not cover this id", not a
		// seventeenth group — a rule stored under it must not reach the side.
		expect(effectiveRule(undefined, '', { '': 'hide' })).toBeUndefined();
	});

	it('leaves a side unruled when neither layer names it', () => {
		expect(effectiveRule(undefined, 'Scarabs', { Currency: 'only' })).toBeUndefined();
	});
});

describe('overridesCategory', () => {
	function rule(state: ItemRule['state']): ItemRule {
		return { id: 'imperial-legacy', name: 'The Imperial Legacy', state };
	}

	it('reports an Only item inside a hidden category as overriding it', () => {
		expect(
			overridesCategory(rule('only'), 'Divination Cards', { 'Divination Cards': 'hide' })
		).toBe(true);
	});

	it('reports a Hide item inside a category ruled Only as overriding it', () => {
		expect(
			overridesCategory(rule('hide'), 'Divination Cards', { 'Divination Cards': 'only' })
		).toBe(true);
	});

	it('reports an Only item in a category with no rule as overriding nothing', () => {
		// A neutral category never said anything to contradict — badging this
		// chip would badge every chip the reader sets.
		expect(overridesCategory(rule('only'), 'Divination Cards', { Currency: 'hide' })).toBe(false);
	});

	it('reports an item saying what its category already says as overriding nothing', () => {
		expect(
			overridesCategory(rule('hide'), 'Divination Cards', { 'Divination Cards': 'hide' })
		).toBe(false);
	});

	it('reports an uncategorised item as overriding nothing, whatever the empty key says', () => {
		expect(overridesCategory(rule('only'), '', { '': 'hide' })).toBe(false);
	});

	it('reports an item the response no longer carries as overriding nothing', () => {
		// The rule outlives the response that created it, so the chip can render
		// with no category to compare against.
		expect(overridesCategory(rule('only'), undefined, { 'Divination Cards': 'hide' })).toBe(false);
	});
});

describe('applyRules', () => {
	const imperial = play({
		key: 'imperial',
		legs: [
			leg({
				item: 'imperial-legacy',
				itemName: 'The Imperial Legacy',
				itemCategory: 'Divination Cards'
			})
		]
	});
	const mirrors = play({
		key: 'mirrors',
		legs: [
			leg({ item: 'house-of-mirrors', itemName: 'House of Mirrors', itemCategory: 'Divination Cards' })
		]
	});
	const divinePlay = play({ key: 'divine' });
	const exaltedPlay = play({
		key: 'exalted',
		legs: [leg({ item: 'exalted', itemName: 'Exalted Orb' })]
	});
	const scarabPlay = play({
		key: 'scarab',
		legs: [leg({ item: 'scarab-gilded', itemName: 'Gilded Scarab', itemCategory: 'Scarabs' })]
	});

	it('keeps every play when no rule is set', () => {
		expect(keys(applyRules([divinePlay, scarabPlay], {}, []))).toEqual(['divine', 'scarab']);
	});

	it('shows an item ruled Only inside a category the reader hid', () => {
		// The Imperial Legacy case: "none of the card market except this one card"
		// is only expressible because the item layer beats the category layer.
		const kept = applyRules([imperial, mirrors], { 'Divination Cards': 'hide' }, [
			{ id: 'imperial-legacy', name: 'The Imperial Legacy', state: 'only' }
		]);

		expect(keys(kept)).toEqual(['imperial']);
	});

	it('hides a play as soon as one side is hidden, however Only the rest of it is', () => {
		// Hide beats Only: a play holding something the reader will not trade is
		// not rescued by the other side being on their list.
		const kept = applyRules([divinePlay, exaltedPlay], { Currency: 'only' }, [
			{ id: 'divine', name: 'Divine Orb', state: 'hide' }
		]);

		expect(keys(kept)).toEqual(['exalted']);
	});

	it('requires an Only match once a category is ruled Only', () => {
		expect(keys(applyRules([divinePlay, scarabPlay], { Scarabs: 'only' }, []))).toEqual(['scarab']);
	});

	it('requires an Only match once a single item is ruled Only', () => {
		// Only gates the whole table, not just its own layer — otherwise picking
		// one item Only would leave every unrelated play on screen.
		const kept = applyRules([divinePlay, scarabPlay], {}, [
			{ id: 'scarab-gilded', name: 'Gilded Scarab', state: 'only' }
		]);

		expect(keys(kept)).toEqual(['scarab']);
	});

	it('matches a rule against the quote side of a leg', () => {
		// The reader shops for whichever side they hold: chaos is never an item
		// here, and a rule naming it still has to catch the play.
		const exaltedQuote = play({
			key: 'exalted-quote',
			legs: [leg({ quote: 'exalted', quoteName: 'Exalted Orb' })]
		});

		expect(keys(applyRules([divinePlay, exaltedQuote], {}, [
			{ id: 'chaos', name: 'Chaos Orb', state: 'hide' }
		]))).toEqual(['exalted-quote']);
	});

	const uncategorised = play({
		key: 'uncategorised',
		legs: [
			leg({
				item: 'unknown-item',
				itemName: 'Unknown',
				itemCategory: '',
				quote: 'unknown-quote',
				quoteName: 'Also Unknown',
				quoteCategory: ''
			})
		]
	});

	it('leaves a side the asset could not categorise out of every category rule', () => {
		expect(keys(applyRules([uncategorised, divinePlay], { Currency: 'hide' }, []))).toEqual([
			'uncategorised'
		]);
	});

	it('ignores a hide rule keyed on the empty category', () => {
		// "" is the wire's uncategorised marker, not a sixteenth group, so a rule
		// keyed on it must not sweep up everything the asset does not cover.
		expect(keys(applyRules([uncategorised], { '': 'hide' }, []))).toEqual(['uncategorised']);
	});

	it('does not gate the table on an Only rule keyed on the empty category', () => {
		// Such a rule can never be matched, so treating it as an Only would hide
		// every play with no way for the reader to see why.
		expect(keys(applyRules([uncategorised, divinePlay], { '': 'only' }, []))).toEqual([
			'uncategorised',
			'divine'
		]);
	});
});

describe('parseGate', () => {
	it('reads an unset knob as the default it was handed, not as a hard-coded off', () => {
		// The fallback is the mechanism, not the value: an unset knob resolves
		// through `gateDefaults`, which is what lets a build move what unset means
		// without rewriting settings files. POE-193 moved it to 0; these cases pass
		// 3 so they keep testing the routing rather than today's number.
		expect(parseGate('', 3)).toBe(3);
	});

	it('reads an explicit zero as the gate turned off', () => {
		// The only way to say "show me the cheap fragments too" — 0 has to survive
		// parsing rather than falling through to the default.
		expect(parseGate('0', 3)).toBe(0);
	});

	it('reads the number the reader typed', () => {
		expect(parseGate('1.5', 3)).toBe(1.5);
	});

	it('reads a half-typed knob as its default rather than as a number', () => {
		// Where `parseAmount` answers "no filter" for "1e", a gate answers its
		// default: a knob mid-typing must not filter on whatever prefix parsed.
		expect(parseGate('1e', 3)).toBe(3);
	});

	it('reads a whitespace-only knob as its default rather than as zero', () => {
		// `Number('  ')` is 0, so without the trim a space left in the box would
		// read as the explicit off and unbar the whole table.
		expect(parseGate('  ', 3)).toBe(3);
	});

	it('reads an infinite knob as its default rather than as a floor nothing clears', () => {
		// `persisted()` hands back whatever is in storage, so the read site is the
		// only place a stored "Infinity" stops.
		expect(parseGate('Infinity', 3)).toBe(3);
	});

	it('reads a negative knob as the gate turned off', () => {
		// The server's positivity floor means no served play has a negative
		// return, so a floor below zero could never drop a row.
		expect(parseGate('-5', 3)).toBe(0);
	});
});

describe('parseGates', () => {
	it('reads a fresh install as every gate off', () => {
		// POE-193's visibility rule: nothing stored yet, so nothing is filtered and
		// the reader sees everything the server served. The old server levels are a
		// recommendation the reader types, not a state they inherit.
		expect(parseGates(gateInputs())).toEqual({
			minRoiChaos: 0,
			minTurnover: 0,
			maxTickPct: 0,
			minEdgeTickRatio: 0,
			minRoiPct: 0
		});
	});

	it('arms one knob without arming the other four', () => {
		expect(parseGates(gateInputs({ minTurnover: '10000' }))).toEqual({
			...gateDefaults,
			minTurnover: 10000
		});
	});

	it('prefers a stored return floor over the off it defaults to', () => {
		expect(parseGates(gateInputs({ minRoiPct: '5' })).minRoiPct).toBe(5);
	});
});

describe('applyGates', () => {
	/** Clears every recommended level: 10c gained, a live hour, a one-step spread. */
	const clean = play({ key: 'clean', roi: 10, turnover: 20000, tick: 0.005, roiPct: 0.05 });
	/** A Sacrifice-fragment-shaped play: fails four of the five recommended levels. */
	const fragment = play({ key: 'fragment', roi: 0.5, turnover: 4000, tick: 0.2, roiPct: 0.03 });

	/**
	 * The levels the server used to enforce for everyone, which the desktop shipped
	 * armed until POE-193 and now only recommends (POE-184's calibration, ADR-015).
	 * Spelled here rather than imported because they are no longer a value the
	 * source exports — the point of these tests is that typing them brings the old
	 * table back, so a copy that could not drift with `gateDefaults` is the honest
	 * fixture.
	 */
	const oldServerLevels: Gates = {
		minRoiChaos: 3,
		minTurnover: 10000,
		maxTickPct: 10,
		minEdgeTickRatio: 5,
		minRoiPct: 2
	};

	it('shows a play nothing about it would have cleared, because nothing is armed', () => {
		// POE-193's whole change: the fragment fails four of the old levels and is
		// on the table anyway, because the reader has not asked for any of them.
		expect(keys(applyGates([clean, fragment], gateDefaults))).toEqual(['clean', 'fragment']);
	});

	it('keeps a play the simulation expects to lose chaos', () => {
		// The gates judge the OPTIMISTIC pair on purpose (POE-193): the levels a
		// reader arms were calibrated against `roi` and `roiPct`, so re-pointing one
		// at the expectation would change what a typed level cuts. The measured
		// loser stays on the table and is ranked and coloured for what it is —
		// serve and flag (ADR-016), not a sixth hidden gate — and it survives the
		// armed levels too, not just the empty ones.
		const measuredLoss = play({
			key: 'measured-loss',
			roi: 10,
			turnover: 20000,
			tick: 0.005,
			roiPct: 0.05,
			expectedRoi: -6,
			expectedRoiPct: -0.03
		});

		expect(keys(applyGates([measuredLoss], gateDefaults))).toEqual(['measured-loss']);
		expect(keys(applyGates([measuredLoss], oldServerLevels))).toEqual(['measured-loss']);
	});

	it('drops that cheap thin play again once the old server levels are typed in', () => {
		// The other half of the contract: the levels still mean what they meant, so
		// a reader who wants the pre-POE-193 table types five numbers and has it.
		expect(keys(applyGates([clean, fragment], oldServerLevels))).toEqual(['clean']);
	});

	it('keeps a play that fails four levels when all five gates are explicitly 0', () => {
		// The explicit-off contract, stated against a written-out baseline rather
		// than against `gateDefaults`: 0 disables every comparison, `maxTickPct`
		// included — the fragment's 20% spread would fail any ceiling that ran at
		// all. This is what makes 0 usable as the shipped default, and it stays
		// true (and stays tested) the day a default is armed again.
		expect(keys(applyGates([fragment], gates()))).toEqual(['fragment']);
	});

	it('drops it on one armed knob alone, leaving the other four off', () => {
		// A knob is armed one at a time in practice — the turnover line is the one
		// that hid a real 8.5k-chaos-an-hour flip, and arming it alone is enough to
		// hide the fragment without the reader having touched the rest.
		expect(keys(applyGates([clean, fragment], gates({ minTurnover: 10000 })))).toEqual(['clean']);
	});

	it('keeps a play gaining exactly the chaos floor', () => {
		const exact = play({ key: 'exact', roi: 3 });

		expect(keys(applyGates([exact], gates({ minRoiChaos: 3 })))).toEqual(['exact']);
	});

	it('drops a play a hundredth of a chaos under the floor', () => {
		const under = play({ key: 'under', roi: 2.99 });

		expect(keys(applyGates([under], gates({ minRoiChaos: 3 })))).toEqual([]);
	});

	it('keeps a play whose hour turned over exactly the floor', () => {
		const exact = play({ key: 'exact', turnover: 10000 });

		expect(keys(applyGates([exact], gates({ minTurnover: 10000 })))).toEqual(['exact']);
	});

	it('drops a play one chaos of turnover short of the floor', () => {
		const under = play({ key: 'under', turnover: 9999 });

		expect(keys(applyGates([under], gates({ minTurnover: 10000 })))).toEqual([]);
	});

	it('keeps a play whose spread sits exactly on the ceiling', () => {
		const exact = play({ key: 'exact', tick: 0.1 });

		expect(keys(applyGates([exact], gates({ maxTickPct: 10 })))).toEqual(['exact']);
	});

	it('drops a play whose spread is a hair over the ceiling', () => {
		const over = play({ key: 'over', tick: 0.101 });

		expect(keys(applyGates([over], gates({ maxTickPct: 10 })))).toEqual([]);
	});

	it('reads the spread ceiling as percent points against a fractional tick', () => {
		// The input says 0.5 and the wire says 0.01 — a ceiling compared raw would
		// pass a 1% spread as comfortably under "0.5".
		const wide = play({ key: 'wide', tick: 0.01 });

		expect(keys(applyGates([wide], gates({ maxTickPct: 0.5 })))).toEqual([]);
	});

	it('keeps a play returning exactly the ratio of its own spread', () => {
		const exact = play({ key: 'exact', tick: 0.01, roiPct: 0.05 });

		expect(keys(applyGates([exact], gates({ minEdgeTickRatio: 5 })))).toEqual(['exact']);
	});

	it('drops a play returning just under the ratio of its own spread', () => {
		// The 1-hop case: the whole edge is one price step, so the ratio is what
		// keeps it off the table until the reader asks for it.
		const thin = play({ key: 'thin', tick: 0.01, roiPct: 0.049 });

		expect(keys(applyGates([thin], gates({ minEdgeTickRatio: 5 })))).toEqual([]);
	});

	it('measures the ratio against the spread of the play in hand, not a fixed floor', () => {
		// Same return, twice the spread: the wider market is the one that fails.
		const tight = play({ key: 'tight', tick: 0.005, roiPct: 0.03 });
		const wide = play({ key: 'wide', tick: 0.01, roiPct: 0.03 });

		expect(keys(applyGates([tight, wide], gates({ minEdgeTickRatio: 5 })))).toEqual(['tight']);
	});

	it('reads the return floor as percent points against a fractional roiPct', () => {
		// The input says 2 and the wire says 0.05 — compared raw, a 5% play would
		// fail a 2% floor.
		expect(keys(applyGates([clean], gates({ minRoiPct: 2 })))).toEqual(['clean']);
	});

	it('keeps a play returning exactly the floor', () => {
		const exact = play({ key: 'exact', roiPct: 0.02 });

		expect(keys(applyGates([exact], gates({ minRoiPct: 2 })))).toEqual(['exact']);
	});

	it('drops a play returning a hundredth of a percent under the floor', () => {
		const under = play({ key: 'under', roiPct: 0.0199 });

		expect(keys(applyGates([under], gates({ minRoiPct: 2 })))).toEqual([]);
	});

	it('drops a play that clears four armed gates and fails only the fifth', () => {
		// One failing gate is enough — the reader who wants this play back has to
		// turn that one off, not all of them.
		const thinHour = play({ key: 'thin-hour', roi: 10, turnover: 9000, tick: 0.005, roiPct: 0.05 });

		expect(keys(applyGates([thinHour], oldServerLevels))).toEqual([]);
		expect(keys(applyGates([thinHour], gateDefaults))).toEqual(['thin-hour']);
	});
});

describe('applyNumericFilters', () => {
	// 100c an exchange EXPECTED to pay 10c: ten flips clear the 100c scale
	// target, so the bounds are asked about 1,000c — never about the 100c one
	// exchange costs. The optimistic `roi` is deliberately 25c, which would
	// divide the target into four flips and 400c instead: the scale counts the
	// expectation (POE-193), and these bounds meet whatever it counts.
	const cheap = play({ key: 'cheap', investment: 100, roi: 25, roiPct: 0.25, expectedRoi: 10 });
	// Dearer at the same expectation per exchange: ten flips of 250c tie up
	// 2,500c.
	const dear = play({ key: 'dear', investment: 250, roi: 25, roiPct: 0.1, expectedRoi: 10 });

	it('keeps every play when no bound is typed', () => {
		// The losing play stays: the return floor is a gate now (POE-191), and
		// nothing in this pass looks at `roiPct` at all.
		const loss = play({
			key: 'loss',
			investment: 0,
			roi: -5,
			roiPct: -0.5,
			expectedRoi: -9,
			expectedRoiPct: -0.4
		});

		expect(keys(applyNumericFilters([cheap, loss], filters()))).toEqual(['cheap', 'loss']);
	});

	it('ignores a half-typed bound rather than emptying the table', () => {
		expect(keys(applyNumericFilters([cheap], filters({ investMin: '1e' })))).toEqual(['cheap']);
	});

	it('ignores a whitespace-only bound rather than reading it as zero', () => {
		// `Number('  ')` is 0, so without the trim a space left in Max investment
		// would become a 0c ceiling and empty the table.
		expect(keys(applyNumericFilters([cheap], filters({ investMax: '  ' })))).toEqual(['cheap']);
	});

	it('ignores an infinite bound rather than emptying the table', () => {
		// `persisted()` hands back whatever is in storage, so the read site is the
		// only place a bound of "Infinity" — which no play can ever clear — stops.
		expect(keys(applyNumericFilters([cheap], filters({ investMin: 'Infinity' })))).toEqual([
			'cheap'
		]);
	});

	it('measures the investment floor against the scale the play must be run at', () => {
		// Ten flips tie up 1,000c, so a 900c floor is met — even though one
		// exchange, at 100c, would fall an order of magnitude short of it.
		expect(keys(applyNumericFilters([cheap], filters({ investMin: '900' })))).toEqual(['cheap']);
	});

	it('keeps a play whose scale sits exactly on the investment floor', () => {
		expect(keys(applyNumericFilters([cheap], filters({ investMin: '1000' })))).toEqual(['cheap']);
	});

	it('drops a play whose scale is one chaos under the investment floor', () => {
		expect(keys(applyNumericFilters([cheap], filters({ investMin: '1001' })))).toEqual([]);
	});

	it('drops a play the bankroll covers one exchange of but not the scale it needs', () => {
		// The discriminating case for the re-anchoring (POE-192): 100c an exchange
		// clears a 500c ceiling, and the 1,000c the play has to tie up to be worth
		// running does not. Answering about the exchange would sell the reader a
		// trip they cannot afford to finish.
		expect(keys(applyNumericFilters([cheap], filters({ investMax: '500' })))).toEqual([]);
	});

	it('keeps a play whose scale sits exactly on the investment ceiling', () => {
		expect(keys(applyNumericFilters([cheap], filters({ investMax: '1000' })))).toEqual(['cheap']);
	});

	it('drops a play whose scale is one chaos over the investment ceiling', () => {
		expect(keys(applyNumericFilters([cheap], filters({ investMax: '999' })))).toEqual([]);
	});

	it('reads the investment bounds as divine when the unit says divine', () => {
		// A ceiling of 5 divine at 200c/div is a 1,000c ceiling: the cheap play's
		// scale ties up exactly that, the dear one's 2,500c does not.
		const kept = applyNumericFilters(
			[cheap, dear],
			filters({ unit: 'divine', divineChaosRate: 200, investMax: '5' })
		);

		expect(keys(kept)).toEqual(['cheap']);
	});

	it('reads the bounds as chaos when the hour carried no divine trade', () => {
		// divineChaosRate 0 means the rate is unknown, not that a divine is worth
		// nothing — converting by it would give a 0c ceiling and empty the table.
		const kept = applyNumericFilters(
			[cheap, dear],
			filters({ unit: 'divine', divineChaosRate: 0, investMax: '1000' })
		);

		expect(keys(kept)).toEqual(['cheap']);
	});

	it('measures a play with no derivable scale against one exchange', () => {
		// A play the simulation expects nothing from never reaches the target, so
		// there is no scaled figure to compare — what it demonstrably ties up is
		// the one exchange. Since POE-193 the table really carries such rows: the
		// server's positivity floor governs `roi`, which is why this fixture's is
		// healthy, while the measured expectation is free to come out negative and
		// the play is served anyway (ADR-016).
		const measuredLoss = play({ key: 'measured-loss', investment: 100, roi: 25, expectedRoi: -3 });

		expect(keys(applyNumericFilters([measuredLoss], filters({ investMax: '100' })))).toEqual([
			'measured-loss'
		]);
	});

	it('drops a play with no derivable scale whose one exchange is over the ceiling', () => {
		const measuredLoss = play({ key: 'measured-loss', investment: 100, roi: 25, expectedRoi: -3 });

		expect(keys(applyNumericFilters([measuredLoss], filters({ investMax: '99' })))).toEqual([]);
	});
});

describe('matchesSearch', () => {
	/** Buy Divine Orbs with Chaos Orbs — the item side is "Divine Orb". */
	const divinePlay = play();

	it('finds a play by part of its item name', () => {
		expect(matchesSearch(divinePlay, 'ivine')).toBe(true);
	});

	it('finds a play by part of its QUOTE name', () => {
		// The reader shops for whichever side they hold: this play's item side is
		// Divine, and it is still the play someone spending chaos is looking for.
		expect(matchesSearch(divinePlay, 'chaos')).toBe(true);
	});

	it('finds a play by a name that only its second leg carries', () => {
		const hop = play({
			legs: [
				leg(),
				leg({
					action: 'sell',
					item: 'scarab-gilded',
					itemName: 'Gilded Scarab',
					itemCategory: 'Scarabs'
				})
			]
		});

		expect(matchesSearch(hop, 'gilded')).toBe(true);
	});

	it('ignores the case of the query', () => {
		expect(matchesSearch(divinePlay, 'DIVINE')).toBe(true);
	});

	it('ignores the case of the name', () => {
		const shouty = play({ legs: [leg({ itemName: 'DIVINE ORB' })] });

		expect(matchesSearch(shouty, 'divine')).toBe(true);
	});

	it('drops a play no side of which carries the query', () => {
		expect(matchesSearch(divinePlay, 'scarab')).toBe(false);
	});

	it('keeps every play for an empty query', () => {
		expect(matchesSearch(divinePlay, '')).toBe(true);
	});

	it('keeps every play for a whitespace-only query', () => {
		// A space left in the box is a search the reader is done with, not a
		// needle no name contains — matching it literally would empty the table.
		expect(matchesSearch(divinePlay, '   ')).toBe(true);
	});

	it('ignores the space around a query typed with one', () => {
		expect(matchesSearch(divinePlay, ' divine ')).toBe(true);
	});

	it('does not match on the exchange id, only on the names', () => {
		// The ids are `Metadata/Items/...`-shaped and never on screen, so an id
		// hit would leave a row with nothing in it to explain why it survived.
		const hidden = play({
			legs: [
				leg({
					item: 'Metadata/Items/Currency/CurrencyRerollRare',
					itemName: 'Chromatic Orb',
					quote: 'chaos',
					quoteName: 'Chaos Orb'
				})
			]
		});

		expect(matchesSearch(hidden, 'metadata')).toBe(false);
	});
});

describe('itemUniverse', () => {
	it('lists an item once however many roles and plays it appears in', () => {
		const first = play({ key: 'first', legs: [leg({ item: 'divine', quote: 'chaos' })] });
		const second = play({
			key: 'second',
			legs: [
				leg({ item: 'scarab-gilded', itemName: 'Gilded Scarab', itemCategory: 'Scarabs' }),
				leg({ action: 'sell', item: 'chaos', itemName: 'Chaos Orb', quote: 'divine', quoteName: 'Divine Orb' })
			]
		});

		expect(
			itemUniverse([first, second]).map((item) => [item.id, item.playCount])
		).toEqual([
			['chaos', 2],
			['divine', 2],
			['scarab-gilded', 1]
		]);
	});

	it('counts an item filling both halves of one play once', () => {
		const both = play({
			key: 'both',
			legs: [
				leg({ item: 'divine', quote: 'chaos' }),
				leg({ action: 'sell', item: 'chaos', itemName: 'Chaos Orb', quote: 'divine', quoteName: 'Divine Orb' })
			]
		});

		expect(itemUniverse([both]).map((item) => [item.id, item.playCount])).toEqual([
			['chaos', 1],
			['divine', 1]
		]);
	});

	it('orders the picker by display name rather than by rank', () => {
		const alpha = play({
			key: 'alpha',
			legs: [leg({ item: 'zulu', itemName: 'Zephyr Scarab', quote: 'alpha', quoteName: 'Alva Card' })]
		});

		expect(itemUniverse([alpha]).map((item) => item.name)).toEqual([
			'Alva Card',
			'Zephyr Scarab'
		]);
	});

	it('breaks a display-name tie on the id so the order is stable between renders', () => {
		// Two distinct ids can humanize to one name; without the id tie-break
		// their relative order would depend on insertion order alone.
		const twins = play({
			key: 'twins',
			legs: [
				leg({ item: 'b-id', itemName: 'Same Name', quote: 'a-id', quoteName: 'Same Name' })
			]
		});

		expect(itemUniverse([twins]).map((item) => item.id)).toEqual(['a-id', 'b-id']);
	});

	it('fills a missing icon from another side of the same item that carries one', () => {
		// A leg the asset could not decorate must not blank a row another leg
		// already filled in.
		const bare = play({ key: 'bare', legs: [leg({ item: 'divine', itemIcon: null })] });
		const decorated = play({
			key: 'decorated',
			legs: [leg({ item: 'divine', itemIcon: '/currency-exchange/icon/divine' })]
		});

		const divine = itemUniverse([bare, decorated]).find((item) => item.id === 'divine');

		expect(divine?.icon).toBe('/currency-exchange/icon/divine');
	});

	it('fills a missing category from another side of the same item that carries one', () => {
		const bare = play({ key: 'bare', legs: [leg({ item: 'divine', itemCategory: '' })] });
		const categorised = play({
			key: 'categorised',
			legs: [leg({ item: 'divine', itemCategory: 'Currency' })]
		});

		const divine = itemUniverse([bare, categorised]).find((item) => item.id === 'divine');

		expect(divine?.category).toBe('Currency');
	});

	it('lists nothing for a response that served no plays', () => {
		expect(itemUniverse([])).toEqual([]);
	});
});
