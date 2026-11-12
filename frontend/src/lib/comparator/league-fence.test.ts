import { describe, expect, it } from 'vitest';
import { createLeagueReadGuard, createLeagueTransition } from './league-fence';

describe('createLeagueReadGuard', () => {
	it('rejects a lease captured before invalidate()', () => {
		const guard = createLeagueReadGuard(() => 'A');
		const lease = guard.capture();
		guard.invalidate();
		expect(lease()).toBe(false);
	});

	it('accepts a current-league result on a live lease', () => {
		const guard = createLeagueReadGuard(() => 'A');
		expect(guard.capture()({ league: 'A' })).toBe(true);
	});

	it('rejects a foreign-league result on a live lease', () => {
		const guard = createLeagueReadGuard(() => 'A');
		expect(guard.capture()({ league: 'B' })).toBe(false);
	});

	it('compares the result against the league at check time, not at capture', () => {
		let league = 'A';
		const lease = createLeagueReadGuard(() => league).capture();
		league = 'B';
		expect(lease({ league: 'A' })).toBe(false);
		expect(lease({ league: 'B' })).toBe(true);
	});

	it('accepts a lease captured after invalidate() while the older lease stays rejected', () => {
		const guard = createLeagueReadGuard(() => 'A');
		const older = guard.capture();
		guard.invalidate();
		const newer = guard.capture();
		expect(newer()).toBe(true);
		expect(older()).toBe(false);
	});
});

describe('createLeagueTransition', () => {
	it.each(['A', ''])('reports no change on the first call with %j', (league) => {
		expect(createLeagueTransition()(league)).toBe(false);
	});

	it('reports no change when the same league repeats', () => {
		const changed = createLeagueTransition();
		changed('A');
		expect(changed('A')).toBe(false);
	});

	it('reports a change from A to B', () => {
		const changed = createLeagueTransition();
		changed('A');
		expect(changed('B')).toBe(true);
	});

	it('reports each step of A to B to A as a change', () => {
		const changed = createLeagueTransition();
		changed('A');
		expect(changed('B')).toBe(true);
		expect(changed('A')).toBe(true);
	});

	it('reports a change from the unresolved empty league to a loaded one', () => {
		const changed = createLeagueTransition();
		changed('');
		expect(changed('A')).toBe(true);
	});
});
