import { describe, it, expect } from 'vitest';
import { defaultSelectedGem, type SelectableGem } from './comparator-selection';
import type { SellabilityLabel } from './api';

/**
 * A gem the overlay could render, reduced to the fields the rule reads.
 * The label defaults to '' — the server sends that for any gem it has no
 * signal row for, and for every row in Dedication mode.
 */
function gem(name: string, transPrice: number, sellabilityLabel: SellabilityLabel = ''): SelectableGem {
	return { name, transPrice, sellabilityLabel };
}

describe('defaultSelectedGem', () => {
	it('picks the most expensive gem when nothing is selected', () => {
		const results = [gem('Cheap', 12), gem('Dear', 340), gem('Middling', 90)];

		expect(defaultSelectedGem(results, null)).toBe('Dear');
	});

	it('picks the most expensive gem even when it is not the one the server recommends', () => {
		// The regression this rule exists to close: a second rule in the overlay's
		// poll took the first gem with recommendation 'BEST' and ran before this
		// one, so the two disagreed whenever the recommended gem was not the
		// dearest. The recommendation is not an input here at all — a payload
		// carrying one cannot change the answer.
		const results = [
			{ ...gem('Recommended', 40), recommendation: 'BEST' as const },
			{ ...gem('Dearest', 500), recommendation: 'OK' as const },
		];

		expect(defaultSelectedGem(results, null)).toBe('Dearest');
	});

	it('keeps the current selection instead of re-deciding it', () => {
		// The user's pick outranks the default: this runs on every poll, so
		// re-deciding would drag the selection back off whatever they clicked.
		const results = [gem('Cheap', 12), gem('Dear', 340)];

		expect(defaultSelectedGem(results, 'Cheap')).toBe('Cheap');
	});

	it('clears the selection when the results go empty', () => {
		expect(defaultSelectedGem([], 'Dear')).toBeNull();
	});

	it('keeps the first gem when two share the top price', () => {
		// Rendering order decides the tie, so the pick matches the row the player
		// reads first rather than moving with the payload's iteration order.
		const results = [gem('First', 200), gem('Second', 200)];

		expect(defaultSelectedGem(results, null)).toBe('First');
	});

	it('picks a gem priced at zero over nothing when every gem is unpriced', () => {
		// NO_DATA gems carry transPrice 0. A rule that only accepted a strictly
		// positive price would leave the overlay with no selection at all, and the
		// pick button would silently do nothing.
		const results = [gem('Unpriced', 0), gem('AlsoUnpriced', 0)];

		expect(defaultSelectedGem(results, null)).toBe('Unpriced');
	});

	it('passes over the dearest gem when it is UNLIKELY and a rival clears half its price', () => {
		// The gem nobody is buying is a worse default than the cheaper one that
		// moves: 400 halved is 200, which 260 beats.
		const results = [gem('DearButDead', 400, 'UNLIKELY'), gem('Moves', 260, 'GOOD')];

		expect(defaultSelectedGem(results, null)).toBe('Moves');
	});

	it('keeps the dearest gem when it is UNLIKELY but no rival clears half its price', () => {
		// The discount is a demotion, not a veto. 400 halved is 200, and the
		// liquid rival at 90 is not worth giving up 310c of headline price for.
		const results = [gem('DearButDead', 400, 'UNLIKELY'), gem('Moves', 90, 'FAST SELL')];

		expect(defaultSelectedGem(results, null)).toBe('DearButDead');
	});

	it('keeps the dearest gem when a rival lands exactly on half its price', () => {
		// The boundary is exclusive: matching the discounted price is not beating
		// it, so the dearer gem holds the pick.
		const results = [gem('DearButDead', 400, 'UNLIKELY'), gem('Moves', 200, 'GOOD')];

		expect(defaultSelectedGem(results, null)).toBe('DearButDead');
	});

	it('falls back to price order when every gem is UNLIKELY', () => {
		// Nothing on offer sells, so the discount tells the player nothing and the
		// dearest gem is still the best of a bad set.
		const results = [gem('Cheap', 30, 'UNLIKELY'), gem('Dear', 340, 'UNLIKELY')];

		expect(defaultSelectedGem(results, null)).toBe('Dear');
	});

	it('leaves a SLOW gem at full price', () => {
		// UNLIKELY is the only label the rule discounts. SLOW is the label one
		// band above it, and a gem that sells slowly is not one nobody is buying.
		const results = [gem('DearAndSlow', 400, 'SLOW'), gem('Moves', 260, 'FAST SELL')];

		expect(defaultSelectedGem(results, null)).toBe('DearAndSlow');
	});

	it('leaves an unlabelled gem at full price', () => {
		// An empty label is a missing signal row, not a verdict — it is what every
		// row carries in Dedication mode. Reading it as unsellable would demote a
		// whole mode's dearest gem on data the server never sent.
		const results = [gem('DearNoSignal', 400, ''), gem('Moves', 260, 'GOOD')];

		expect(defaultSelectedGem(results, null)).toBe('DearNoSignal');
	});
});
