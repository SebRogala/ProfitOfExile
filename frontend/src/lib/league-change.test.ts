import { describe, it, expect } from 'vitest';
import { isLeagueChange } from './api';

describe('isLeagueChange', () => {
	it('reports a change when the event names a different league', () => {
		expect(isLeagueChange({ topic: 'poe/league/active', league: 'Keepers', leagueRevision: 3 }, 'Mercenaries')).toBe(true);
	});

	it('reports no change when the event names the held league', () => {
		expect(isLeagueChange({ topic: 'poe/league/active', league: 'Mercenaries', leagueRevision: 3 }, 'Mercenaries')).toBe(false);
	});

	it('reports a change when the page holds no league yet and the event names one', () => {
		expect(isLeagueChange({ league: 'Keepers' }, '')).toBe(true);
	});

	it.each([
		['no league field', { topic: 'poe/league/active' }],
		['a non-string league', { league: 42 }],
		['a null payload', null],
		['an undefined payload', undefined],
	])('reports no change for %s', (_label, data) => {
		expect(isLeagueChange(data, 'Mercenaries')).toBe(false);
	});
});
