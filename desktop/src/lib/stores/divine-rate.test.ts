/**
 * The top-bar divine rate's pure half (POE-284): normalization, the state
 * derivation, the formatters and the three tooltip texts. Every expected string
 * is a literal from the design handoff (`design/handoffs/poe-284--…/README.md`,
 * `fixtures.json` `_formats`) or measured with node 24 on 2026-10-10 using the
 * same Intl/Number calls — never a re-call of the helper under test.
 */
import { describe, expect, it } from 'vitest';
import {
	deriveDivineRateState,
	divineRateView,
	formatAge,
	formatChipRate,
	formatClock,
	formatPriceHour,
	formatTooltipRate,
	normalizeDivineRate,
	type DivineRateSnapshot
} from './divine-rate';
import { readDivineRateFixture } from './divine-rate-fixture';

const NOW = Date.parse('2026-09-10T00:00:00Z');
const MIN = 60_000;

const LINE_3 = '<div style="color:var(--color-lab-text-secondary)">Every chaos ↔ divine figure in the app uses this rate.</div>';

function snap(over: Partial<DivineRateSnapshot>): DivineRateSnapshot {
	return {
		divineChaosRate: 360.07,
		priceHour: '2026-09-09T23:00:00Z',
		updatedAt: new Date(NOW - 6 * MIN).toISOString(),
		state: 'ready',
		unreachableSince: null,
		...over
	};
}

describe('formatChipRate', () => {
	it('rounds the fixture rate to a whole chaos', () => {
		expect(formatChipRate(360.07)).toBe('360');
	});

	it('rounds a half up rather than flooring it', () => {
		expect(formatChipRate(359.5)).toBe('360');
	});

	it('prints 999.4 without grouping', () => {
		expect(formatChipRate(999.4)).toBe('999');
	});

	it('groups once rounding reaches 1,000', () => {
		expect(formatChipRate(999.5)).toBe('1,000');
	});

	it('groups a rate above 1,000', () => {
		expect(formatChipRate(1234.5)).toBe('1,235');
	});
});

describe('formatTooltipRate', () => {
	it('keeps one decimal of the fixture rate', () => {
		expect(formatTooltipRate(360.07)).toBe('360.1');
	});

	it('keeps the decimal and the grouping above 1,000', () => {
		expect(formatTooltipRate(1234.5)).toBe('1,234.5');
	});
});

describe('formatPriceHour', () => {
	it('prints the fixture hour as DD Mon HH:mm in UTC', () => {
		expect(formatPriceHour('2026-09-09T23:00:00Z', 'UTC')).toBe('09 Sep 23:00');
	});

	it('prints the hour in the zone it is given', () => {
		expect(formatPriceHour('2026-09-09T23:00:00Z', 'Europe/Warsaw')).toBe('10 Sep 01:00');
	});

	it('zero-pads a single-digit day and midnight', () => {
		expect(formatPriceHour('2026-01-05T00:00:00Z', 'UTC')).toBe('05 Jan 00:00');
	});
});

describe('formatClock', () => {
	it('prints HH:mm in the zone it is given', () => {
		expect(formatClock('2026-09-09T21:04:00Z', 'Europe/Warsaw')).toBe('23:04');
	});
});

describe('formatAge', () => {
	it('counts the fixture age in whole minutes', () => {
		expect(formatAge(new Date(NOW - 6 * MIN).toISOString(), NOW)).toBe('updated 6 min ago');
	});

	it('does not round 59 seconds up to a minute', () => {
		expect(formatAge(new Date(NOW - 59_000).toISOString(), NOW)).toBe('updated 0 min ago');
	});

	it('counts exactly 60 seconds as one minute', () => {
		expect(formatAge(new Date(NOW - 60_000).toISOString(), NOW)).toBe('updated 1 min ago');
	});

	it('never prints a negative age for a timestamp in the future', () => {
		expect(formatAge(new Date(NOW + 5 * MIN).toISOString(), NOW)).toBe('updated 0 min ago');
	});
});

describe('normalizeDivineRate', () => {
	it('stores a zero, negative or non-finite rate as null and keeps a positive one', () => {
		const port = { priceHour: null, updatedAt: '2026-09-09T23:06:00Z', state: 'ready' as const, unreachableSince: null };
		expect([0, -1, NaN, 360.07].map((r) => normalizeDivineRate({ ...port, divineChaosRate: r }).divineChaosRate))
			.toEqual([null, null, null, 360.07]);
	});
});

describe('normalizeDivineRate finite guard', () => {
	it('stores an infinite rate as null', () => {
		const port = { divineChaosRate: Infinity, priceHour: null, updatedAt: '2026-09-09T23:06:00Z', state: 'ready' as const, unreachableSince: null };
		expect(normalizeDivineRate(port).divineChaosRate).toBeNull();
	});
});

describe('deriveDivineRateState', () => {
	it('is cold without a rate even when the source says stale', () => {
		expect(deriveDivineRateState({ divineChaosRate: null, state: 'stale' })).toBe('cold');
	});

	it('is stale when a rate is held and the source says stale', () => {
		expect(deriveDivineRateState({ divineChaosRate: 360.07, state: 'stale' })).toBe('stale');
	});

	it('is ready when a rate is held and the source says ready', () => {
		expect(deriveDivineRateState({ divineChaosRate: 360.07, state: 'ready' })).toBe('ready');
	});
});

describe('divineRateView', () => {
	it('words the ready state as reference 01', () => {
		expect(divineRateView(snap({}), NOW, 'UTC')).toEqual({
			state: 'ready',
			chip: '360',
			tooltipHtml:
				'<div style="font-weight:700">1 divine = 360.1 chaos</div>' +
				'<div style="color:var(--color-lab-text-secondary)">Currency Exchange, hour of 09 Sep 23:00 · updated 6 min ago</div>' +
				LINE_3
		});
	});

	it('words the cold state as reference 02', () => {
		expect(divineRateView(snap({ divineChaosRate: null, priceHour: null, state: 'cold' }), NOW, 'UTC')).toEqual({
			state: 'cold',
			chip: '—',
			tooltipHtml:
				'<div style="font-weight:700">No divine rate yet</div>' +
				'<div style="color:var(--color-lab-text-secondary)">Waiting for the first Currency Exchange hour.</div>' +
				LINE_3
		});
	});

	it('prints neither 0 nor NaN anywhere in the cold state', () => {
		const view = divineRateView(snap({ divineChaosRate: null, priceHour: null, updatedAt: null, state: 'cold' }), NOW, 'UTC');
		expect(`${view.chip}${view.tooltipHtml}`).not.toMatch(/\b0\b|NaN/);
	});

	it('words the stale state as reference 03', () => {
		const stale = snap({ priceHour: '2026-09-09T21:00:00Z', state: 'stale', unreachableSince: '2026-09-09T21:04:00Z' });
		expect(divineRateView(stale, NOW, 'UTC')).toEqual({
			state: 'stale',
			chip: '360',
			tooltipHtml:
				'<div style="font-weight:700">1 divine = 360.1 chaos (stale)</div>' +
				'<div style="color:var(--color-lab-text-secondary)">Last known rate from 21:00; server unreachable since 21:04.</div>' +
				LINE_3
		});
	});
});

describe('readDivineRateFixture', () => {
	it('answers the fixture ready rate, hour and age by default', () => {
		expect(readDivineRateFixture(NOW)).toEqual({
			divineChaosRate: 360.07,
			priceHour: '2026-09-09T23:00:00Z',
			updatedAt: new Date(NOW - 6 * MIN).toISOString(),
			state: 'ready',
			unreachableSince: null
		});
	});

	it('answers no rate and no hour when asked for cold', () => {
		expect(readDivineRateFixture(NOW, 'cold')).toEqual(expect.objectContaining({
			divineChaosRate: null,
			priceHour: null,
			state: 'cold',
			unreachableSince: null
		}));
	});

	it('answers the stale example hour and outage start when asked for stale', () => {
		expect(readDivineRateFixture(NOW, 'stale')).toEqual(expect.objectContaining({
			divineChaosRate: 360.07,
			priceHour: '2026-09-09T21:00:00Z',
			state: 'stale',
			unreachableSince: '2026-09-09T21:04:00Z'
		}));
	});
});
