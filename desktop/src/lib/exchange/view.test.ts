import { describe, it, expect } from 'vitest';
import {
	CHAOS_ID,
	DIVINE_ID,
	HORIZON_OPTIONS,
	MODE_OPTIONS,
	REFETCH_DEBOUNCE_MS,
	REFETCH_JITTER_MS,
	SORT_OPTIONS,
	anyConvertStep,
	chaosIconPath,
	currencyIconPath,
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
	moneyColumns,
	parseDensity,
	parseHorizon,
	parseMode,
	parseSort,
	parseUnit,
	quoteUnit,
	refetchDelay,
	routeSlots,
	runLedger,
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

	it('orders by the run-priced ROI the column shows, not by the per-exchange one', () => {
		// The two orders genuinely disagree here, which is the whole point: `a`
		// gains 5c an exchange but its 1c expectation takes 100 flips to clear the
		// target, so its ROI column reads +500c; `b` gains ten times as much per
		// exchange and clears in 2, so its column reads +100c. A table ordered by
		// the wire's `roi` would put `b` on top of a number half `a`'s.
		const served = [
			play({ key: 'a', roi: 5, expectedRoi: 1 }),
			play({ key: 'b', roi: 50, expectedRoi: 50 })
		];

		expect(keys(sortPlays(served, 'roi'))).toEqual(['a', 'b']);
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

describe('moneyColumns', () => {
	// The invariant under test is ONE SCALE PER ROW: the three money columns are
	// the worthwhile RUN's figures whenever the play has a run, and one
	// exchange's when it does not — the same two branches `routeSlots` takes.
	//
	// Every scaled case runs on an `expectedRoi` of 3, which is 34 flips to clear
	// the 100c target. That count is deliberately not the one the fixture's other
	// fields produce: the optimistic `roi` of 10 would divide the target into 10,
	// so a production swap onto the wrong field answers a number no case here
	// expects. It is also not a divisor of the target, so the scaled gain
	// overshoots to 102c and cannot be confused with `SCALE_TARGET_CHAOS` itself.

	it('reports the chaos the whole run ties up, not the cost of one exchange', () => {
		// 34 flips at 40c each.
		expect(moneyColumns(play({ expectedRoi: 3, investment: 40 })).investment).toBe(1360);
	});

	it('reports the optimistic return across the whole run', () => {
		// The column stays the RAW best case (the NET/RAW convention the ROI%
		// column spells out) and is simply told at the run's size: 34 × 10c.
		expect(moneyColumns(play({ expectedRoi: 3, roi: 10 })).roi).toBe(340);
	});

	it('reports the chaos the run is expected to pay, not the per-exchange mean', () => {
		expect(moneyColumns(play({ expectedRoi: 3 })).expectedRoi).toBe(102);
	});

	it('falls back to the cost of one exchange for a play with no worthwhile run', () => {
		// A measured loser is served and flagged (ADR-016), and has no run to
		// repeat toward — so the fallback is a live branch, not a guard.
		expect(moneyColumns(play({ expectedRoi: -6, investment: 40 })).investment).toBe(40);
	});

	it('falls back to the optimistic return on one exchange for a play with no run', () => {
		expect(moneyColumns(play({ expectedRoi: -6, roi: 500 })).roi).toBe(500);
	});

	it('falls back to the per-exchange expectation for a play with no run', () => {
		// The one money figure on the row that can print a minus, and this is the
		// branch it prints in: scaling only ever happens above zero.
		expect(moneyColumns(play({ expectedRoi: -6 })).expectedRoi).toBe(-6);
	});

	it('falls back on an expectation of exactly zero', () => {
		// The boundary `worthwhileScale` refuses: no repeat count reaches a
		// positive target from a zero step.
		expect(moneyColumns(play({ expectedRoi: 0, investment: 40 })).investment).toBe(40);
	});
});

describe('runLedger', () => {
	// The rate every divine-entry fixture here is read back into chaos with.
	const DIVINE_RATE = 200;

	/**
	 * A wire leg. The pair and the price are ONE fact on the wire —
	 * `priceQuoteQty / priceItemQty === price` exactly — so every fixture below
	 * overrides all three together.
	 */
	function leg(overrides: Partial<CurrencyExchangeLeg> = {}): CurrencyExchangeLeg {
		return {
			action: 'buy',
			item: 'scarab',
			quote: CHAOS_ID,
			price: 1,
			priceItemQty: 1,
			priceQuoteQty: 1,
			fair: 1.1,
			fairOk: true,
			tick: 0.01,
			volume: 2000,
			stock: 40,
			suspect: false,
			itemName: 'Ambush Scarab',
			itemIcon: '/icon/Scarab',
			itemCategory: 'Scarabs',
			quoteName: 'Chaos Orb',
			quoteIcon: '/currency-exchange/icon/Chaos',
			quoteCategory: 'Currency',
			...overrides
		};
	}

	// These fixtures are WIRE-CONSISTENT, per §7 of
	// `docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md`: `investment` is `u0 · r`,
	// `roiPct` is the server's own formula over the legs' UNDERCUT prices
	// (`internal/exchange/plays.go:1083-1092`), and `roi` is `investment ·
	// roiPct`. A fixture that contradicted those identities could make a ledger
	// assertion pass or fail for a reason that has nothing to do with the code.
	//
	// The PAIR is pinned from the readable end: each fixture states `roi` as the
	// round decimal it wants the ledger to carry, and `roiPct` is `roi /
	// investment` — the back-solve, so that `investment · roiPct` returns exactly
	// that `roi` in float. Stating `roiPct` as the float `u1/u0 − 1` instead
	// would land an ulp away from the back-solved literal on two of the three
	// fixtures, and the product would then miss the round `roi` by an ulp too.
	// Each fixture's comment shows the formula the wire computes and the value it
	// lands on, to the digits where the two agree.

	/**
	 * Chaos-entry direct: a scarab bought at 19c and sold back at 21c on the one
	 * market.
	 *
	 * Worked by hand: `u0 = 19 × 1.01 = 19.19`, `u1 = 21 × 0.99 = 20.79`, and the
	 * entry is chaos so `r = 1` and `investment = u0 = 19.19`.
	 * `roiPct = u1/u0 − 1 = 0.0833767…`, and the gain the fixture pins is
	 * `roi = 1.6`, so the literal `roiPct` carries is `1.6 / 19.19` — the same
	 * value to the digits shown, an ulp off the float division.
	 * `expectedRoi` 0.5 needs `ceil(100 / 0.5)` = 200 exchanges to clear the 100c
	 * target, gaining `0.5 × 200` = 100c.
	 */
	function chaosScarab(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
		return play({
			key: 'direct:scarab:chaos',
			legs: [
				leg({ price: 19, priceItemQty: 1, priceQuoteQty: 19 }),
				leg({ action: 'sell', price: 21, priceItemQty: 1, priceQuoteQty: 21 })
			],
			roiPct: 0.08337675872850443,
			edge: 0.08337675872850443,
			roiPctRaw: 0.10526315789473684,
			investment: 19.19,
			roi: 1.6,
			expectedRoi: 0.5,
			...overrides
		});
	}

	/**
	 * Divine-entry direct: the same scarab, quoted in divine on both sides.
	 *
	 * `u0 = 0.0625 × 1.01 = 0.063125` div, `u1 = 0.1 × 0.99 = 0.099` div, and at
	 * `r = 200` chaos a divine `investment = u0 · r = 12.625`c.
	 * `roiPct = u1/u0 − 1 = 0.5683168…`, and the gain the fixture pins is
	 * `roi = 7.175`c — the same figure `r · (u1 − u0)` states — so the literal
	 * `roiPct` carries is `7.175 / 12.625`. Both float routes to the gain miss
	 * 7.175 by an ulp in opposite directions, which is why the round gain is the
	 * pinned end of the pair. `expectedRoi` 2 needs 50 exchanges, gaining 100c.
	 */
	function divineScarab(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
		const side = { quote: DIVINE_ID, quoteName: 'Divine Orb', quoteIcon: '/icon/Divine' };
		return play({
			key: 'direct:scarab:divine',
			legs: [
				leg({ ...side, price: 0.0625, priceItemQty: 16, priceQuoteQty: 1 }),
				leg({ ...side, action: 'sell', price: 0.1, priceItemQty: 10, priceQuoteQty: 1 })
			],
			roiPct: 0.5683168316831683,
			edge: 0.5683168316831683,
			roiPctRaw: 0.6,
			investment: 12.625,
			roi: 7.175,
			expectedRoi: 2,
			...overrides
		});
	}

	/**
	 * Chaos-entry 1-hop: buy the omen for chaos, sell it against divine, convert
	 * the divine back into chaos. The triangle, whose third leg is the only place
	 * `u2` is read.
	 *
	 * `u0 = 35 × 1.01 = 35.35`c, `u1 = 0.25 × 0.99 = 0.2475` div,
	 * `u2 = 209 × 0.99 = 206.91`c a divine — leg 3 is a `sell` on the wire, which
	 * is why its undercut takes the minus form. `r = 1`, so
	 * `investment = 35.35`, `roiPct = u1·u2/u0 − 1 = 0.4486626…` and
	 * `roi = investment · roiPct = 15.860225`. `roiPctRaw` is that same formula
	 * over the RAW prices, `0.25 × 209 / 35 − 1 = 0.4928571…`. `expectedRoi` 8.5
	 * needs `ceil(100 / 8.5)` = 12 exchanges, gaining `8.5 × 12` = 102c.
	 */
	function omen(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
		const item = { item: 'omen', itemName: 'Omen of Amelioration', itemIcon: '/icon/Omen' };
		return play({
			key: '1-hop:omen:divine',
			mode: '1-hop',
			legs: [
				leg({ ...item, price: 35, priceItemQty: 1, priceQuoteQty: 35 }),
				leg({
					...item,
					action: 'sell',
					quote: DIVINE_ID,
					quoteName: 'Divine Orb',
					quoteIcon: '/icon/Divine',
					price: 0.25,
					priceItemQty: 4,
					priceQuoteQty: 1
				}),
				leg({
					action: 'sell',
					item: DIVINE_ID,
					itemName: 'Divine Orb',
					itemIcon: '/icon/Divine',
					itemCategory: 'Currency',
					price: 209,
					priceItemQty: 1,
					priceQuoteQty: 209
				})
			],
			roiPct: 0.44866265912305514,
			edge: 0.44866265912305514,
			roiPctRaw: 0.4928571428571429,
			investment: 35.35,
			roi: 15.860225,
			expectedRoi: 8.5,
			...overrides
		});
	}

	/**
	 * The same triangle with a convert leg that came through with no usable
	 * price — the version skew the forward fallback exists for.
	 *
	 * The money fields stay the consistent ones: `roiPct`, `roi` and `investment`
	 * are what the server computed in the hour it DID price that leg, and the
	 * skew is on the leg alone. `chainEnd` is `I + R` and never touched `u2`, so
	 * it is unaffected; only the sale's own total loses its backward reading.
	 *
	 * The quantity pair goes with the price, per `unpriced` below.
	 */
	function skewedConvert(price: number): CurrencyExchangePlay {
		const legs = omen().legs;
		return omen({ legs: [legs[0]!, legs[1]!, unpriced(legs[2]!, price)] });
	}

	/**
	 * A leg whose price came through unusable, with the quantity pair dropped
	 * alongside it: the pair and the price are ONE fact (see `leg`), so a leg
	 * that lost its price lost the market ratio the pair states too. Leaving the
	 * priced fixture's 1-for-209 behind would put a readable ratio on the very
	 * leg the fixture is calling unpriced.
	 */
	function unpriced(base: CurrencyExchangeLeg, price: number): CurrencyExchangeLeg {
		return leg({ ...base, price, priceItemQty: 0, priceQuoteQty: 0 });
	}

	it('counts the exchanges of the worthwhile run', () => {
		// `ceil(100 / 0.5)`.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.flips).toBe(200);
	});

	it('says the row is scaled when a worthwhile run exists', () => {
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.scaled).toBe(true);
	});

	it('counts one exchange when the expectation is not worth repeating', () => {
		// A measured loser is served and ranked (ADR-016); it simply has no repeat
		// count that reaches a positive target, so the row counts a single trip.
		expect(runLedger(chaosScarab({ expectedRoi: -3 }), DIVINE_RATE)?.flips).toBe(1);
	});

	it('counts one exchange at an expectation of exactly zero', () => {
		// The boundary `worthwhileScale` refuses.
		expect(runLedger(chaosScarab({ expectedRoi: 0 }), DIVINE_RATE)?.flips).toBe(1);
	});

	it('says the row is unscaled when there is no worthwhile run', () => {
		expect(runLedger(chaosScarab({ expectedRoi: -3 }), DIVINE_RATE)?.scaled).toBe(false);
	});

	it('ties up the chaos the Investment column reports for the run', () => {
		// `19.19 × 200`, which in float is 3838.0000000000005 — NOT the 3838 that
		// `200 × 19 × 1.01` answers. The two differ in the last ulp precisely
		// because they associate the same product differently, so this literal is
		// what tells a ledger reading the Investment column apart from one
		// re-pricing the run off the buy leg.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.investmentChaos).toBe(3838.0000000000005);
	});

	it('carries the chaos the ROI column reports for the run', () => {
		// `1.6 × 200`, the best case at the run's size.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.roiChaos).toBe(320);
	});

	it('carries the chaos the Exp. ROI column reports for the run', () => {
		// `0.5 × 200` — the scale's own gain, and the only root that can go
		// negative.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.expectedRoiChaos).toBe(100);
	});

	it('roots an unscaled row in the wire’s per-exchange figures', () => {
		// One branch, so one wrong change loses all three at once.
		const ledger = runLedger(chaosScarab({ expectedRoi: -3 }), DIVINE_RATE);

		expect(ledger?.investmentChaos).toBe(19.19);
		expect(ledger?.roiChaos).toBe(1.6);
		expect(ledger?.expectedRoiChaos).toBe(-3);
	});

	it('ends the chain on the investment plus the best case', () => {
		// 3838.0000000000005 + 320, which lands back on a round 4158.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.chainEndChaos).toBe(4158);
	});

	it('ends the row on the investment plus the measured expectation', () => {
		// 3838.0000000000005 + 100. The Get end is the Spend end plus the Exp. ROI
		// column and nothing else — E5.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.getChaos).toBe(3938.0000000000005);
	});

	it('ends an unscaled row below its spend when the expectation is a loss', () => {
		// 19.19 − 3. The measurement, not broken arithmetic: the best case is still
		// on the row, as the chain end of 20.79.
		expect(runLedger(chaosScarab({ expectedRoi: -3 }), DIVINE_RATE)?.getChaos).toBe(16.19);
	});

	it('values a chaos entry at one chaos per unit', () => {
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.entryRate).toBe(1);
	});

	it('values a divine entry at the response’s divine rate', () => {
		expect(runLedger(divineScarab(), DIVINE_RATE)?.entryRate).toBe(200);
	});

	it('names a divine entry as the currency both ends are rendered in', () => {
		// Leg 1's quote, and not chaos by default: this is what the reader pays
		// with.
		expect(runLedger(divineScarab(), DIVINE_RATE)?.entryQuote).toBe(DIVINE_ID);
	});

	it('names the entry quote of a row whose sale is quoted elsewhere', () => {
		// The omen enters in chaos and sells against divine, so a ledger reading
		// the SELL leg's quote — the one currency on this row that is neither the
		// entry nor the end — answers divine here.
		expect(runLedger(omen(), DIVINE_RATE)?.entryQuote).toBe(CHAOS_ID);
	});

	it('spends the investment column itself on a chaos entry', () => {
		// Same ulp as `investmentChaos`, and the same discriminator: a Spend
		// recomputed as `flips × price × (1 + tick)` answers a flat 3838.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.spend).toBe(3838.0000000000005);
	});

	it('spends the chaos investment divided by the entry rate', () => {
		// 631.25c of investment at 200c a divine. A Spend left in chaos would read
		// 631.25 on a row whose unit word says divine.
		expect(runLedger(divineScarab(), DIVINE_RATE)?.spend).toBe(3.15625);
	});

	it('renders the chain end in the entry currency', () => {
		// (631.25 + 358.75) / 200.
		expect(runLedger(divineScarab(), DIVINE_RATE)?.chainEnd).toBe(4.95);
	});

	it('renders what the row gets in the entry currency', () => {
		// (631.25 + 100) / 200.
		expect(runLedger(divineScarab(), DIVINE_RATE)?.get).toBe(3.65625);
	});

	it('sells a direct play’s run for the chain end itself', () => {
		// A direct play's key is `direct:<marketID>` — one market, so both legs
		// carry the same quote and the sale is already in the entry currency.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.sellTotal).toBe(4158);
	});

	it('sells a divine-entry direct play’s run in the entry currency', () => {
		// 4.95 div — the chain end the row renders, NOT the 990c it was built
		// from. Only a divine entry tells those two apart: at `r = 1` the chaos
		// root and the entry-currency rendering are the same number, so the chaos
		// fixture above passes either way.
		expect(runLedger(divineScarab(), DIVINE_RATE)?.sellTotal).toBe(4.95);
	});

	it('derives no forward sale on a direct play', () => {
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.sellForward).toBe(false);
	});

	it('divides a 1-hop’s chain end by the undercut convert price', () => {
		// 614.5227 / 206.91 = 2.97 div exactly. The two wrong divisors are both
		// visible from here: the RAW 209 answers 2.9403, and the forward figure
		// `12 × 0.2475` answers 2.9699999999999998 — a different float from 2.97.
		expect(runLedger(omen(), DIVINE_RATE)?.sellTotal).toBe(2.97);
	});

	it('derives no forward sale while the convert leg carries a price', () => {
		expect(runLedger(omen(), DIVINE_RATE)?.sellForward).toBe(false);
	});

	it('falls forward to the sell leg’s own run total when the convert leg has no price', () => {
		// `12 × (0.25 × 0.99)` = 2.9699999999999998, which is NOT the 2.97 the
		// backward reading answers — so this pins the route, not just the value.
		expect(runLedger(skewedConvert(0), DIVINE_RATE)?.sellTotal).toBe(2.9699999999999998);
	});

	it('says the sale was derived forwards when the convert leg had no price', () => {
		// The flag is what tells the convert step to print the market's ratio: two
		// amounts derived by different routes would not be in this market's ratio.
		expect(runLedger(skewedConvert(0), DIVINE_RATE)?.sellForward).toBe(true);
	});

	it('falls forward on a convert price that is not a finite number', () => {
		// `Infinity > 0` is true, so only the finiteness half of the guard catches
		// this one — without it the sale would total a clean 0.
		expect(runLedger(skewedConvert(Number.POSITIVE_INFINITY), DIVINE_RATE)?.sellTotal).toBe(
			2.9699999999999998
		);
	});

	it('falls forward on a convert price below zero', () => {
		// −206.91 is a finite number, so only the sign half of the guard catches
		// it. A guard reading `!== 0` would divide the chain end by it and print a
		// negative 2.97 as the sale.
		expect(runLedger(skewedConvert(-209), DIVINE_RATE)?.sellTotal).toBe(2.9699999999999998);
	});

	it('reports no sale total when neither the convert nor the sell leg can price it', () => {
		const legs = omen().legs;
		const blind = omen({
			legs: [legs[0]!, unpriced(legs[1]!, 0), unpriced(legs[2]!, 0)]
		});

		expect(runLedger(blind, DIVINE_RATE)?.sellTotal).toBeNull();
	});

	it('refuses a forward sale on a sell price that is not a finite number', () => {
		// One guard, so one wrong change loses both facts: `Infinity > 0` is true,
		// so without the finiteness half the run would total Infinity AND claim
		// the total was derived forwards.
		const legs = omen().legs;
		const blind = omen({
			legs: [legs[0]!, unpriced(legs[1]!, Number.POSITIVE_INFINITY), unpriced(legs[2]!, 0)]
		});
		const ledger = runLedger(blind, DIVINE_RATE);

		expect(ledger?.sellTotal).toBeNull();
		expect(ledger?.sellForward).toBe(false);
	});

	it('reports no sale total on a sell price below zero', () => {
		// The sign half of the forward guard, the twin of the convert case above:
		// −0.2475 is finite, and a guard reading `!== 0` would total the run at a
		// negative 2.97 and print it as the sale.
		const legs = omen().legs;
		const blind = omen({
			legs: [legs[0]!, unpriced(legs[1]!, -0.25), unpriced(legs[2]!, 0)]
		});

		expect(runLedger(blind, DIVINE_RATE)?.sellTotal).toBeNull();
	});

	it('claims no forward derivation when there was no sale left to derive', () => {
		const legs = omen().legs;
		const blind = omen({
			legs: [legs[0]!, unpriced(legs[1]!, 0), unpriced(legs[2]!, 0)]
		});

		expect(runLedger(blind, DIVINE_RATE)?.sellForward).toBe(false);
	});

	it('draws no ledger for a body with fewer than two legs', () => {
		expect(runLedger(chaosScarab({ legs: [leg()] }), DIVINE_RATE)).toBeNull();
	});

	it('draws no ledger for a divine entry the hour cannot value in chaos', () => {
		// The one exempt branch of the invariant: no `r`, so no entry-currency
		// rendering exists to state the chain in. Unreachable on a served body and
		// guarded anyway, because the alternative is a division by zero printed as
		// an amount.
		expect(runLedger(divineScarab(), 0)).toBeNull();
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

describe('currencyIconPath', () => {
	it('escapes an id’s slashes the way the server’s path escaping does', () => {
		expect(currencyIconPath(DIVINE_ID)).toBe(
			'/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyModValues'
		);
	});
});

describe('quoteUnit', () => {
	it('abbreviates a chaos-quoted price to c', () => {
		expect(quoteUnit(CHAOS_ID)).toBe('c');
	});

	it('abbreviates a divine-quoted price to div', () => {
		expect(quoteUnit(DIVINE_ID)).toBe('div');
	});

	it('gives no unit to a currency it cannot name', () => {
		// A wrong unit beside a real price is the bug the units were added to
		// remove, so an id nobody checked prints nothing rather than "c".
		expect(quoteUnit('Metadata/Items/Currency/CurrencyUpgradeToRare')).toBe('');
	});
});

describe('routeSlots', () => {
	const CHAOS_ICON = '/currency-exchange/icon/Chaos';
	const DIVINE_ICON = '/currency-exchange/icon/Divine';
	/** The rate the divine-entry cases are read back into chaos with. */
	const DIVINE_RATE = 200;
	/** A scarab leg quoted in divine — the entry side of every mirror-route case. */
	const DIVINE_SIDE = {
		quote: DIVINE_ID,
		quoteName: 'Divine Orb',
		quoteIcon: DIVINE_ICON,
		item: 'scarab',
		itemName: 'Ambush Scarab',
		itemIcon: '/icon/Scarab'
	};

	function leg(overrides: Partial<CurrencyExchangeLeg> = {}): CurrencyExchangeLeg {
		return {
			action: 'buy',
			item: 'card',
			quote: CHAOS_ID,
			// The pair and the price are one fact on the wire — the server builds
			// them together and `priceQuoteQty / priceItemQty === price` exactly —
			// so every case that overrides one overrides the other, and the
			// factory's default 1-for-1 is 1c. A fixture whose pair contradicted
			// its price would let a rate read right for the wrong reason.
			price: 1,
			priceItemQty: 1,
			priceQuoteQty: 1,
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

	/**
	 * The chaos-entry screenshot case: a scarab bought at 19c and sold at 21c on
	 * the one market.
	 *
	 * The arithmetic every case below is checked against, worked independently of
	 * the production code: `expectedRoi` 0.5 needs `ceil(100 / 0.5)` = 200 flips
	 * to clear the 100c target, so the run is expected to gain `0.5 × 200` = 100c
	 * and costs `200 × 19 × 1.01` = 3,838c at the undercut entry — the +1% tick
	 * being the step an order that actually fills is posted above the extreme.
	 * It returns 3,838 + 100 = 3,938c.
	 */
	function chaosScarab(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
		return play({
			legs: [
				leg({
					item: 'scarab',
					itemName: 'Ambush Scarab',
					itemIcon: '/icon/Scarab',
					price: 19,
					priceItemQty: 1,
					priceQuoteQty: 19
				}),
				leg({
					action: 'sell',
					item: 'scarab',
					itemName: 'Ambush Scarab',
					itemIcon: '/icon/Scarab',
					price: 21,
					priceItemQty: 1,
					priceQuoteQty: 21
				})
			],
			investment: 19.19,
			roi: 1.5,
			expectedRoi: 0.5,
			...overrides
		});
	}

	/**
	 * The divine-entry screenshot case: the same scarab traded against divine on
	 * both sides — the mirror route that used to read chaos → chaos → chaos with
	 * the divine appearing nowhere on the row.
	 *
	 * Worked independently: `expectedRoi` 2c needs `ceil(100 / 2)` = 50 flips,
	 * gaining 100c. The run costs `50 × 0.0625 × 1.01` = 3.15625 divine, which at
	 * 200c a divine is 631.25c — and `investment` is that chaos figure per
	 * exchange, 12.625c. The profit is `100 / 200` = 0.5 divine, so the run
	 * returns 3.65625 divine.
	 */
	function divineScarab(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
		return play({
			legs: [
				leg({ ...DIVINE_SIDE, price: 0.0625, priceItemQty: 16, priceQuoteQty: 1 }),
				leg({ ...DIVINE_SIDE, action: 'sell', price: 0.1, priceItemQty: 10, priceQuoteQty: 1 })
			],
			investment: 12.625,
			roi: 0.4,
			expectedRoi: 2,
			...overrides
		});
	}

	/**
	 * The divine-entry 1-hop: buy the scarab against DIVINE, sell it against
	 * chaos, then convert the chaos back into divine. The triangle whose every
	 * step is quoted in a different currency from the step before it, which is
	 * what makes it the one shape that can catch a convert slot reading its leg's
	 * quote instead of its item.
	 *
	 * Worked independently of the production code: `expectedRoi` 2c needs
	 * `ceil(100 / 2)` = 50 flips and gains 100c, so the run costs
	 * `50 × 0.0625 × 1.01` = 3.15625 divine — 631.25c at 200c a divine, which is
	 * the per-exchange `investment` of 12.625c across those 50 flips. The profit
	 * is `100 / 200` = 0.5 divine, so the run returns 3.65625 divine. The convert
	 * leg prices chaos in divine at `1 / 196` = 0.005102 divine an orb.
	 */
	function divineOneHop(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
		const scarab = { item: 'scarab', itemName: 'Ambush Scarab', itemIcon: '/icon/Scarab' };
		return play({
			mode: '1-hop',
			legs: [
				leg({
					...scarab,
					quote: DIVINE_ID,
					quoteName: 'Divine Orb',
					quoteIcon: DIVINE_ICON,
					price: 0.0625,
					priceItemQty: 16,
					priceQuoteQty: 1
				}),
				leg({ ...scarab, action: 'sell', price: 14, priceItemQty: 1, priceQuoteQty: 14 }),
				leg({
					action: 'sell',
					item: CHAOS_ID,
					itemName: 'Chaos Orb',
					itemIcon: CHAOS_ICON,
					quote: DIVINE_ID,
					quoteName: 'Divine Orb',
					quoteIcon: DIVINE_ICON,
					price: 1 / 196,
					priceItemQty: 196,
					priceQuoteQty: 1
				})
			],
			investment: 12.625,
			roi: 0.4,
			expectedRoi: 2,
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
					quoteIcon: DIVINE_ICON,
					price: 0.5,
					priceItemQty: 2,
					priceQuoteQty: 1
				}),
				leg({
					action: 'sell',
					item: DIVINE_ID,
					itemName: 'Divine Orb',
					itemIcon: DIVINE_ICON,
					price: 204,
					priceItemQty: 1,
					priceQuoteQty: 204
				})
			],
			investment: 50,
			roi: 5050,
			...overrides
		});
	}

	/** The omen the screenshot case trades, on all three of its markets. */
	const OMEN = { item: 'omen', itemName: 'Omen of Amelioration', itemIcon: '/icon/Omen' };

	/** Bought with chaos on a market that posts one omen at a time for 35c. */
	function omenBuy(overrides: Partial<CurrencyExchangeLeg> = {}): CurrencyExchangeLeg {
		return leg({ ...OMEN, price: 35, priceItemQty: 1, priceQuoteQty: 35, ...overrides });
	}

	/** Sold against divine on a market that posts four omens for one divine. */
	function omenSell(overrides: Partial<CurrencyExchangeLeg> = {}): CurrencyExchangeLeg {
		return leg({
			...OMEN,
			action: 'sell',
			quote: DIVINE_ID,
			quoteName: 'Divine Orb',
			quoteIcon: DIVINE_ICON,
			price: 0.25,
			priceItemQty: 4,
			priceQuoteQty: 1,
			...overrides
		});
	}

	/** The divine proceeds turned back into chaos: one divine for 209c. */
	function omenConvert(overrides: Partial<CurrencyExchangeLeg> = {}): CurrencyExchangeLeg {
		return leg({
			action: 'sell',
			item: DIVINE_ID,
			itemName: 'Divine Orb',
			itemIcon: DIVINE_ICON,
			price: 209,
			priceItemQty: 1,
			priceQuoteQty: 209,
			...overrides
		});
	}

	/**
	 * The omen screenshot case — the row the postable-order rendering was written
	 * for, and the one shape whose three steps carry three different pairs.
	 *
	 * Worked independently of the production code: `expectedRoi` 8.5c needs
	 * `ceil(100 / 8.5)` = `ceil(11.764…)` = 12 flips. The buy market posts one
	 * omen for 35c, so twelve flips is twelve of that order — `12 × 35` = 420c.
	 * The sell market posts four omens for one divine and 12 is exactly three of
	 * those lots, so it prints `12 for 3 div` with nothing to snap. The convert
	 * market posts one divine for 209c and carries no run at all: what step 3
	 * moves is the sale's proceeds, which is 3 divine here and a fraction on any
	 * run the sell lot does not divide.
	 */
	function omen(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
		return play({
			mode: '1-hop',
			legs: [omenBuy(), omenSell(), omenConvert()],
			investment: 35.35,
			roi: 12,
			expectedRoi: 8.5,
			...overrides
		});
	}

	it('spends the whole worthwhile run at the undercut buy price', () => {
		// 200 flips × 19c × 1.01, not 200 × 19 and not the per-exchange 19.19.
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.spend.amount).toBe('3,838');
	});

	it('counts the run size into the buy step’s order', () => {
		// 200 flips of a market that posts one at a time for 19c: 200 × 19 = 3,800c.
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.buy.rate).toBe('buy 200 for 3,800c');
	});

	it('gets the run’s spend back plus the profit it is expected to make', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.get.amount).toBe('3,938');
	});

	it('keeps the run’s expected profit in chaos alone when the entry is chaos', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.get.sub).toBe('keep ≈ 100c');
	});

	it('keeps exactly the chaos the Exp. ROI column reports for the same play', () => {
		// The two are one number by construction, not two agreeing calculations,
		// and this is the case that can tell the difference: a 3c expectation
		// takes 34 flips and overshoots the target to 102c, so neither the wire
		// field (3) nor the target constant (100) can produce the answer.
		const scarab = chaosScarab({ expectedRoi: 3 });
		const column = moneyColumns(scarab).expectedRoi;

		expect(column).toBe(102);
		expect(routeSlots(scarab, DIVINE_RATE)?.get.sub).toBe(`keep ≈ ${formatChaos(column)}c`);
	});

	it('adds no chaos reading under a spend that is already chaos', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.spend.sub).toBeNull();
	});

	it('names the item bought at step 1, at its posted buy price', () => {
		const buy = routeSlots(chaosScarab(), DIVINE_RATE)?.buy;

		expect(buy?.name).toBe('Ambush Scarab');
		expect(buy?.icon).toBe('/icon/Scarab');
	});

	it('names the item sold at step 2 rather than the currency it is sold for', () => {
		// The headline bug: the sell leg's QUOTE — "Divine Orb", with the divine
		// artwork — sat beside the SCARAB's 0.10 price.
		const sell = routeSlots(divineScarab(), DIVINE_RATE)?.sell;

		expect(sell?.name).toBe('Ambush Scarab');
		expect(sell?.icon).toBe('/icon/Scarab');
	});

	it('quotes the sell step in the currency its own leg is priced in', () => {
		// The 0.1 divine this used to print is not a price the exchange can hold:
		// the market posts 10 scarabs for 1 divine. It sells FOUR of those lots,
		// not five — the buy step could only fill 48 of the 50-flip run, and
		// `floor(48 / 10)` = 4 is the largest sell order 48 scarabs covers. The
		// leftover 8 stay in the stash; the ends still price all 50 flips.
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.sell.rate).toBe('sell 40 for 4 div');
	});

	it('names the stock a sell lot leaves behind', () => {
		// 48 bought, 40 sold. The ends price all 50 flips and the rate prints 40,
		// so the 8 in between appear nowhere else on the row — without this clause
		// the reader has to derive the shortfall from two numbers in different
		// slots.
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.sell.rateTitle).toContain(
			'8 of the 48 bought stay unsold'
		);
	});

	it('leaves the unsold clause off a buy step, which bought no stock to leave', () => {
		// The buy step snaps 50 down to 48 for the same reason, but the 2 it drops
		// were never bought — the run the ends price already accounts for them, and
		// "2 of the 50 bought stay unsold" would invent a holding.
		const title = routeSlots(divineScarab(), DIVINE_RATE)?.buy.rateTitle;

		expect(title).toContain('multiples of 16');
		expect(title).not.toContain('unsold');
	});

	it('quotes the buy step of a divine-entry run in divine', () => {
		// The buy market posts 16 for 1 div, and 50 is not a multiple of 16, so the
		// order snaps down to the three whole lots that fit: 48 for 3 div.
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.buy.rate).toBe('buy 48 for 3 div');
	});

	it('spends a divine-entry run in divine rather than in chaos', () => {
		const spend = routeSlots(divineScarab(), DIVINE_RATE)?.spend;

		expect(spend?.amount).toBe('3.16');
		expect(spend?.unit).toBe('divine');
	});

	it('reads a divine spend back into chaos on its sub-line', () => {
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.spend.sub).toBe('≈ 631c');
	});

	it('returns a divine-entry run in divine', () => {
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.get.amount).toBe('3.66');
	});

	it('gives a divine-entry profit line the divine equivalent of its chaos', () => {
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.get.sub).toBe('keep ≈ 100c (≈ 0.50 div)');
	});

	it('hands the unit word and the profit line back on one hover', () => {
		// Dense hides both the unit beside the number and the sub-line under it, so
		// this title is the only route back to either — a bare 3.66 with no way to
		// learn it counts divine is not a row anyone can act on.
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.get.title).toBe(
			'divine — keep ≈ 100c (≈ 0.50 div)'
		);
	});

	it('hovers an end with nothing under it with its unit word alone', () => {
		// A chaos spend carries no sub-line, and a title ending in a dash with
		// nothing after it reads as a string that lost its tail.
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.spend.title).toBe('chaos');
	});

	it('keeps a divine end precise to the hundredth above a hundred orbs', () => {
		// A leg-price formatter drops to whole numbers above 100, which here would
		// print 101 spent and 102 returned on a run that keeps half a divine — the
		// total moving by four times the profit that produced it.
		// 50 flips × 2.005 × 1.01 = 101.2525 divine, plus 100c / 200 = 0.5.
		const big = divineScarab({
			legs: [
				leg({ ...DIVINE_SIDE, price: 2.005, priceItemQty: 200, priceQuoteQty: 401 }),
				leg({ ...DIVINE_SIDE, action: 'sell', price: 3, priceItemQty: 1, priceQuoteQty: 3 })
			],
			investment: 405
		});

		const route = routeSlots(big, DIVINE_RATE);

		expect(route?.spend.amount).toBe('101.25');
		expect(route?.get.amount).toBe('101.75');
		expect(route?.get.sub).toBe('keep ≈ 100c (≈ 0.50 div)');
	});

	it('groups a divine end into thousands', () => {
		// 50 flips × 20 × 1.01 = 1,010 divine. Grouped by hand, for the reason
		// `formatChaos` groups by hand: a locale that separates with "." would
		// print 1.010 beside an English sentence.
		const huge = divineScarab({
			legs: [
				leg({ ...DIVINE_SIDE, price: 20, priceItemQty: 1, priceQuoteQty: 20 }),
				leg({ ...DIVINE_SIDE, action: 'sell', price: 25, priceItemQty: 1, priceQuoteQty: 25 })
			],
			investment: 4040
		});

		expect(routeSlots(huge, DIVINE_RATE)?.spend.amount).toBe('1,010.00');
	});

	it('gives the end tiles the built artwork of the currency the run is entered with', () => {
		// Built from the id, never scavenged off `quoteIcon` — a leg served with
		// no artwork left both ends as empty tiles (POE-189).
		const route = routeSlots(divineScarab(), DIVINE_RATE);

		expect(route?.spend.icon).toBe(currencyIconPath(DIVINE_ID));
		expect(route?.get.icon).toBe(currencyIconPath(DIVINE_ID));
		expect(route?.spend.icon).not.toBe(DIVINE_ICON);
	});

	it('gives a chaos-entry run the built chaos artwork', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.spend.icon).toBe(chaosIconPath());
	});

	it('falls back to chaos ends when the hour published no divine rate', () => {
		// Not a served shape — a hour with no divine trade serves no divine-quoted
		// play — but the alternative to the guard is a division by zero printed as
		// an amount. 50 flips × 12.625c invested, plus the same 100c gain.
		const route = routeSlots(divineScarab(), 0);

		expect(route?.spend.unit).toBe('chaos');
		expect(route?.spend.amount).toBe('631');
		expect(route?.get.amount).toBe('731');
	});

	it('leaves a direct play without a convert step', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.convert).toBeNull();
	});

	it('converts the intermediate currency back at step 3', () => {
		// The third leg is a `sell` on the wire; on screen it is the conversion
		// that returns the reader to the currency they started in. 25 flips gaining
		// 100c cost 25.25c and come back as 125.25c, and this market prices a divine
		// at 204c — so what the run converts is 125.25 / 204 = 0.61 divine.
		const convert = routeSlots(oneHop(), DIVINE_RATE)?.convert;

		expect(convert?.rate).toBe('≈ 0.61 div → 125c');
		expect(convert?.icon).toBe(DIVINE_ICON);
	});

	it('sells the bought item at step 2 of a 1-hop play, not the currency it lands in', () => {
		expect(routeSlots(oneHop(), DIVINE_RATE)?.sell.name).toBe('Nameless Astrolabe');
	});

	it('marks step 3 with the currency being converted, not the one it is converted into', () => {
		// The convert leg's ITEM is the intermediate chaos being spent; its QUOTE
		// is the divine coming back. The tile is what names the currency now that
		// the run total has taken the name line's place, so reading the quote here
		// would put the divine artwork over a quantity of chaos orbs.
		expect(routeSlots(divineOneHop(), DIVINE_RATE)?.convert?.icon).toBe(CHAOS_ICON);
	});

	it('ends a divine-entry triangle’s convert line on the amount its Get slot shows', () => {
		// The tail is the Get amount itself, at the precision the ENDS use — the
		// hundredth of a divine, not the whole orbs a postable order counts in. The
		// chaos being converted is that same total read back through this market's
		// own price: 3.65625 × 196 = 716.6, which rounds to 717 whole orbs.
		const route = routeSlots(divineOneHop(), DIVINE_RATE);

		expect(route?.convert?.rate).toBe('≈ 717c → 3.66 div');
		expect(route?.get.amount).toBe('3.66');
	});

	it('quotes each step of a divine-entry triangle in that step’s own currency', () => {
		// Three steps, two currencies, and the row used to show neither: buying in
		// divine, selling in chaos and converting back read as one undifferentiated
		// sequence of bare numbers.
		const route = routeSlots(divineOneHop(), DIVINE_RATE);

		// 48 bought, so 48 sold — `floor(48 / 1)` on a one-at-a-time chaos market
		// is 48, at 14c each: 672c.
		expect(route?.buy.rate).toBe('buy 48 for 3 div');
		expect(route?.sell.rate).toBe('sell 48 for 672c');
	});

	it('enters a divine-entry triangle in divine', () => {
		const route = routeSlots(divineOneHop(), DIVINE_RATE);

		expect(route?.spend.unit).toBe('divine');
		expect(route?.spend.amount).toBe('3.16');
		expect(route?.spend.sub).toBe('≈ 631c');
	});

	it('returns a divine-entry triangle in divine', () => {
		const route = routeSlots(divineOneHop(), DIVINE_RATE);

		expect(route?.get.amount).toBe('3.66');
		expect(route?.get.sub).toBe('keep ≈ 100c (≈ 0.50 div)');
	});

	it('marks only the slot whose own leg is suspect', () => {
		const route = routeSlots(
			chaosScarab({
				legs: [
					leg({ price: 19, priceItemQty: 1, priceQuoteQty: 19 }),
					leg({ action: 'sell', price: 21, priceItemQty: 1, priceQuoteQty: 21, suspect: true })
				]
			}),
			DIVINE_RATE
		);

		expect(route?.buy.suspect).toBe(false);
		expect(route?.sell.suspect).toBe(true);
	});

	it('reports a gain when the run is expected to profit', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.positive).toBe(true);
	});

	it('still reports a gain on a measured loser, whose ends are its optimistic pair', () => {
		// `expectedRoi` negative means no run size, so the ends fall back to
		// `investment` and `investment + roi` — and that Get is visibly the larger
		// of the two. The flag styles what is on screen; the measured verdict is
		// the Exp. ROI cell's and the Scale column's to carry.
		const route = routeSlots(chaosScarab({ expectedRoi: -3, investment: 50, roi: 700 }), 0);

		expect(route?.get.amount).toBe('750');
		expect(route?.positive).toBe(true);
	});

	it('reports no gain when a play with no run size returns exactly what it cost', () => {
		expect(routeSlots(chaosScarab({ expectedRoi: -3, roi: 0 }), DIVINE_RATE)?.positive).toBe(
			false
		);
	});

	it('falls back to what one exchange costs when there is no run size to draw', () => {
		// `expectedRoi <= 0` has no repeat count that reaches the target, and
		// ADR-016 serves such a play anyway — the ends drop to the two chaos
		// figures the wire guarantees.
		const route = routeSlots(chaosScarab({ expectedRoi: 0, investment: 50, roi: 700 }), DIVINE_RATE);

		expect(route?.spend.amount).toBe('50');
		expect(route?.get.amount).toBe('750');
		expect(route?.spend.unit).toBe('chaos');
	});

	it('shows one bare lot on the buy step when there is no run size', () => {
		// No run to scale to, so the step is the market's own pair: one for 19c.
		expect(routeSlots(chaosScarab({ expectedRoi: 0 }), DIVINE_RATE)?.buy.rate).toBe(
			'buy 1 for 19c'
		);
	});

	it('says on every step of an unscaled play why it shows one lot', () => {
		// The two markets post in different lots, so this branch renders "buy 1 for
		// 35c → sell 4 for 1 div" — a 4:1 divergence that is no snap and that
		// nothing else on the row accounts for, the ends having dropped to what ONE
		// exchange costs. Both steps carry the note; only step 3 is silent, its bare
		// pair being what a convert always shows.
		const unscaled = routeSlots(omen({ expectedRoi: 0 }), DIVINE_RATE);

		expect(unscaled?.buy.rate).toBe('buy 1 for 35c');
		expect(unscaled?.sell.rate).toBe('sell 4 for 1 div');
		expect(unscaled?.buy.rateTitle).toContain('no worthwhile size');
		expect(unscaled?.sell.rateTitle).toContain('one lot of its own market');
		expect(unscaled?.convert?.rateTitle).toBeNull();
	});

	it('claims no lot mismatch on an unscaled row whose two markets post alike', () => {
		// The note goes on every unscaled step, including a row where the counts do
		// line up (1 for 19c → 1 for 21c) — so it may not assert that the two lots
		// differ, only that agreeing is a coincidence of the two markets.
		const alike = routeSlots(chaosScarab({ expectedRoi: 0 }), DIVINE_RATE);

		expect(alike?.buy.rate).toBe('buy 1 for 19c');
		expect(alike?.sell.rate).toBe('sell 1 for 21c');
		expect(alike?.buy.rateTitle).toContain('agree only where the two markets happen to post the same lot');
	});

	it('leaves the unscaled note off a step that printed no lot to explain', () => {
		// Version skew on top of a losing expectation: the buy step shows a decimal,
		// so "this step shows one lot of its own market" would describe a line that
		// is not on screen.
		const skewed = omen({
			expectedRoi: 0,
			legs: [omenBuy({ priceItemQty: 0, priceQuoteQty: 0 }), omenSell(), omenConvert()]
		});

		expect(routeSlots(skewed, DIVINE_RATE)?.buy.rate).toBe('buy @ 35.00 c');
		expect(routeSlots(skewed, DIVINE_RATE)?.buy.rateTitle).toBeNull();
	});

	it('keeps the unit on a rate of a play with no run size', () => {
		// The mirror-route fix does not depend on the scale: a divine price stays
		// labelled divine on a play the simulation could not measure.
		expect(routeSlots(divineScarab({ expectedRoi: 0 }), DIVINE_RATE)?.sell.rate).toBe(
			'sell 10 for 1 div'
		);
	});

	it('promises no profit under a play with no run size', () => {
		expect(routeSlots(chaosScarab({ expectedRoi: 0 }), DIVINE_RATE)?.get.sub).toBeNull();
	});

	it('leaves a rate quoted in neither chaos nor divine without a unit', () => {
		const exotic = chaosScarab({
			legs: [
				leg({ price: 19, priceItemQty: 1, priceQuoteQty: 19 }),
				leg({
					action: 'sell',
					price: 21,
					priceItemQty: 1,
					priceQuoteQty: 21,
					quote: 'Metadata/Items/Currency/CurrencyUpgradeToRare'
				})
			]
		});

		// 200 flips of a one-for-21 market — the quantities still scale, and only
		// the unit word is withheld, because a wrong currency beside a real number
		// is worse than none.
		expect(routeSlots(exotic, DIVINE_RATE)?.sell.rate).toBe('sell 200 for 4,200');
	});

	it('posts the buy step as a whole order rather than a per-unit price', () => {
		// The rendering this replaced said "buy 12 @ 35.00 c", which is a number
		// the in-game exchange has no field for. Twelve omens for 420 chaos is an
		// order: 12 × 35c.
		expect(routeSlots(omen(), DIVINE_RATE)?.buy.rate).toBe('buy 12 for 420c');
	});

	it('posts the sell step at the run quantity when the market’s lot divides it', () => {
		// 12 omens is three of the market's four-for-one-divine lots, so the run
		// and the postable order are the same quantity — and the divine side is a
		// whole 3, never the 0.25 each the decimal reading implied.
		expect(routeSlots(omen(), DIVINE_RATE)?.sell.rate).toBe('sell 12 for 3 div');
	});

	it('adds no hover to a step whose posted order is the whole run', () => {
		// The hover is the snap's explanation; on an exact multiple there is
		// nothing to explain, and a title on every rate would train the reader to
		// ignore the one that matters.
		expect(routeSlots(omen(), DIVINE_RATE)?.sell.rateTitle).toBeNull();
	});

	it('totals the whole run at step 3 rather than one lot of its market', () => {
		// "convert 1 div for 209c" read as an order to convert ONE divine on a row
		// whose other four slots all count the 12-flip run. What step 3 actually
		// moves is that run's proceeds: the 526c Get read back through this
		// market's own price, 526.2 / 209 = 2.52 div.
		expect(routeSlots(omen(), DIVINE_RATE)?.convert?.rate).toBe('≈ 2.52 div → 526c');
	});

	it('ends the convert line on exactly the Get amount the row finishes with', () => {
		// The two are one figure by construction, not two agreeing calculations:
		// re-deriving the total from the quantities the buy and sell steps print
		// would use numbers deliberately snapped to their markets' lots (3 div ×
		// 209c = 627c here) and put a third, disagreeing total on the row.
		const route = routeSlots(omen(), DIVINE_RATE);

		expect(route?.get.amount).toBe('526');
		expect(route?.convert?.rate.endsWith(`→ ${route?.get.amount}c`)).toBe(true);
	});

	it('moves step 3’s item name off the line and onto its hover', () => {
		// The total names both currencies and the tile carries the artwork, so a
		// "Divine Orb" heading over it repeats the tile and costs a line the row
		// has no height for. It is not dropped — it leads the hover.
		const convert = routeSlots(omen(), DIVINE_RATE)?.convert;

		expect(convert?.name).toBeNull();
		expect(convert?.rateTitle).toContain('Divine Orb');
	});

	it('keeps the market’s own ratio on the convert step’s hover', () => {
		// The line is a total now, so the order this market actually posts appears
		// nowhere else on the row: a reader who wants to know what one divine
		// fetches has only the hover to ask.
		expect(routeSlots(omen(), DIVINE_RATE)?.convert?.rateTitle).toContain(
			'convert 1 div for 209c'
		);
	});

	it('shows the market’s ratio at step 3 of a play with no run to total', () => {
		// `worthwhileScale` answered null, so the ends are what ONE exchange costs
		// and there are no run proceeds for step 3 to total. The market's own order
		// is what is left that stays true, and it keeps its name line with it.
		const unscaled = routeSlots(omen({ expectedRoi: 0 }), DIVINE_RATE);

		expect(unscaled?.convert?.rate).toBe('convert 1 div for 209c');
		expect(unscaled?.convert?.name).toBe('Divine Orb');
	});

	it('shows no run total at step 3 when the convert leg came through without a price', () => {
		// Version skew, the shape the buy and sell steps already guard against. The
		// run's Get divided by a price of 0 is not a quantity, and printing it as
		// "≈ 0.00 div → 526c" would put a total on the row that no market backs.
		const priceless = omen({
			legs: [omenBuy(), omenSell(), omenConvert({ price: 0, priceItemQty: 0, priceQuoteQty: 0 })]
		});

		expect(routeSlots(priceless, DIVINE_RATE)?.convert?.rate).toBe('convert @ 0.00 c');
	});

	it('snaps a displayed order down to the largest lot the market can post', () => {
		// A market that posts five omens for two divine cannot be told 12:
		// `floor(12 / 5)` = 2 lots, so the order shown is 10 for 4 div — short of
		// the run, and exactly postable, which is the trade this rendering makes.
		const chunky = omen({
			legs: [omenBuy(), omenSell({ price: 0.4, priceItemQty: 5, priceQuoteQty: 2 }), omenConvert()]
		});

		expect(routeSlots(chunky, DIVINE_RATE)?.sell.rate).toBe('sell 10 for 4 div');
	});

	it('says on a snapped step why its quantity is not the run’s', () => {
		// The route's ends still count all 12 flips, so the row deliberately shows
		// two different quantities and owes the reader the reason.
		const chunky = omen({
			legs: [omenBuy(), omenSell({ price: 0.4, priceItemQty: 5, priceQuoteQty: 2 }), omenConvert()]
		});

		const title = routeSlots(chunky, DIVINE_RATE)?.sell.rateTitle;

		expect(title).toContain('multiples of 5');
		expect(title).toContain('12 is not one order');
		expect(title).toContain('the largest order that fits');
		expect(title).toContain('still count the whole run of 12');
		expect(title).toContain('2 of the 12 bought stay unsold');
	});

	it('buys the whole run when the buy market posts one at a time', () => {
		// `expectedRoi` 40c needs `ceil(100 / 40)` = 3 flips, and a one-for-35c
		// market posts any count: 3 for 105c, exactly the run.
		const tiny = omen({ expectedRoi: 40 });

		expect(routeSlots(tiny, DIVINE_RATE)?.buy.rate).toBe('buy 3 for 105c');
		expect(routeSlots(tiny, DIVINE_RATE)?.buy.rateTitle).toBeNull();
	});

	it('posts one whole lot when the quantity bought is under the market’s lot', () => {
		// 3 omens against a four-for-one-divine market: `floor(3 / 4)` is 0, and
		// "sell 0 for 0 div" is not a reading. One whole lot is the smallest order
		// this market accepts, even though it is more than the play holds.
		const tiny = omen({ expectedRoi: 40 });

		expect(routeSlots(tiny, DIVINE_RATE)?.sell.rate).toBe('sell 4 for 1 div');
	});

	it('says on an overshooting step that its lot is bigger than the play', () => {
		// The undershoot wording ("the ends still count the whole run") would be a
		// lie here — the ORDER is the larger of the two, and a reader who acts on
		// it needs a fourth omen the run never bought.
		const tiny = omen({ expectedRoi: 40 });

		const title = routeSlots(tiny, DIVINE_RATE)?.sell.rateTitle;

		expect(title).toContain('posts 4 at a time');
		expect(title).toContain('more than the 3 this step moves');
		expect(title).toContain('bigger than the run the two ends are priced for');
	});

	it('falls back to the decimal rate for a leg whose pair came through zeroed', () => {
		// Version skew — this app caches no response, but it does point at a
		// configurable server, so a build carrying these fields can be aimed at one
		// from before POE-193. "buy 12 for 0" would be a wrong order; the decimal
		// the page printed before is merely an awkward one.
		const skewed = omen({
			legs: [omenBuy({ priceItemQty: 0, priceQuoteQty: 0 }), omenSell(), omenConvert()]
		});

		expect(routeSlots(skewed, DIVINE_RATE)?.buy.rate).toBe('buy 12 @ 35.00 c');
	});

	it('falls back to the decimal rate for a leg whose pair is absent entirely', () => {
		// The shape an older server actually sends: no such keys in the JSON at
		// all, which reaches the renderer as `undefined` however the type reads.
		// The cast is the point of the case — a guard written as `=== 0` would let
		// this through and multiply `undefined` into a NaN order.
		const skewed = omen({
			legs: [
				omenBuy({
					priceItemQty: undefined as unknown as number,
					priceQuoteQty: undefined as unknown as number
				}),
				omenSell(),
				omenConvert()
			]
		});

		expect(routeSlots(skewed, DIVINE_RATE)?.buy.rate).toBe('buy 12 @ 35.00 c');
	});

	it('sells against the run itself when the buy step could post no quantity', () => {
		// A skewed buy leg posts nothing, so there is no bought quantity for step 2
		// to be capped by — the run is the only target left, and dropping the sell
		// quantity along with the buy one would lose the step's whole order.
		const skewed = omen({
			legs: [omenBuy({ priceItemQty: 0, priceQuoteQty: 0 }), omenSell(), omenConvert()]
		});

		expect(routeSlots(skewed, DIVINE_RATE)?.sell.rate).toBe('sell 12 for 3 div');
	});

	it('draws no route for a play that arrives with fewer than two legs', () => {
		expect(routeSlots(play({ legs: [leg()] }), DIVINE_RATE)).toBeNull();
	});
});

describe('anyConvertStep', () => {
	/**
	 * One fully-shaped leg, reused for every position. `anyConvertStep` reads the
	 * LENGTH of the leg list and nothing inside a leg, so the cases below vary the
	 * count and hold the contents fixed — a fixture that also varied the contents
	 * would leave it unclear which of the two the answer came from.
	 */
	const someLeg: CurrencyExchangeLeg = {
		action: 'buy',
		item: 'scarab',
		quote: CHAOS_ID,
		price: 19,
		priceItemQty: 1,
		priceQuoteQty: 19,
		fair: 19.5,
		fairOk: true,
		tick: 0.01,
		volume: 2000,
		stock: 40,
		suspect: false,
		itemName: 'Ambush Scarab',
		itemIcon: '/icon/Scarab',
		itemCategory: 'Scarabs',
		quoteName: 'Chaos Orb',
		quoteIcon: '/currency-exchange/icon/Chaos',
		quoteCategory: 'Currency'
	};

	/** A play with `count` legs — two is a direct flip, three a 1-hop route. */
	function withLegs(count: number, overrides: Partial<CurrencyExchangePlay> = {}) {
		return play({ legs: Array.from({ length: count }, () => someLeg), ...overrides });
	}

	it('collapses the column when every rendered play is a direct flip', () => {
		// The case the collapse exists for: a table filtered to direct plays draws
		// an empty dashed tile and two arrows on every row, and the convert column
		// is dead width down the whole table.
		expect(anyConvertStep([withLegs(2), withLegs(2), withLegs(2)])).toBe(false);
	});

	it('shows the column when one rendered play converts', () => {
		// All-or-nothing over the set: the single 1-hop row needs the slot, so
		// every direct row around it keeps holding it open. Collapsing the rest
		// per-row would slide their Get under this row's sell.
		expect(anyConvertStep([withLegs(2), withLegs(3), withLegs(2)])).toBe(true);
	});

	it('reads the legs and not the mode label a play carries', () => {
		// `routeSlots` draws the convert slot off the THIRD LEG, by position. A
		// play labelled 1-hop that arrived with two legs has no third step to put
		// in the slot, so reserving one would spend the width on a tile nothing
		// can fill.
		expect(anyConvertStep([withLegs(2, { mode: '1-hop' })])).toBe(false);
	});

	it('collapses on an empty set rather than reserving a slot for no row', () => {
		// Unobservable in practice — the table is not rendered when nothing
		// survives the filters — but the answer is pinned so the collapse cannot
		// flip to "shown" on the way through an empty render.
		expect(anyConvertStep([])).toBe(false);
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
