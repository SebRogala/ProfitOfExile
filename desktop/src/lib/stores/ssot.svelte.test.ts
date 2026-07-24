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

	it('coerces a missing league slice to null instead of throwing', () => {
		applySnapshot({ league: { name: 'Settlers' } });
		// Malformed snapshot (e.g. an older/empty payload) must not leak undefined.
		applySnapshot({} as unknown as Parameters<typeof applySnapshot>[0]);
		expect(ssot.league).toBeNull();
	});
});
