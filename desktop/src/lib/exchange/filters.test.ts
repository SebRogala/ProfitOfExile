import { describe, it, expect } from 'vitest';
import {
	cycleCategoryRule,
	effectiveRule,
	gateDefaults,
	movedGates,
	overridesCategory,
	parseCategoryRules,
	parseGate,
	parseGates,
	parseItemRules,
	playSides,
	resetGateInputs,
	serializeCategoryRules,
	serializeItemRules,
	snapGateInput,
	exchangePageParams
} from './filters';
import type { CategoryRules, ExchangePagePrefs, GateInputs, ItemRule } from './filters';
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
		priceItemQty: 1,
		priceQuoteQty: 196,
		fair: 197.4,
		fairOk: true,
		tick: 0.005,
		volume: 1200,
		stock: 40,
		suspect: false,
		itemName: 'Divine Orb',
		itemIcon: '/icon/currency-exchange/divine',
		itemCategory: 'Currency',
		quoteName: 'Chaos Orb',
		quoteIcon: '/icon/currency-exchange/chaos',
		quoteCategory: 'Currency',
		...overrides
	};
}

/** A ranked play over one leg; `playSides` reads only its legs. */
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
		display: {
			units: 1,
			basis: 'posting',
			money: { investment: 200, roi: 10, expectedRoi: 4 },
			expectedPrinted: true,
			scale: { flips: 25, gain: 100, investment: 5000, hours: 1 },
			runInvestment: 5000,
			sortKeys: { expected: 4, roi: 10, fastestHours: 1 }
		},
		...overrides
	};
}

/** Every gate knob unset, which is the state a fresh install stores. */
function gateInputs(overrides: Partial<GateInputs> = {}): GateInputs {
	return {
		minItemPrice: '',
		minItemPriceDiv: '',
		minRoiChaos: '',
		minTurnover: '',
		maxTickPct: '',
		minEdgeTickRatio: '',
		minRoiPct: '',
		...overrides
	};
}

describe('parseCategoryRules', () => {
	it('reads back the rules it stored', () => {
		const rules: CategoryRules = { Currency: 'hide', Threads: 'only' };

		expect(parseCategoryRules(serializeCategoryRules(rules))).toEqual(rules);
	});

	it('keeps a rule on a category this build has never heard of', () => {
		// The taxonomy is the server's — a category GGG adds is a real category,
		// and dropping the rule would silently un-hide a group the reader hid.
		expect(parseCategoryRules('{"Vaults":"hide"}')).toEqual({ Vaults: 'hide' });
	});

	it('drops a category whose stored state is not a rule', () => {
		expect(parseCategoryRules('{"Currency":"maybe","Threads":"hide"}')).toEqual({
			Threads: 'hide'
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
			icon: '/icon/currency-exchange/divine',
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
		expect(effectiveRule(undefined, 'Threads', { Currency: 'only' })).toBeUndefined();
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
	it('reads a fresh install as both trash-price floors armed and every other gate off', () => {
		// POE-193's visibility rule with its two sanctioned exceptions: nothing is
		// stored yet, so the reader sees everything the server served above the
		// sub-chaos and hundreds-per-divine tiers. The old server levels are a
		// recommendation the reader types, not a state they inherit; the two price
		// floors are the opposite — states they inherit and have to type 0 to leave.
		expect(parseGates(gateInputs())).toEqual({
			minItemPrice: 0.5,
			minItemPriceDiv: 0.4,
			minRoiChaos: 0,
			minTurnover: 0,
			maxTickPct: 0,
			minEdgeTickRatio: 0,
			minRoiPct: 0
		});
	});

	it('reads a blanked divine trash-price box as the shipped floor rather than as off', () => {
		// The divine twin of the case below it, and the reason both knobs persist as
		// '': a reader who empties this box is asking for whatever this build ships.
		// A `parseGates` line that fell back to 0 for this knob alone would look
		// right on every other assertion in this file.
		expect(parseGates(gateInputs({ minItemPriceDiv: '' })).minItemPriceDiv).toBe(0.4);
	});

	it('reads an explicit zero in the divine trash-price box as that floor turned off', () => {
		expect(parseGates(gateInputs({ minItemPriceDiv: '0' })).minItemPriceDiv).toBe(0);
	});

	it('reads a negative divine trash-price floor as off rather than as a level', () => {
		// `parseGate`'s convention reaching this knob: a stored negative is garbage,
		// not a request, and an entry cost is never negative for it to bite on.
		expect(parseGates(gateInputs({ minItemPriceDiv: '-1' })).minItemPriceDiv).toBe(0);
	});

	it('turns one trash-price floor off without touching the other', () => {
		// The two knobs are one line drawn in two currencies and are disarmed
		// separately — a reader who wants the fractional-chaos tier back has not
		// asked for the hundreds-per-divine tier with it.
		expect(parseGates(gateInputs({ minItemPrice: '0' }))).toEqual({
			...gateDefaults,
			minItemPrice: 0
		});
	});

	it('reads a blanked trash-price box as the shipped floor rather than as off', () => {
		// The whole reason the knobs persist as '' — a reader who empties the box is
		// asking for whatever this build ships, not for no filter. Distinct from the
		// fresh install above in intent: this is the state after Defaults, which is
		// the only way BACK to the floor once 0 has been typed into it.
		expect(parseGates(gateInputs({ minItemPrice: '' })).minItemPrice).toBe(0.5);
	});

	it('reads an explicit zero in the trash-price box as the floor turned off', () => {
		// The documented way to disable the one default-on gate. It must not fall
		// through to the default the blank case resolves to.
		expect(parseGates(gateInputs({ minItemPrice: '0' })).minItemPrice).toBe(0);
	});

	it('reads a typed trash-price floor as the level the reader asked for', () => {
		expect(parseGates(gateInputs({ minItemPrice: '5' })).minItemPrice).toBe(5);
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

describe('snapGateInput', () => {
	it('leaves an un-parseable box blank rather than freezing it on the shipped floor', () => {
		// THE FREEZE. `parseGate('abc', 0.4)` is 0.4, so a snap that wrote the parsed
		// number back would put '0.4' in the box and in the settings file — and the
		// knob would stop tracking `gateDefaults` for good, which is exactly what
		// persisting these as '' exists to prevent. Blank resolves to the same 0.4
		// today and follows the build tomorrow.
		expect(snapGateInput('abc', 0.4)).toBe('');
	});

	it('does not freeze a half-typed exponent on the shipped floor either', () => {
		// The realistic way to leave a box un-parseable: a reader typing 1e5 who
		// tabbed away at '1e'. Same answer, and it is the case that made the freeze
		// reachable in normal use.
		expect(snapGateInput('1e', 0.4)).toBe('');
	});

	it('leaves a box the reader emptied alone', () => {
		// '' already means "whatever this build ships", so there is nothing to snap
		// to; writing anything here would undo the Defaults reset one box at a time.
		expect(snapGateInput('', 0.4)).toBeNull();
	});

	it('leaves a box holding only whitespace alone', () => {
		// The trim that keeps `parseGate` from reading a stray space as an explicit 0
		// has to reach the snap too, or a space would be snapped to '0' — the one
		// state that permanently disarms a shipped floor.
		expect(snapGateInput('  ', 0.4)).toBeNull();
	});

	it('leaves a box that already reads as its own parsed value alone', () => {
		// No write when nothing would change, so blurring an untouched box does not
		// churn the preference.
		expect(snapGateInput('0.5', 0.5)).toBeNull();
	});

	it('tidies a box whose text is a different spelling of the same number', () => {
		// '0.50' and 0.5 are one floor, and the box should say so once the reader has
		// moved on.
		expect(snapGateInput('0.50', 0.5)).toBe('0.5');
	});

	it('snaps a negative box to the off it actually runs at', () => {
		// `parseGate` reads a negative as the gate turned off, and off is a state the
		// reader can honestly be shown — unlike the default's digits, which they
		// never asked for.
		expect(snapGateInput('-5', 0.4)).toBe('0');
	});
});

describe('movedGates', () => {
	it('counts nothing on a fresh install, so the collapsed row badges nothing', () => {
		// The badge exists to say the table is not showing this build's answer. An
		// untouched install IS that answer, armed floor included, so a count that
		// read 1 here would badge every reader on first launch.
		expect(movedGates(parseGates(gateInputs()))).toEqual([]);
	});

	it('counts a knob the reader armed', () => {
		expect(movedGates(parseGates(gateInputs({ minTurnover: '10000' })))).toEqual(['minTurnover']);
	});

	it('counts the trash-price floor as moved when the reader turns it off', () => {
		// Moved, not armed — POE-196 split those. Disarming the shipped floor
		// changes what the table shows as surely as arming a knob does, so the
		// reader gets the same badge pointing at the same row.
		expect(movedGates(parseGates(gateInputs({ minItemPrice: '0' })))).toEqual(['minItemPrice']);
	});

	it('counts the divine trash-price floor as moved when the reader turns it off', () => {
		// Its own case rather than a variant of the chaos one: `movedGates` is keyed
		// off `gateDefaults`, so this is what catches a knob added to the type with a
		// default of 0 — it would then be indistinguishable from a gate that ships
		// off and would badge nothing when disarmed.
		expect(movedGates(parseGates(gateInputs({ minItemPriceDiv: '0' })))).toEqual([
			'minItemPriceDiv'
		]);
	});

	it('does not count a knob re-typed at the level it already ships at', () => {
		// Compared on the parsed numbers, not the strings: '' and '0.5' are one
		// floor, and a count over text would badge a reader who typed the default
		// back in.
		expect(movedGates(parseGates(gateInputs({ minItemPrice: '0.5' })))).toEqual([]);
	});
});

describe('resetGateInputs', () => {
	it('restores the shipped state, not a blank bar', () => {
		// What the Gates row's Defaults button writes into the preferences. Every
		// box gets '' and never the number it stands for, because '' means whatever
		// this build ships — so the reset must land the five off AND both price
		// floors back at 0.5c and 0.4 div. A reset that wrote '0' would look
		// identical for the five and would quietly hand the reader the trash tier
		// for good.
		expect(parseGates(resetGateInputs())).toEqual(gateDefaults);
	});

	it('leaves no knob counted as moved', () => {
		// The badge's end of the same contract: after Defaults the collapsed row is
		// bare, which is only true if the reset agreed with `gateDefaults` on all
		// seven.
		expect(movedGates(parseGates(resetGateInputs()))).toEqual([]);
	});
});

describe('exchangePageParams', () => {
	const BLANK_GATES: GateInputs = {
		minItemPrice: '',
		minItemPriceDiv: '',
		minRoiChaos: '',
		minTurnover: '',
		maxTickPct: '',
		minEdgeTickRatio: '',
		minRoiPct: ''
	};

	/** A fresh install's persisted picks: every pref at the string the page stores by default. */
	function prefs(overrides: Partial<ExchangePagePrefs> = {}): ExchangePagePrefs {
		return {
			sort: 'expected',
			rowCap: '50',
			categoryRules: '{}',
			itemRules: '[]',
			gateInputs: BLANK_GATES,
			investMin: '',
			investMax: '',
			unit: 'chaos',
			search: '',
			...overrides
		};
	}

	it('sends every gate explicitly, including defaults', () => {
		// The server never defaults a gate (ADR-017, M1): a blank knob must reach
		// it as this build's shipped number, and an armed one as what was typed.
		const params = exchangePageParams(prefs({ gateInputs: { ...BLANK_GATES, minRoiChaos: '5', maxTickPct: '-3' } }));

		expect({
			minItemPrice: params.minItemPrice,
			minItemPriceDiv: params.minItemPriceDiv,
			minRoiChaos: params.minRoiChaos,
			minTurnover: params.minTurnover,
			maxTickPct: params.maxTickPct,
			minEdgeTickRatio: params.minEdgeTickRatio,
			minRoiPct: params.minRoiPct
		}).toEqual({
			minItemPrice: '0.5',
			minItemPriceDiv: '0.4',
			minRoiChaos: '5',
			minTurnover: '0',
			maxTickPct: '0',
			minEdgeTickRatio: '0',
			minRoiPct: '0'
		});
	});

	it('maps an item rule and a category rule to their params', () => {
		const params = exchangePageParams(
			prefs({
				categoryRules: serializeCategoryRules({ Currency: 'only', Omens: 'hide' }),
				itemRules: serializeItemRules([
					{ id: 'item-hidden', name: 'Hidden', state: 'hide' },
					{ id: 'item-kept', name: 'Kept', state: 'only' }
				])
			})
		);

		expect([params.onlyCategory, params.hideCategory, params.onlyItem, params.hideItem]).toEqual([
			['Currency'],
			['Omens'],
			['item-kept'],
			['item-hidden']
		]);
	});

	it('sends no rule param when no rule is set', () => {
		const params = exchangePageParams(prefs());

		expect(['onlyCategory', 'hideCategory', 'onlyItem', 'hideItem'].filter((key) => key in params)).toEqual([]);
	});

	it('sends divine bounds with unit divine', () => {
		const params = exchangePageParams(prefs({ investMin: '1', investMax: '2.5', unit: 'divine' }));

		expect([params.investMin, params.investMax, params.unit]).toEqual(['1', '2.5', 'divine']);
	});

	it('omits a blank or half-typed bound rather than sending it', () => {
		// A half-typed "1e" must not filter the table while the exponent is typed.
		const params = exchangePageParams(prefs({ investMin: '  ', investMax: '1e' }));

		expect(['investMin', 'investMax'].filter((key) => key in params)).toEqual([]);
	});

	it('maps Show all to pageSize=all', () => {
		expect(exchangePageParams(prefs({ rowCap: 'all' })).pageSize).toBe('all');
	});

	it('maps a numeric row cap to its page size', () => {
		expect(exchangePageParams(prefs({ rowCap: '25' })).pageSize).toBe('25');
	});

	it('sends the parsed sort', () => {
		expect([
			exchangePageParams(prefs({ sort: 'fastest' })).sort,
			exchangePageParams(prefs({ sort: 'bogus' })).sort
		]).toEqual(['fastest', 'expected']);
	});

	it('sends a trimmed search and omits a blank one', () => {
		expect([
			exchangePageParams(prefs({ search: '  Omen ' })).search,
			'search' in exchangePageParams(prefs({ search: '   ' }))
		]).toEqual(['Omen', false]);
	});
});
