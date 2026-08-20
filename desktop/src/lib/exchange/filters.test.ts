import { describe, it, expect } from 'vitest';
import {
	applyNumericFilters,
	applyRules,
	cycleCategoryRule,
	itemUniverse,
	overDepth,
	parseCategoryRules,
	parseItemRules,
	playSides,
	serializeCategoryRules,
	serializeItemRules
} from './filters';
import type { CategoryRules, ItemRule, NumericFilters } from './filters';
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

/** A ranked play over one leg. */
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
		quantity: 1,
		investMin: '',
		investMax: '',
		unit: 'chaos',
		divineChaosRate: 198.97,
		minRoiPct: '',
		minGain: '',
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

describe('applyNumericFilters', () => {
	const cheap = play({ key: 'cheap', investment: 100, roi: 10, roiPct: 0.1 });
	const dear = play({ key: 'dear', investment: 250, roi: 25, roiPct: 0.1 });

	it('keeps every play when no bound is typed', () => {
		const loss = play({ key: 'loss', investment: 0, roi: -5, roiPct: -0.5 });

		expect(keys(applyNumericFilters([cheap, loss], filters()))).toEqual(['cheap', 'loss']);
	});

	it('ignores a half-typed bound rather than emptying the table', () => {
		expect(keys(applyNumericFilters([cheap], filters({ investMin: '1e' })))).toEqual(['cheap']);
	});

	it('ignores a whitespace-only bound rather than reading it as zero', () => {
		// `Number('  ')` is 0, so without the trim a space left in Min ROI%
		// would become a 0% floor and silently drop every negative-ROI play.
		const loss = play({ key: 'loss', investment: 10, roi: -5, roiPct: -0.5 });

		expect(keys(applyNumericFilters([loss], filters({ minRoiPct: '  ' })))).toEqual(['loss']);
	});

	it('ignores an infinite bound rather than emptying the table', () => {
		// `persisted()` hands back whatever is in storage, so the read site is the
		// only place a bound of "Infinity" — which no play can ever clear — stops.
		expect(keys(applyNumericFilters([cheap], filters({ investMin: 'Infinity' })))).toEqual([
			'cheap'
		]);
	});

	it('measures the investment floor against the whole run, not one exchange', () => {
		// One exchange costs 100c, five cost 500c — the reader set the quantity to
		// say how many they intend to run, so the bound is a question about the run.
		expect(keys(applyNumericFilters([cheap], filters({ quantity: 5, investMin: '300' })))).toEqual(
			['cheap']
		);
	});

	it('keeps a run sitting exactly on the investment floor', () => {
		expect(keys(applyNumericFilters([cheap], filters({ quantity: 5, investMin: '500' })))).toEqual(
			['cheap']
		);
	});

	it('drops a run one chaos under the investment floor', () => {
		expect(keys(applyNumericFilters([cheap], filters({ quantity: 5, investMin: '501' })))).toEqual(
			[]
		);
	});

	it('keeps a run sitting exactly on the investment ceiling', () => {
		expect(keys(applyNumericFilters([cheap], filters({ quantity: 5, investMax: '500' })))).toEqual(
			['cheap']
		);
	});

	it('drops a run one chaos over the investment ceiling', () => {
		expect(keys(applyNumericFilters([cheap], filters({ quantity: 5, investMax: '499' })))).toEqual(
			[]
		);
	});

	it('reads the investment bounds as divine when the unit says divine', () => {
		// A ceiling of 1 divine at 200c/div is a 200c ceiling: the 100c play fits
		// and the 250c one does not.
		const kept = applyNumericFilters(
			[cheap, dear],
			filters({ unit: 'divine', divineChaosRate: 200, investMax: '1' })
		);

		expect(keys(kept)).toEqual(['cheap']);
	});

	it('reads the bounds as chaos when the hour carried no divine trade', () => {
		// divineChaosRate 0 means the rate is unknown, not that a divine is worth
		// nothing — converting by it would give a 0c ceiling and empty the table.
		const kept = applyNumericFilters(
			[cheap, dear],
			filters({ unit: 'divine', divineChaosRate: 0, investMax: '200' })
		);

		expect(keys(kept)).toEqual(['cheap']);
	});

	it('measures the minimum gain across the whole quantity', () => {
		// 10c per exchange clears a 40c floor only because five exchanges are run.
		expect(keys(applyNumericFilters([cheap], filters({ quantity: 5, minGain: '40' })))).toEqual([
			'cheap'
		]);
	});

	it('keeps a run gaining exactly the minimum', () => {
		expect(keys(applyNumericFilters([cheap], filters({ quantity: 5, minGain: '50' })))).toEqual([
			'cheap'
		]);
	});

	it('drops a run one chaos short of the minimum gain', () => {
		expect(keys(applyNumericFilters([cheap], filters({ quantity: 5, minGain: '51' })))).toEqual([]);
	});

	it('leaves the minimum gain in chaos when the investment bounds are typed in divine', () => {
		// The unit toggle sits with the investment inputs; the gain input is chaos
		// by definition, and converting it would raise a 50c floor to 10,000c.
		const kept = applyNumericFilters(
			[dear],
			filters({ unit: 'divine', divineChaosRate: 200, minGain: '25' })
		);

		expect(keys(kept)).toEqual(['dear']);
	});

	it('reads the minimum ROI input as percent points against a fractional roiPct', () => {
		// The input says 5 and the wire says 0.1 — a filter that compared them raw
		// would drop every play on the table.
		expect(keys(applyNumericFilters([cheap], filters({ minRoiPct: '5' })))).toEqual(['cheap']);
	});

	it('keeps a play returning exactly the minimum ROI', () => {
		expect(keys(applyNumericFilters([cheap], filters({ minRoiPct: '10' })))).toEqual(['cheap']);
	});

	it('drops a play returning just under the minimum ROI', () => {
		expect(keys(applyNumericFilters([cheap], filters({ minRoiPct: '10.1' })))).toEqual([]);
	});

	it('leaves the minimum ROI unmoved by the quantity', () => {
		// A percentage is per exchange whatever the run size; scaling it would make
		// the filter tighten every time the reader stepped the quantity up.
		expect(keys(applyNumericFilters([cheap], filters({ quantity: 5, minRoiPct: '10' })))).toEqual([
			'cheap'
		]);
	});
});

describe('overDepth', () => {
	const shallow = play({ depth: 40 });

	it('reports a quantity above the depth of that hour as over it', () => {
		expect(overDepth(shallow, 41)).toBe(true);
	});

	it('reports a quantity equal to the depth as within it', () => {
		// 40 units changed hands in that hour, so 40 is a quantity the book
		// demonstrably supported.
		expect(overDepth(shallow, 40)).toBe(false);
	});

	it('reports a quantity below the depth as within it', () => {
		expect(overDepth(shallow, 1)).toBe(false);
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
