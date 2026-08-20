import { describe, it, expect } from 'vitest';
import {
	HORIZON_OPTIONS,
	MODE_OPTIONS,
	REFETCH_DEBOUNCE_MS,
	REFETCH_JITTER_MS,
	dataAgeParts,
	deriveState,
	formatLegPrice,
	formatRoiPct,
	formatTime,
	formatTimeAgo,
	formatVolume,
	iconSrc,
	legLabel,
	parseDensity,
	parseHorizon,
	parseMode,
	parseQuantity,
	parseSort,
	parseUnit,
	refetchDelay,
	sortPlays
} from './view';
import type {
	CurrencyExchangeLeg,
	CurrencyExchangePlay,
	CurrencyExchangeResponse
} from '$lib/api';

/**
 * A warm response with no plays. Every `deriveState` case overrides only the
 * field it is about, so a test that reads as "warm + error" cannot be quietly
 * carrying a second difference.
 */
function response(overrides: Partial<CurrencyExchangeResponse> = {}): CurrencyExchangeResponse {
	return {
		league: 'Mirage',
		lastUpdated: '2026-08-19T12:00:00.000Z',
		from: '2026-08-18T12:00:00.000Z',
		to: '2026-08-19T12:00:00.000Z',
		hours: 24,
		warm: true,
		mode: 'all',
		horizon: 'recent',
		divineChaosRate: 198.97,
		count: 0,
		plays: [],
		// The full sixteen, as the wire guarantees on every served body — a
		// shorter stand-in would let a category-filter test pass against a
		// universe the server never sends.
		categories: [
			'Currency',
			'Essences',
			'Delve',
			'Scarabs',
			'Divination Cards',
			'Delirium',
			'Legion',
			'Fragments',
			'Oils',
			'Catalysts',
			'Omens',
			'Tattoos',
			'Expedition',
			'Harvest',
			'Runegrafts',
			'Allflame'
		],
		...overrides
	};
}

/**
 * A clean, ranked play. `sortPlays` reads only `roi` and `suspect`, so every
 * sort case overrides one of those and the `key` it is identified by.
 */
function play(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
	return {
		key: 'direct:divine:chaos',
		mode: 'direct',
		legs: [],
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

describe('parseMode', () => {
	it('passes "all" through', () => {
		expect(parseMode('all')).toBe('all');
	});

	it('passes "direct" through', () => {
		expect(parseMode('direct')).toBe('direct');
	});

	it('passes "1-hop" through', () => {
		expect(parseMode('1-hop')).toBe('1-hop');
	});

	it('falls back to "all" for a mode the server would reject with a 400', () => {
		expect(parseMode('bogus')).toBe('all');
	});

	it('falls back to "all" for an unset preference', () => {
		// `persisted()` hands back the raw stored string, so a preference written
		// by an older build (or never written at all) arrives here as "".
		expect(parseMode('')).toBe('all');
	});
});

describe('MODE_OPTIONS', () => {
	it('offers the three server modes, labelled, in picker order', () => {
		// The values are wire values: SegmentedButtons hands the selected one
		// straight to fetchCurrencyExchangePlays, so a prettified value ("1 hop")
		// would be a 400 rather than a cosmetic slip.
		expect(MODE_OPTIONS).toEqual([
			{ value: 'all', label: 'All' },
			{ value: 'direct', label: 'Direct' },
			{ value: '1-hop', label: '1-hop' }
		]);
	});
});

describe('parseHorizon', () => {
	it('passes "recent" through', () => {
		expect(parseHorizon('recent')).toBe('recent');
	});

	it('passes "day" through', () => {
		expect(parseHorizon('day')).toBe('day');
	});

	it('falls back to "recent" for a horizon the server would reject with a 400', () => {
		expect(parseHorizon('week')).toBe('recent');
	});

	it('falls back to "recent" for an unset preference', () => {
		// The window the page fetched before the toggle existed, so a preference
		// written by an older build resolves to what that build already showed.
		expect(parseHorizon('')).toBe('recent');
	});
});

describe('HORIZON_OPTIONS', () => {
	it('offers the two server horizons, labelled with their window length', () => {
		// Wire values: the selected one goes to fetchCurrencyExchangePlays as a
		// query param, so a prettified value would be a 400 rather than a slip.
		expect(HORIZON_OPTIONS).toEqual([
			{ value: 'recent', label: 'Recent 6h' },
			{ value: 'day', label: 'Day 24h' }
		]);
	});
});

describe('parseSort', () => {
	it('passes "roiPct" through', () => {
		expect(parseSort('roiPct')).toBe('roiPct');
	});

	it('passes "roi" through', () => {
		expect(parseSort('roi')).toBe('roi');
	});

	it('falls back to "roiPct" for an unknown sort', () => {
		expect(parseSort('turnover')).toBe('roiPct');
	});

	it('falls back to "roiPct" for an unset preference', () => {
		// The server's own ranking is the default order, so an unset preference
		// leaves the list exactly as served.
		expect(parseSort('')).toBe('roiPct');
	});
});

describe('parseDensity', () => {
	it('passes "comfortable" through', () => {
		expect(parseDensity('comfortable')).toBe('comfortable');
	});

	it('passes "dense" through', () => {
		expect(parseDensity('dense')).toBe('dense');
	});

	it('falls back to "comfortable" for an unknown density', () => {
		expect(parseDensity('compact')).toBe('comfortable');
	});

	it('falls back to "comfortable" for an unset preference', () => {
		expect(parseDensity('')).toBe('comfortable');
	});
});

describe('parseUnit', () => {
	it('passes "chaos" through', () => {
		expect(parseUnit('chaos')).toBe('chaos');
	});

	it('passes "divine" through', () => {
		expect(parseUnit('divine')).toBe('divine');
	});

	it('falls back to "chaos" for a currency the bounds cannot be converted to', () => {
		expect(parseUnit('exalted')).toBe('chaos');
	});

	it('falls back to "chaos" for an unset preference', () => {
		// Chaos is what every wire number is denominated in, so it is the one unit
		// that stays readable when divineChaosRate is 0.
		expect(parseUnit('')).toBe('chaos');
	});
});

describe('parseQuantity', () => {
	it('passes a whole count through', () => {
		expect(parseQuantity('12')).toBe(12);
	});

	it('falls back to one for an unset preference', () => {
		expect(parseQuantity('')).toBe(1);
	});

	it('truncates a fractional count rather than claiming a partial exchange', () => {
		expect(parseQuantity('2.9')).toBe(2);
	});

	it('falls back to one for a zero, which would flatten every figure to nothing', () => {
		expect(parseQuantity('0')).toBe(1);
	});

	it('falls back to one for a negative count', () => {
		expect(parseQuantity('-4')).toBe(1);
	});

	it('falls back to one for a count that is not a number at all', () => {
		expect(parseQuantity('lots')).toBe(1);
	});

	it('falls back to one for an infinite count', () => {
		expect(parseQuantity('Infinity')).toBe(1);
	});
});

describe('deriveState', () => {
	const now = new Date('2026-08-19T12:00:00.000Z');

	it('reports loading while the first fetch is still in flight', () => {
		expect(deriveState({ result: null, lastFetchedAt: null, lastError: null, now })).toEqual({
			kind: 'loading'
		});
	});

	it('reports unreachable when the first fetch failed and there is nothing to show', () => {
		expect(
			deriveState({ result: null, lastFetchedAt: null, lastError: 'connection refused', now })
		).toEqual({ kind: 'unreachable' });
	});

	it('reports stale with the age of the last good fetch when a later fetch failed', () => {
		const state = deriveState({
			result: response(),
			lastFetchedAt: new Date('2026-08-19T11:58:00.000Z'),
			lastError: 'connection refused',
			now
		});

		expect(state).toEqual({ kind: 'stale', staleSince: '2 min ago' });
	});

	it('dates a stale result from now when no fetch time was recorded', () => {
		// Not a real path today, but the fallback is what keeps the sentence from
		// reading "stale since  — server unreachable" if one appears.
		const state = deriveState({
			result: response(),
			lastFetchedAt: null,
			lastError: 'connection refused',
			now
		});

		expect(state).toEqual({ kind: 'stale', staleSince: 'just now' });
	});

	it('reports warming while the server has not closed a Currency Exchange hour', () => {
		expect(
			deriveState({ result: response({ warm: false }), lastFetchedAt: now, lastError: null, now })
		).toEqual({ kind: 'warming' });
	});

	it('reports stale rather than warming when a cold result is also out of date', () => {
		// An error outranks warming: "waiting for the first hour" would hide the
		// fact that the last request never landed.
		const state = deriveState({
			result: response({ warm: false }),
			lastFetchedAt: new Date('2026-08-19T11:00:00.000Z'),
			lastError: 'connection refused',
			now
		});

		expect(state).toEqual({ kind: 'stale', staleSince: '1 h ago' });
	});

	it('reports ready with the age of the server-side computation', () => {
		// `updatedAgo` comes from result.lastUpdated (when the server computed the
		// hour), not from lastFetchedAt (when this client asked) — the two differ
		// by up to a full hour and only the first is what the header claims.
		const state = deriveState({
			result: response({ lastUpdated: '2026-08-19T09:00:00.000Z' }),
			lastFetchedAt: new Date('2026-08-19T11:59:59.000Z'),
			lastError: null,
			now
		});

		expect(state).toEqual({ kind: 'ready', updatedAgo: '3 h ago' });
	});

	it('omits the age on a ready result the server could not timestamp', () => {
		const state = deriveState({
			result: response({ lastUpdated: null }),
			lastFetchedAt: now,
			lastError: null,
			now
		});

		expect(state).toEqual({ kind: 'ready', updatedAgo: undefined });
	});
});

describe('formatTimeAgo', () => {
	const now = new Date('2026-08-19T12:00:00.000Z');

	function ago(ms: number): string {
		return formatTimeAgo(new Date(now.getTime() - ms), now);
	}

	it('says "just now" for the current instant', () => {
		expect(ago(0)).toBe('just now');
	});

	it('says "just now" one second before the first minute closes', () => {
		expect(ago(59_000)).toBe('just now');
	});

	it('switches to minutes exactly on the first minute', () => {
		expect(ago(60_000)).toBe('1 min ago');
	});

	it('switches to hours exactly on the first hour', () => {
		expect(ago(60 * 60_000)).toBe('1 h ago');
	});

	it('reports whole hours below a day', () => {
		expect(ago(2 * 60 * 60_000)).toBe('2 h ago');
	});

	it('switches to days exactly on the first day', () => {
		expect(ago(24 * 60 * 60_000)).toBe('1 d ago');
	});

	it('reads an ISO string as well as a Date', () => {
		// deriveState passes result.lastUpdated (a wire string) for `ready` and a
		// Date for `stale`; both go through this one function.
		expect(formatTimeAgo('2026-08-19T09:30:00.000Z', now)).toBe('2 h ago');
	});

	it('renders nothing for a missing timestamp', () => {
		expect(formatTimeAgo(null, now)).toBe('');
	});

	it('renders nothing rather than "NaN min ago" for an unparseable timestamp', () => {
		expect(formatTimeAgo('not-a-date', now)).toBe('');
	});

	it('reads a timestamp ahead of the local clock as "just now", not a negative age', () => {
		expect(formatTimeAgo(new Date(now.getTime() + 5 * 60_000), now)).toBe('just now');
	});
});

describe('formatTime', () => {
	it('renders a local 24-hour clock reading', () => {
		expect(formatTime(new Date(2026, 7, 19, 14, 35).toISOString())).toBe('14:35');
	});

	it('zero-pads both the hour and the minute', () => {
		expect(formatTime(new Date(2026, 7, 19, 9, 5).toISOString())).toBe('09:05');
	});

	it('renders midnight as 00:00 rather than 24:00 or 12:00', () => {
		expect(formatTime(new Date(2026, 7, 19, 0, 0).toISOString())).toBe('00:00');
	});

	it('renders nothing for a missing timestamp', () => {
		expect(formatTime(null)).toBe('');
	});

	it('renders nothing rather than "NaN:NaN" for an unparseable timestamp', () => {
		expect(formatTime('not-a-date')).toBe('');
	});
});

describe('formatRoiPct', () => {
	it('renders a positive return as a signed percentage with one decimal', () => {
		// The wire value is a FRACTION — 0.1234 is 12.34%, not 0.12% — so the
		// formatter multiplies. Pointing it at a gem roiPct (already percentage
		// points) would inflate the number a hundredfold.
		expect(formatRoiPct(0.1234)).toBe('+12.3%');
	});

	it('renders a negative return with a minus sign', () => {
		expect(formatRoiPct(-0.05)).toBe('-5.0%');
	});

	it('renders a zero return as +0.0%', () => {
		expect(formatRoiPct(0)).toBe('+0.0%');
	});

	it('renders a return that rounds away to nothing as +0.0%, never -0.0%', () => {
		expect(formatRoiPct(-0.00001)).toBe('+0.0%');
	});
});

describe('dataAgeParts', () => {
	// Built from local components, like the formatTime cases: formatTime reads a
	// local clock, so an ISO literal would assert a different hour per timezone.
	const hourEnd = new Date(2026, 7, 19, 11, 0);
	const computedAt = new Date(2026, 7, 19, 11, 45);
	const now = new Date(2026, 7, 19, 12, 0);

	it('dates the badge from the end of the settled hour, not from the ranking', () => {
		// The feed publishes 40-60 min after an hour closes, so `to` and
		// `lastUpdated` differ by most of an hour — and only `to` answers "how old
		// are these prices", which is what the badge claims.
		expect(
			dataAgeParts(
				response({ to: hourEnd.toISOString(), lastUpdated: computedAt.toISOString() }),
				now
			)
		).toEqual({ label: 'as of 11:00', ago: '1 h ago' });
	});

	it('falls back to the ranking time for a body served without a window', () => {
		expect(
			dataAgeParts(response({ to: null, lastUpdated: computedAt.toISOString() }), now)
		).toEqual({ label: 'as of 11:45', ago: '15 min ago' });
	});

	it('renders no badge for a body carrying neither timestamp', () => {
		expect(dataAgeParts(response({ to: null, lastUpdated: null }), now)).toBeNull();
	});

	it('renders no badge rather than "as of " for a timestamp that will not parse', () => {
		expect(dataAgeParts(response({ to: 'not-a-date' }), now)).toBeNull();
	});

	it('renders no badge before the first fetch has landed', () => {
		expect(dataAgeParts(null, now)).toBeNull();
	});
});

describe('sortPlays', () => {
	function keys(plays: CurrencyExchangePlay[]): string[] {
		return plays.map((p) => p.key);
	}

	it('leaves the server ranking untouched under the ROI% sort', () => {
		// The served order already carries roiPct desc plus turnover, direct-first
		// and key tie-breaks; re-sorting on roiPct alone would discard them.
		const served = [
			play({ key: 'a', roi: 5 }),
			play({ key: 'b', roi: 500 }),
			play({ key: 'c', roi: 50 })
		];

		expect(keys(sortPlays(served, 'roiPct'))).toEqual(['a', 'b', 'c']);
	});

	it('orders by chaos gained per exchange under the ROI sort', () => {
		const served = [
			play({ key: 'a', roi: 5 }),
			play({ key: 'b', roi: 500 }),
			play({ key: 'c', roi: 50 })
		];

		expect(keys(sortPlays(served, 'roi'))).toEqual(['b', 'c', 'a']);
	});

	it('keeps a suspect play behind every clean one even when its ROI is the largest', () => {
		// The flag is the reason the server ranks it last: its price sits outside
		// the fair band, so the big number is the part not to trust.
		const served = [
			play({ key: 'clean-small', roi: 5 }),
			play({ key: 'clean-big', roi: 50 }),
			play({ key: 'suspect-huge', roi: 5000, suspect: true })
		];

		expect(keys(sortPlays(served, 'roi'))).toEqual(['clean-big', 'clean-small', 'suspect-huge']);
	});

	it('keeps the server order between two plays tied on ROI', () => {
		const served = [play({ key: 'first', roi: 40 }), play({ key: 'second', roi: 40 })];

		expect(keys(sortPlays(served, 'roi'))).toEqual(['first', 'second']);
	});

	it('sorts into a new array rather than reordering the fetched list', () => {
		// The page holds the fetched list in reactive state and re-derives the sort
		// from it; an in-place sort would make the ROI% option unable to restore
		// the server ranking without a refetch.
		const served = [play({ key: 'a', roi: 5 }), play({ key: 'b', roi: 500 })];

		sortPlays(served, 'roi');

		expect(keys(served)).toEqual(['a', 'b']);
	});

	it("hands out a copy under the ROI% sort too, never the caller's own array", () => {
		// The ROI% branch keeps the served order but must not alias the
		// response's array: the page mutating the sorted list (or Svelte state
		// wrapping it) would otherwise write through to the fetched result.
		const served = [play({ key: 'a' }), play({ key: 'b' })];

		const sorted = sortPlays(served, 'roiPct');

		expect(sorted).not.toBe(served);
		expect(keys(sorted)).toEqual(['a', 'b']);
	});
});

describe('formatVolume', () => {
	it('renders zero volume as "0"', () => {
		expect(formatVolume(0)).toBe('0');
	});

	it('renders a two-digit volume verbatim', () => {
		expect(formatVolume(42)).toBe('42');
	});

	it('renders the largest un-abbreviated volume verbatim', () => {
		expect(formatVolume(999)).toBe('999');
	});

	it('abbreviates a thousand with one decimal', () => {
		expect(formatVolume(1234)).toBe('1.2k');
	});

	it('abbreviates a million with one decimal', () => {
		expect(formatVolume(13_001_051)).toBe('13.0M');
	});

	it('rounds a fractional volume rather than printing its decimals', () => {
		expect(formatVolume(41.6)).toBe('42');
	});
});

describe('formatLegPrice', () => {
	it('drops the decimals on a price of 100 or more', () => {
		expect(formatLegPrice(196)).toBe('196');
	});

	it('keeps two decimals just below the hundreds threshold', () => {
		// The threshold is on the value, not on its rounded form: 99.999 is still
		// a two-decimal price, so it prints "100.00" rather than "100".
		expect(formatLegPrice(99.999)).toBe('100.00');
	});

	it('keeps two decimals on a sub-unit price with one significant digit', () => {
		expect(formatLegPrice(0.5)).toBe('0.50');
	});

	it('keeps four significant digits on a tenths-scale price', () => {
		expect(formatLegPrice(0.142857)).toBe('0.1429');
	});

	it('keeps four significant digits past the leading zeros of a tiny price', () => {
		// A fragment priced in divine: two decimals would print "0.00" and lose
		// the whole quantity.
		expect(formatLegPrice(0.004975)).toBe('0.004975');
	});

	it('renders a zero price with the two-decimal floor', () => {
		expect(formatLegPrice(0)).toBe('0.00');
	});

	it('renders a non-finite price as "0" rather than "NaN"', () => {
		expect(formatLegPrice(Number.NaN)).toBe('0');
	});
});

describe('legLabel', () => {
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
			itemName: 'Mod Values',
			itemIcon: null,
			itemCategory: 'Currency',
			quoteName: 'Reroll Rare',
			quoteIcon: null,
			quoteCategory: 'Currency',
			...overrides
		};
	}

	it('words a buy leg as buying the item with the quote currency', () => {
		expect(legLabel(leg())).toBe('buy Mod Values with Reroll Rare @ 196');
	});

	it('words a sell leg as selling the item for the quote currency', () => {
		expect(legLabel(leg({ action: 'sell' }))).toBe('sell Mod Values for Reroll Rare @ 196');
	});

	it('names the display names, not the raw exchange ids', () => {
		// The ids stay on the row's `title` attribute; a chip reading
		// "buy divine with chaos" would be the wrong vocabulary on screen.
		const label = legLabel(leg({ itemName: 'Mod Values', quoteName: 'Reroll Rare' }));
		expect(label).not.toContain('divine');
		expect(label).not.toContain('chaos');
	});

	it('prices the leg through formatLegPrice rather than printing the raw number', () => {
		expect(legLabel(leg({ price: 0.004975 }))).toBe(
			'buy Mod Values with Reroll Rare @ 0.004975'
		);
	});
});

describe('iconSrc', () => {
	// What `getApiBase()` hands the page: an origin plus the `/api` mount.
	const BASE = 'https://server.test/api';
	// What the server puts on a leg — `url.PathEscape`d id under the icon route.
	const PATH = '/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare';

	it('joins the API base onto the icon path the server sent', () => {
		// The `%2F`s are the server's escaping of the metadata id's slashes; the
		// join must carry them through byte for byte, because a decoded
		// "Metadata/Items/..." would address a different route entirely and a
		// re-encoded "%252F" would reach the handler as a literal percent. The
		// `/api` mount has to survive too: the icon route lives under it, so a
		// join that treated the path as origin-relative (`new URL(path, base)`)
		// would drop the mount and request an endpoint the server does not serve.
		expect(iconSrc(BASE, PATH)).toBe(
			'https://server.test/api/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare'
		);
	});

	it('renders no icon for a leg the server sent without artwork', () => {
		// `itemIcon: null` is the asset saying the item has no image at all, which
		// ItemIcon renders as nothing rather than as the "?" 404 fallback.
		expect(iconSrc(BASE, null)).toBeNull();
	});

	it('renders no icon for an empty path rather than requesting the API base itself', () => {
		// An empty path joined onto the base would fetch `/api`, which answers with
		// something that is not an image and would show the broken-image glyph.
		expect(iconSrc(BASE, '')).toBeNull();
	});

	it('trims a trailing slash off the base rather than emitting a double slash', () => {
		// Defensive: `iconSrc` takes the base as an argument, so its contract is
		// "join any base", not "join the one `getApiBase()` happens to build
		// today". `//currency-exchange/...` is normalised by some proxies and
		// 404d by others, so the trim is pinned for whatever base reaches it.
		expect(iconSrc('https://server.test/api/', PATH)).toBe(
			'https://server.test/api/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare'
		);
	});

	it('inserts the separator when the path arrives without a leading slash', () => {
		expect(iconSrc(BASE, 'currency-exchange/icon/Chaos')).toBe(
			'https://server.test/api/currency-exchange/icon/Chaos'
		);
	});
});

describe('refetchDelay', () => {
	it('waits the bare debounce when the roll comes up lowest', () => {
		expect(refetchDelay(() => 0)).toBe(REFETCH_DEBOUNCE_MS);
		expect(refetchDelay(() => 0)).toBe(2000);
	});

	it('waits debounce plus the whole jitter window when the roll comes up highest', () => {
		expect(refetchDelay(() => 1)).toBe(REFETCH_DEBOUNCE_MS + REFETCH_JITTER_MS);
		expect(refetchDelay(() => 1)).toBe(6000);
	});

	it('spreads the roll linearly across the window', () => {
		// A mid roll must land mid-window, not at an endpoint — the whole point of
		// the jitter is that clients receiving the same publish do not all refetch
		// in the same slot.
		expect(refetchDelay(() => 0.25)).toBe(3000);
	});

	it('re-rolls on every call instead of caching one offset per session', () => {
		// A fixed per-client offset would spread the herd once and then put the
		// same clients in the same slot on every subsequent publish.
		const rolls = [0, 0.5, 1];
		let i = 0;
		const random = () => rolls[i++];

		expect([refetchDelay(random), refetchDelay(random), refetchDelay(random)]).toEqual([
			2000, 4000, 6000
		]);
	});
});
