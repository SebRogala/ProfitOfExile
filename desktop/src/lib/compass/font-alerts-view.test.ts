import { describe, it, expect, beforeEach } from 'vitest';
import { setDivineRate, formatPrice } from '$lib/price.svelte';
import {
	ageLabel,
	alertPanels,
	exceptionalView,
	LOADING_SOURCE,
	LOADING_SUPPORT_NAMES,
	type ExceptionalSupportsPayload,
	type PriceCell,
} from './font-alerts-view';
import pricedJson from './__fixtures__/exceptional-supports/priced.json';
import missingJson from './__fixtures__/exceptional-supports/missing.json';

const priced = pricedJson as ExceptionalSupportsPayload;
const missing = missingJson as ExceptionalSupportsPayload;

// 6 min after the goldens' asOf (2026-09-09T00:33:00Z).
const NOW = Date.parse('2026-09-09T00:39:00Z');
const AS_OF = Date.parse('2026-09-09T00:33:00Z');
const TWO_HOURS = 2 * 60 * 60 * 1000;
const format = (chaos: number) => formatPrice(chaos);

function cellText(cell: PriceCell): string | undefined {
	return cell.kind === 'price' ? cell.text : undefined;
}

describe('exceptionalView', () => {
	beforeEach(() => setDivineRate(0));

	it('formats each priced gem in chaos below two divines', () => {
		const view = exceptionalView(priced, NOW, format);
		expect(view.rows.map((r) => cellText(r.cell))).toEqual(['9c', '185c', '43c']);
	});

	it('renders a price from two divines up in divines', () => {
		setDivineRate(50);
		const view = exceptionalView(priced, NOW, format);
		expect(view.rows[1].cell).toEqual({ kind: 'price', text: '3.7d' });
	});

	it('keeps a gem with no price as a "no price" row', () => {
		const view = exceptionalView(missing, NOW, format);
		expect(view.rows[1]).toEqual({ name: 'Enlighten Support', cell: { kind: 'missing' } });
	});

	it('keeps the priced rows beside a missing one', () => {
		const view = exceptionalView(missing, NOW, format);
		expect([cellText(view.rows[0].cell), cellText(view.rows[2].cell)]).toEqual(['9c', '43c']);
	});

	it('shows every row as loading before the first payload', () => {
		const view = exceptionalView(null, NOW, format);
		expect(view.rows.map((r) => r.cell.kind)).toEqual(['loading', 'loading', 'loading']);
		expect(view.rows.map((r) => r.name)).toEqual([
			'Enhance Support',
			'Enlighten Support',
			'Empower Support',
		]);
	});

	it('drops the age while loading', () => {
		const view = exceptionalView(null, NOW, format);
		expect(view.age).toBeNull();
		expect(view.footer).toBe('poe.ninja · level 1, uncorrupted');
	});

	it("keeps the server's row order whatever the prices", () => {
		const view = exceptionalView(priced, NOW, format);
		expect(view.rows.map((r) => r.name)).toEqual([
			'Enhance Support',
			'Enlighten Support',
			'Empower Support',
		]);
	});

	it('is fresh one millisecond before two hours', () => {
		const view = exceptionalView(priced, AS_OF + TWO_HOURS - 1, format);
		expect(view.age?.stale).toBe(false);
	});

	it('turns stale at two hours', () => {
		const view = exceptionalView(priced, AS_OF + TWO_HOURS, format);
		expect(view.age).toEqual({ text: '2h ago', stale: true });
	});

	it('has no age for a payload without a snapshot time', () => {
		const view = exceptionalView({ ...priced, asOf: null }, NOW, format);
		expect(view.age).toBeNull();
	});
});

describe('ageLabel', () => {
	it.each([
		[30_000, '<1m'],
		[6 * 60_000, '6m ago'],
		[59 * 60_000 + 59_000, '59m ago'],
		[60 * 60_000, '1h ago'],
		[3 * 60 * 60_000, '3h ago'],
	])('labels an age of %i ms as %s', (ageMs, label) => {
		expect(ageLabel(ageMs)).toBe(label);
	});
});

describe('loading placeholders', () => {
	it("the loading names are the server's names", () => {
		expect(LOADING_SUPPORT_NAMES).toEqual(priced.gems.map((g) => g.name));
		expect(LOADING_SOURCE).toEqual(priced.source);
	});
});

describe('alertPanels', () => {
	it('maps both alert crafts to their panels in order', () => {
		expect(alertPanels(['exchange_exceptional', 'transform_direct'])).toEqual([
			'exceptional',
			'transfigure',
		]);
	});

	it('ignores craft types that are not alerts', () => {
		expect(alertPanels(['transform_random'])).toEqual([]);
	});
});
