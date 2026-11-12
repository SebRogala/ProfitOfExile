/** Mirrored byte-for-byte in frontend/src/lib/comparator/; imports nothing so both copies behave alike. */

/** Component-owned reads use a captured lease; invalidation never writes a second store. */
export function createLeagueReadGuard(league: () => string | null) {
	let generation = 0;
	return {
		invalidate() { generation++; },
		capture() {
			const mine = generation;
			return (result?: { league: string }) => mine === generation && (!result || result.league === league());
		}
	};
}

/** First call records the league and reports no change; later calls report a change from the previous call. */
export function createLeagueTransition() {
	let previous: string | undefined;
	return (league: string) => {
		const changed = previous !== undefined && previous !== league;
		previous = league;
		return changed;
	};
}
