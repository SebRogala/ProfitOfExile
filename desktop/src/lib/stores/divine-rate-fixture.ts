/**
 * MOCK — the fixture adapter behind `fetchDivineRate()` (POE-284). Deleted at
 * go-live, when the backend task replaces that function's body with the real
 * read of owner `internal/exchange`; nothing else changes then.
 *
 * `DIVINE_RATE_FIXTURE` is the `rate` object of
 * `design/handoffs/poe-284--desktop-app--top-bar--divine-rate/fixtures.json`,
 * verbatim. It is copied rather than imported because the desktop container
 * mounts only `./desktop`, so a path into `design/` would not resolve in tests.
 */
import type { DivineRate } from '$lib/api';

export const DIVINE_RATE_FIXTURE = {
	divineChaosRate: 360.07,
	priceHour: '2026-09-09T23:00:00Z',
	updatedMinutesAgo: 6,
	state: 'ready',
	_note: "state: 'ready' | 'cold' (no exchange hour yet → value null, shows '—') | 'stale' (server unreachable; keep the last value). Stale example: lastHour 21:00, unreachable since 21:04."
} as const satisfies { state: DivineRate['state'] } & Record<string, unknown>;

/**
 * The `_note`'s stale example ("lastHour 21:00, unreachable since 21:04"),
 * dated on the fixture's own `priceHour` day — the date is an interpretation,
 * logged in the handoff's NOTES.md.
 */
const STALE_PRICE_HOUR = '2026-09-09T21:00:00Z';
const STALE_UNREACHABLE_SINCE = '2026-09-09T21:04:00Z';

/**
 * One of the three drawn states as the port returns it. The product path
 * passes no `state`, so the app shows the fixture's own (`ready`); the other
 * two are reachable for verification.
 */
export function readDivineRateFixture(now: number, state: DivineRate['state'] = DIVINE_RATE_FIXTURE.state): DivineRate {
	const updatedAt = new Date(now - DIVINE_RATE_FIXTURE.updatedMinutesAgo * 60_000).toISOString();
	if (state === 'cold') {
		return { divineChaosRate: null, priceHour: null, updatedAt, state, unreachableSince: null };
	}
	if (state === 'stale') {
		return {
			divineChaosRate: DIVINE_RATE_FIXTURE.divineChaosRate,
			priceHour: STALE_PRICE_HOUR,
			updatedAt,
			state,
			unreachableSince: STALE_UNREACHABLE_SINCE
		};
	}
	return {
		divineChaosRate: DIVINE_RATE_FIXTURE.divineChaosRate,
		priceHour: DIVINE_RATE_FIXTURE.priceHour,
		updatedAt,
		state,
		unreachableSince: null
	};
}
