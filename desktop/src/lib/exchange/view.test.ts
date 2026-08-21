import { describe, it, expect } from 'vitest';
import {
	CHAOS_ID,
	HORIZON_OPTIONS,
	MODE_OPTIONS,
	REFETCH_DEBOUNCE_MS,
	REFETCH_JITTER_MS,
	SORT_OPTIONS,
	chaosIconPath,
	dataAgeParts,
	deriveState,
	formatChaos,
	formatGain,
	formatLegPrice,
	formatRoiPct,
	formatTime,
	formatTimeAgo,
	formatVolume,
	hoursProgress,
	iconSrc,
	legLabel,
	parseDensity,
	parseHorizon,
	parseMode,
	parseSort,
	parseUnit,
	refetchDelay,
	routeSlots,
	sortPlays,
	worthwhileScale
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
 * A clean, ranked play whose OPTIMISTIC `roi` (10c) and measured `expectedRoi`
 * (4c) deliberately differ, and differ from every value a case overrides.
 *
 * The two re-sorting branches read different fields — `'roi'` the optimistic
 * one, `'fastest'` the expectation through `worthwhileScale` — so a case about
 * one of them overrides that field only, and leaves the other at the factory
 * value across the whole fixture. A production swap between the two then reads
 * a column that says nothing about the row, and the answer stops being the one
 * the test asked for.
 *
 * The `'expected'` cases INVERT that idiom, and deliberately: that branch sorts
 * by nothing at all, so a fixture tied on the untested field would let a
 * mutation that lost the branch fall through to a comparator, tie, and hand
 * back the served order anyway. Those cases vary both fields instead.
 *
 * `expectedRoiPct` is NOT `expectedRoi / investment` (4/200 would be 0.02): the
 * wire's expectation pair carries no such identity — each is a mean over the
 * simulated entries, each with its own chased outlay — unlike
 * `roi === roiPct * investment`, which the optimistic pair does hold to
 * (POE-193).
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
	it('passes "expected" through', () => {
		expect(parseSort('expected')).toBe('expected');
	});

	it('passes "roi" through', () => {
		expect(parseSort('roi')).toBe('roi');
	});

	it('passes "fastest" through', () => {
		expect(parseSort('fastest')).toBe('fastest');
	});

	it('reads a stored "fill" as the fastest order rather than dropping to the default', () => {
		// The Fill order was renamed, not removed (POE-192): a reader who left the
		// picker on it asked for the shortest wait, and falling back to the served
		// order would silently re-rank their table on the next launch.
		expect(parseSort('fill')).toBe('fastest');
	});

	it('reads a stored "roiPct" as the served order rather than dropping to the default', () => {
		// The same forward mapping for the other renamed pick (POE-193): that name
		// meant "the list as the server ranked it", and the server now ranks on
		// `expectedRoi`. Falling back would land on the same order by accident
		// today and stop doing so the moment the default moves.
		expect(parseSort('roiPct')).toBe('expected');
	});

	it('falls back to "expected" for an unknown sort', () => {
		expect(parseSort('turnover')).toBe('expected');
	});

	it('falls back to "expected" for an unset preference', () => {
		// The server's own ranking is the default order, so an unset preference
		// leaves the list exactly as served — tie-breaks included.
		expect(parseSort('')).toBe('expected');
	});
});

describe('SORT_OPTIONS', () => {
	it('offers the three orders the table can be read in, labelled, in picker order', () => {
		expect(SORT_OPTIONS).toEqual([
			{ value: 'expected', label: 'Exp. ROI' },
			{ value: 'roi', label: 'ROI' },
			{ value: 'fastest', label: 'Fastest' }
		]);
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

	it('leaves the server ranking untouched under the Exp. ROI sort', () => {
		// The served order already carries the low-coverage band and expectedRoi
		// desc plus turnover, direct-first and key tie-breaks (POE-193);
		// re-sorting on expectedRoi alone would discard them.
		//
		// BOTH money fields vary, and out of order in the same direction, because
		// this case has two ways to go wrong and a tie would hide one of them:
		// re-sorting on expectedRoi desc answers b,c,a, and losing the branch
		// entirely — falling through to the roi comparator — answers b,c,a as
		// well. Either way the served order is gone.
		const served = [
			play({ key: 'a', roi: 5, expectedRoi: 2 }),
			play({ key: 'b', roi: 500, expectedRoi: 50 }),
			play({ key: 'c', roi: 50, expectedRoi: 20 })
		];

		expect(keys(sortPlays(served, 'expected'))).toEqual(['a', 'b', 'c']);
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

		expect(keys(sortPlays(served, 'roi'))).toEqual([
			'clean-big',
			'clean-small',
			'suspect-huge'
		]);
	});

	it('keeps the server order between two plays tied on ROI', () => {
		const served = [play({ key: 'first', roi: 40 }), play({ key: 'second', roi: 40 })];

		expect(keys(sortPlays(served, 'roi'))).toEqual(['first', 'second']);
	});

	it('sorts into a new array rather than reordering the fetched list', () => {
		// The page holds the fetched list in reactive state and re-derives the sort
		// from it; an in-place sort would make the Exp. ROI option unable to
		// restore the server ranking without a refetch.
		const served = [play({ key: 'a', roi: 5 }), play({ key: 'b', roi: 500 })];

		sortPlays(served, 'roi');

		expect(keys(served)).toEqual(['a', 'b']);
	});

	it("hands out a copy under the Exp. ROI sort too, never the caller's own array", () => {
		// The Exp. ROI branch keeps the served order but must not alias the
		// response's array: the page mutating the sorted list (or Svelte state
		// wrapping it) would otherwise write through to the fetched result.
		//
		// Both fields vary for the reason the order case above gives — the copy
		// has to be a copy of the SERVED list, and a tied fixture would let a
		// lost branch return a re-sorted array that happens to read the same.
		const served = [
			play({ key: 'a', roi: 5, expectedRoi: 2 }),
			play({ key: 'b', roi: 500, expectedRoi: 50 })
		];

		const sorted = sortPlays(served, 'expected');

		expect(sorted).not.toBe(served);
		expect(keys(sorted)).toEqual(['a', 'b']);
	});

	it('puts the play the market absorbs soonest first under the Fastest sort', () => {
		// Ascending, not descending: the question the Scale column answers is how
		// long the worthwhile size waits, and the shortest wait is the best row.
		// 100 flips at 10/h is 10 hours; 10 flips at 5/h is 2; 10 at 4000/h is 1.
		//
		// The scale is counted off the EXPECTATION (POE-193), so that is the field
		// each row varies; every one of them keeps the fixture's single `roi`, at
		// which the target divides into ten flips for all three alike. A scale
		// counted off that number would leave only `depth` to tell the rows apart
		// and answer slow, fast, middling — the thin book first, because ten flips
		// at 10/h and ten at 4000/h both round up to the same single hour.
		const served = [
			play({ key: 'slow', expectedRoi: 1, depth: 10 }),
			play({ key: 'fast', expectedRoi: 10, depth: 4000 }),
			play({ key: 'middling', expectedRoi: 10, depth: 5 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['fast', 'middling', 'slow']);
	});

	it('puts a play whose depth cannot be read last under the Fastest sort', () => {
		// An unreadable depth is an unknown wait, not a zero one — sorting it to
		// the front would put the least-known row at the top of the list.
		const served = [
			play({ key: 'unreadable', depth: 0 }),
			play({ key: 'slow', expectedRoi: 1, depth: 10 }),
			play({ key: 'fast', expectedRoi: 10, depth: 4000 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['fast', 'slow', 'unreadable']);
	});

	it('puts a play with no worthwhile scale last under the Fastest sort', () => {
		// A play the simulation expects nothing from never reaches the target, so
		// it has no wait to compare — the same "unknown, not instant" reading a
		// dead depth gets, and the branch a scale-less play would otherwise sort
		// by NaN. Since POE-193 this is a row the server really serves.
		const served = [
			play({ key: 'no-scale', expectedRoi: 0, depth: 4000 }),
			play({ key: 'slow', expectedRoi: 1, depth: 10 }),
			play({ key: 'fast', expectedRoi: 10, depth: 4000 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['fast', 'slow', 'no-scale']);
	});

	it('keeps the served order between two plays with no readable wait at all', () => {
		// Two unknowns compare as equal — a comparator that claims either one
		// precedes the other is non-reflexive, and the sort is free to act on
		// the lie in any order it likes.
		const served = [
			play({ key: 'first-unknown', depth: 0 }),
			play({ key: 'second-unknown', expectedRoi: 0, depth: 4000 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['first-unknown', 'second-unknown']);
	});

	it('keeps a clean play with an unreadable wait ahead of every suspect one', () => {
		// The suspect partition outranks the null-last rule: a clean unknown is
		// still a clean row, and the flag is the reason a suspect play sits last.
		const served = [
			play({ key: 'suspect-fast', expectedRoi: 100, depth: 100_000, suspect: true }),
			play({ key: 'clean-unreadable', depth: 0 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['clean-unreadable', 'suspect-fast']);
	});

	it('keeps a suspect play behind every clean one under the Fastest sort, however fast it absorbs', () => {
		const served = [
			play({ key: 'clean-slow', expectedRoi: 1, depth: 10 }),
			play({ key: 'suspect-instant', expectedRoi: 100, depth: 100_000, suspect: true }),
			play({ key: 'clean-quick', expectedRoi: 10, depth: 900 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual([
			'clean-quick',
			'clean-slow',
			'suspect-instant'
		]);
	});

	it('keeps the server order between two plays the market absorbs inside the same hour', () => {
		// The hours are whole ones, so everything under an hour ties at 1 — the
		// deeper book does NOT out-sort the thinner one, it keeps the server's
		// remaining tie-breaks.
		const served = [
			play({ key: 'first', expectedRoi: 10, depth: 900 }),
			play({ key: 'second', expectedRoi: 10, depth: 4000 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['first', 'second']);
	});

	it('sorts into a new array under the Fastest sort as well', () => {
		const served = [
			play({ key: 'slow', expectedRoi: 1, depth: 10 }),
			play({ key: 'fast', expectedRoi: 10, depth: 4000 })
		];

		sortPlays(served, 'fastest');

		expect(keys(served)).toEqual(['slow', 'fast']);
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

describe('formatChaos', () => {
	it('renders a whole-chaos amount as a bare count of orbs', () => {
		// Chaos AMOUNTS are integers — a count of items in a stash tab. The
		// decimals belong to the leg RATES, which go through formatLegPrice.
		expect(formatChaos(50)).toBe('50');
	});

	it('rounds a fractional amount to the nearest whole orb', () => {
		expect(formatChaos(49.6)).toBe('50');
	});

	it('separates the thousands of a four-figure payout', () => {
		expect(formatChaos(5050)).toBe('5,050');
	});

	it('separates every group of a seven-figure payout', () => {
		expect(formatChaos(1234567.891)).toBe('1,234,568');
	});

	it('rounds a sub-chaos amount away to zero', () => {
		// Accepted (POE-189): a play whose whole per-exchange figure is under one
		// chaos is not flippable once the exchange's gold fee is paid, so the four
		// significant digits this used to print dressed junk as precision.
		expect(formatChaos(0.0125)).toBe('0');
	});

	it('renders zero as a bare "0"', () => {
		expect(formatChaos(0)).toBe('0');
	});

	it('puts the minus sign outside the grouped digits', () => {
		expect(formatChaos(-1234)).toBe('-1,234');
	});

	it('rounds a half orb up on a gain', () => {
		expect(formatChaos(2.5)).toBe('3');
	});

	it('rounds a half orb up in magnitude on a loss, matching the gain', () => {
		// Rounding the signed value would break halves towards +Infinity and print
		// a loss one orb smaller than the identical gain.
		expect(formatChaos(-2.5)).toBe('-3');
	});

	it('renders a loss too small to print as an unsigned "0", never "-0"', () => {
		expect(formatChaos(-0.4)).toBe('0');
	});

	it('renders a non-finite amount as "0" rather than "NaN"', () => {
		expect(formatChaos(Number.NaN)).toBe('0');
	});
});

describe('formatGain', () => {
	it('signs a gain so the column does not read as a second cost', () => {
		expect(formatGain(700)).toBe('+700');
	});

	it('keeps the thousands grouping under the sign', () => {
		expect(formatGain(5050)).toBe('+5,050');
	});

	it('signs a gain that rounds up to a single orb', () => {
		expect(formatGain(0.6)).toBe('+1');
	});

	it('leaves a sub-chaos gain unsigned, because it rounds away to nothing', () => {
		// The rule formatRoiPct follows: the sign comes from the ROUNDED
		// magnitude, so a gain too small to print is not dressed as a gain.
		expect(formatGain(0.0125)).toBe('0');
	});

	it('signs a loss', () => {
		expect(formatGain(-5)).toBe('-5');
	});

	it('leaves a play that returns what it cost unsigned', () => {
		// A round trip at 0c is not a positive play, and "+0" would rank it as one
		// to a reader scanning the column for plus signs.
		expect(formatGain(0)).toBe('0');
	});

	it('renders negative zero unsigned rather than as "-0"', () => {
		expect(formatGain(-0)).toBe('0');
	});
});

describe('hoursProgress', () => {
	it('reports the fraction of the window a play held', () => {
		expect(hoursProgress(3, 6)).toBe(0.5);
	});

	it('reports a full window as 1', () => {
		expect(hoursProgress(6, 6)).toBe(1);
	});

	it('reports 0 for a window of no hours rather than dividing by zero', () => {
		// A body served before any hour closed: the bar is a CSS width, so an
		// Infinity here would be a broken track rather than an empty one.
		expect(hoursProgress(2, 0)).toBe(0);
	});

	it('clamps a count above the window to a full bar', () => {
		expect(hoursProgress(8, 6)).toBe(1);
	});

	it('reports 0 for a non-finite count', () => {
		expect(hoursProgress(Number.NaN, 6)).toBe(0);
	});
});

describe('worthwhileScale', () => {
	// Every case is stated against the 100c target the constant carries, so a
	// change to that constant fails these tests rather than passing silently.
	//
	// The step the target is divided by is the EXPECTATION (POE-193), never the
	// optimistic `roi` this used to read — so every case below, bar the last,
	// leaves `roi` at the fixture's 10c, which divides the target into 10 flips
	// and is therefore an answer none of the expectations here produce. The last
	// case is the one that needs a `roi` of its own, and says why.

	it('rounds the flip count up to the exchange that actually clears the target', () => {
		// 100 ÷ 3 is 33.3 exchanges, and 33 of them pay 99c — a chaos short.
		expect(worthwhileScale(play({ expectedRoi: 3 }))?.flips).toBe(34);
	});

	it('adds no extra flip to an expectation that divides the target exactly', () => {
		// Four exchanges expected to pay 25c clear exactly 100c, so the fifth is
		// not needed.
		expect(worthwhileScale(play({ expectedRoi: 25 }))?.flips).toBe(4);
	});

	it('reports a single flip for a play expected to clear the target on its own', () => {
		expect(worthwhileScale(play({ expectedRoi: 150 }))?.flips).toBe(1);
	});

	it('reports the chaos those flips are expected to pay, not the target', () => {
		// 34 exchanges at 3c overshoot to 102c; reporting the flat 100c would put a
		// number on screen the play does not pay.
		expect(worthwhileScale(play({ expectedRoi: 3 }))?.gain).toBe(102);
	});

	it('reports the chaos the flips tie up, not the cost of one exchange', () => {
		expect(worthwhileScale(play({ expectedRoi: 3, investment: 40 }))?.investment).toBe(1360);
	});

	it('rounds the hours the market needs to absorb the flips up to a whole hour', () => {
		// 200 flips against 30 units an hour is 6.7 hours of trading, which is a
		// seventh hour the reader spends, not a sixth.
		expect(worthwhileScale(play({ expectedRoi: 0.5, depth: 30 }))?.hours).toBe(7);
	});

	it('reports exactly one hour for a scale the hourly volume covers whole', () => {
		expect(worthwhileScale(play({ expectedRoi: 25, depth: 4 }))?.hours).toBe(1);
	});

	it('reports a second hour for a scale one flip past what the hour covers', () => {
		expect(worthwhileScale(play({ expectedRoi: 25, depth: 3 }))?.hours).toBe(2);
	});

	it('reports no hours for a play whose thinnest leg traded nothing', () => {
		// An hourly volume of 0 is an unreadable wait, not an instant one, and
		// dividing by it would answer Infinity — which the column would print.
		expect(worthwhileScale(play({ depth: 0 }))?.hours).toBeNull();
	});

	it('reports no hours for a negative depth', () => {
		expect(worthwhileScale(play({ depth: -5 }))?.hours).toBeNull();
	});

	it('reports no hours for a non-finite depth', () => {
		expect(worthwhileScale(play({ depth: Number.NaN }))?.hours).toBeNull();
	});

	it('still reports the scale for a play whose depth cannot be read', () => {
		// The flip count and what it ties up are known whatever the book did last
		// hour; only the wait is missing, so the row keeps its "×N → +Gc".
		expect(worthwhileScale(play({ expectedRoi: 25, depth: 0 }))?.flips).toBe(4);
	});

	it('reports no scale for a play the simulation expects to gain nothing', () => {
		// No repeat count reaches a positive target from a zero step, and dividing
		// would answer Infinity flips.
		expect(worthwhileScale(play({ expectedRoi: 0 }))).toBeNull();
	});

	it('reports no scale for a play the simulation expects to lose chaos', () => {
		expect(worthwhileScale(play({ expectedRoi: -5 }))).toBeNull();
	});

	it('reports no scale for a non-finite expectation', () => {
		expect(worthwhileScale(play({ expectedRoi: Number.NaN }))).toBeNull();
	});

	it('reports no scale for a measured loser however large its optimistic return', () => {
		// The row POE-193 put on the table and the old `roi` reading could not
		// express: the server's positivity floor (ADR-015) still keeps `roi` above
		// zero, and the simulation is free to measure a loss anyway — so a play can
		// carry the table's biggest ROI and no scale at all. Counting the
		// optimistic 500c would answer ×1 rather than the dash the column owes a
		// play with nothing to repeat toward.
		expect(worthwhileScale(play({ roi: 500, roiPct: 2.5, expectedRoi: -6 }))).toBeNull();
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

describe('chaosIconPath', () => {
	it('escapes the id’s slashes the way the server’s PathEscape does', () => {
		// Pinned, not rebuilt from CHAOS_ID: the whole point is that this string
		// matches the route `IconPath` registers. Verified 200 against a running
		// server — a differently escaped path would 404 and empty the tile again.
		expect(chaosIconPath()).toBe(
			'/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare'
		);
	});
});

describe('routeSlots', () => {
	const CHAOS_ICON = '/currency-exchange/icon/Chaos';
	const DIVINE_ID = 'Metadata/Items/Currency/CurrencyModValues';

	function leg(overrides: Partial<CurrencyExchangeLeg> = {}): CurrencyExchangeLeg {
		return {
			action: 'buy',
			item: 'card',
			quote: CHAOS_ID,
			price: 1,
			fair: 1.1,
			fairOk: true,
			tick: 0.01,
			volume: 2000,
			stock: 40,
			suspect: false,
			itemName: 'Imperial Legacy',
			itemIcon: '/currency-exchange/icon/Card',
			itemCategory: 'Divination Cards',
			quoteName: 'Chaos Orb',
			quoteIcon: CHAOS_ICON,
			quoteCategory: 'Currency',
			...overrides
		};
	}

	/** Buy the card at 1c, sell it at 15c — the two legs the wire sends. */
	function direct(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
		return play({
			legs: [leg(), leg({ action: 'sell', price: 15 })],
			investment: 50,
			roi: 700,
			...overrides
		});
	}

	/** Buy the astrolabe in chaos, sell it in divine, convert the divine back. */
	function oneHop(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
		return play({
			mode: '1-hop',
			legs: [
				leg({ item: 'astrolabe', itemName: 'Nameless Astrolabe' }),
				leg({
					action: 'sell',
					item: 'astrolabe',
					itemName: 'Nameless Astrolabe',
					quote: DIVINE_ID,
					quoteName: 'Divine Orb',
					quoteIcon: '/currency-exchange/icon/Divine',
					price: 0.5
				}),
				leg({
					action: 'sell',
					item: DIVINE_ID,
					itemName: 'Divine Orb',
					itemIcon: '/currency-exchange/icon/Divine',
					price: 204
				})
			],
			investment: 50,
			roi: 5050,
			...overrides
		});
	}

	it('spends what one exchange costs to enter', () => {
		expect(routeSlots(direct())?.spend.amount).toBe('50');
	});

	it('gets back the entry plus the round trip’s gain', () => {
		// Not `roi` alone (the gain is not the payout) and not a product of the
		// leg prices (those are raw, the ends are net of the ticks).
		expect(routeSlots(direct())?.get.amount).toBe('750');
	});

	it('names the item bought in step 1, at its buy price', () => {
		const buy = routeSlots(direct())?.buy;

		expect(buy?.name).toBe('Imperial Legacy');
		expect(buy?.rate).toBe('buy @ 1.00');
		expect(buy?.icon).toBe('/currency-exchange/icon/Card');
	});

	it('names what the sale pays out in for step 2, not the item sold', () => {
		const sell = routeSlots(direct())?.sell;

		expect(sell?.name).toBe('Chaos Orb');
		expect(sell?.rate).toBe('sell @ 15.00');
		expect(sell?.icon).toBe(CHAOS_ICON);
	});

	it('leaves a direct play without a convert step', () => {
		expect(routeSlots(direct())?.convert).toBeNull();
	});

	it('sells a 1-hop play into the intermediate currency at step 2', () => {
		expect(routeSlots(oneHop())?.sell.name).toBe('Divine Orb');
	});

	it('converts the intermediate currency back at step 3', () => {
		// The third leg is a `sell` on the wire; on screen it is the conversion
		// that returns the reader to the currency they started in.
		const convert = routeSlots(oneHop())?.convert;

		expect(convert?.name).toBe('Divine Orb');
		expect(convert?.rate).toBe('convert @ 204');
	});

	it('marks only the slot whose own leg is suspect', () => {
		const route = routeSlots(
			direct({ legs: [leg(), leg({ action: 'sell', price: 15, suspect: true })] })
		);

		expect(route?.buy.suspect).toBe(false);
		expect(route?.sell.suspect).toBe(true);
	});

	it('reports a gain when the round trip returns more chaos than it cost', () => {
		expect(routeSlots(direct({ roi: 700 }))?.positive).toBe(true);
	});

	it('reports no gain when the round trip only breaks even', () => {
		expect(routeSlots(direct({ roi: 0 }))?.positive).toBe(false);
	});

	it('gives both ends the built chaos artwork, not the path a leg happens to carry', () => {
		// The legs here ARE quoted in chaos and carry their own icon path; the ends
		// still take `chaosIconPath()`, so the two tiles cannot depend on which
		// artwork this particular play was served with.
		const route = routeSlots(direct());

		expect(route?.spend.icon).toBe(chaosIconPath());
		expect(route?.get.icon).toBe(chaosIconPath());
		expect(route?.spend.icon).not.toBe(CHAOS_ICON);
	});

	it('still gives both ends chaos artwork when no leg of the play is quoted in chaos', () => {
		// The black-square bug (POE-189): a round trip quoted end to end in divine
		// is still valued in chaos, so the ends must wear the chaos icon rather
		// than render as two empty tiles.
		const divineQuoted = direct({
			legs: [
				leg({ quote: DIVINE_ID, quoteName: 'Divine Orb', quoteIcon: '/icon/Divine' }),
				leg({ action: 'sell', quote: DIVINE_ID, quoteName: 'Divine Orb', quoteIcon: '/icon/Divine' })
			]
		});

		const route = routeSlots(divineQuoted);

		expect(route?.spend.icon).toBe(chaosIconPath());
		expect(route?.get.icon).toBe(chaosIconPath());
	});

	it('draws no route for a play that arrives with fewer than two legs', () => {
		expect(routeSlots(play({ legs: [leg()] }))).toBeNull();
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
