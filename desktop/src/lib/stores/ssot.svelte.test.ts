import { describe, it, expect } from 'vitest';
import { ssot, applySnapshot } from './ssot.svelte';

describe('applySnapshot', () => {
	it('maps league.name to the flat league field', () => {
		applySnapshot({ league: { name: 'Settlers' } });
		expect(ssot.league).toBe('Settlers');
	});

	it('maps a null league.name to null (fail-closed, not-yet-resolved)', () => {
		// Seed a resolved value first so a passing null-case proves the mapping
		// wrote null, not that it was already null.
		applySnapshot({ league: { name: 'Settlers' } });
		applySnapshot({ league: { name: null } });
		expect(ssot.league).toBeNull();
	});

	it('maps the resolving flag from the snapshot (true then false)', () => {
		applySnapshot({ league: { name: null }, resolving: true });
		expect(ssot.resolving).toBe(true);
		// A later snapshot with the flag cleared must flip it back — proves the
		// mapping writes the incoming value, not a one-way latch.
		applySnapshot({ league: { name: 'Mirage' }, resolving: false });
		expect(ssot.resolving).toBe(false);
	});

	it('defaults resolving to false when the field is absent', () => {
		applySnapshot({ league: { name: null }, resolving: true });
		applySnapshot({ league: { name: null } });
		expect(ssot.resolving).toBe(false);
	});

	it('maps the unreachable flag from the snapshot (true then false)', () => {
		applySnapshot({ league: { name: null }, resolving: true, unreachable: true });
		expect(ssot.unreachable).toBe(true);
		// A later snapshot with the flag cleared must flip it back — proves the
		// mapping writes the incoming value, not a one-way latch.
		applySnapshot({ league: { name: 'Mirage' }, resolving: false, unreachable: false });
		expect(ssot.unreachable).toBe(false);
	});

	it('defaults unreachable to false when the field is absent', () => {
		applySnapshot({ league: { name: null }, resolving: true, unreachable: true });
		applySnapshot({ league: { name: null } });
		expect(ssot.unreachable).toBe(false);
	});

	it('coerces a missing league slice to null instead of throwing', () => {
		applySnapshot({ league: { name: 'Settlers' } });
		// Malformed snapshot (e.g. an older/empty payload) must not leak undefined.
		applySnapshot({} as unknown as Parameters<typeof applySnapshot>[0]);
		expect(ssot.league).toBeNull();
	});
});
