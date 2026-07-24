import { describe, it, expect } from 'vitest';
import { baseGemTradeUrl, cheapestCorrupted2123TradeUrl } from './trade-utils';

/**
 * Decode the league from the `/trade/search/<league>` path segment of a built
 * trade URL. Asserting on the decoded segment (not a substring of the raw URL)
 * is what makes these tests mutation-resistant: re-adding a `|| 'Mirage'`
 * fallback would change this exact segment, so a passing test proves the
 * caller-supplied league is the one that reached the URL.
 */
function leaguePathSegment(url: string): string {
	const { pathname } = new URL(url);
	const prefix = '/trade/search/';
	expect(pathname.startsWith(prefix)).toBe(true);
	return decodeURIComponent(pathname.slice(prefix.length));
}

describe('baseGemTradeUrl', () => {
	it('puts the caller-supplied league in the trade search path', () => {
		const url = baseGemTradeUrl('Kinetic Blast of Clustering', '20/20', 'Settlers');
		expect(leaguePathSegment(url)).toBe('Settlers');
	});

	it('emits an empty league segment (not the removed Mirage fallback) for an empty league', () => {
		// The point of POE-128: no hardcoded default. Re-adding `|| 'Mirage'`
		// would make this segment 'Mirage' and fail here.
		const url = baseGemTradeUrl('Kinetic Blast of Clustering', '20/20', '');
		const segment = leaguePathSegment(url);
		expect(segment).toBe('');
		expect(segment).not.toBe('Mirage');
	});
});

describe('cheapestCorrupted2123TradeUrl', () => {
	it('puts the caller-supplied league in the trade search path', () => {
		const url = cheapestCorrupted2123TradeUrl('RED', false, 'Settlers');
		expect(leaguePathSegment(url)).toBe('Settlers');
	});

	it('emits an empty league segment (not the removed Mirage fallback) for an empty league', () => {
		const url = cheapestCorrupted2123TradeUrl('RED', false, '');
		const segment = leaguePathSegment(url);
		expect(segment).toBe('');
		expect(segment).not.toBe('Mirage');
	});
});
