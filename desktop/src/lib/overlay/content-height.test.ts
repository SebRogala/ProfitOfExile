import { describe, expect, it } from 'vitest';
import { HEIGHT_EPSILON_PX, overlayHeightRequest } from './content-height';

describe('deciding whether to ask for a resize', () => {
	it('asks for the first height it ever measures', () => {
		expect(overlayHeightRequest(212, null)).toBe(212);
	});

	it('asks again when the panel has actually grown', () => {
		expect(overlayHeightRequest(240, 212)).toBe(240);
	});

	it('asks again when the panel has shrunk', () => {
		expect(overlayHeightRequest(96, 212)).toBe(96);
	});

	// Without this the window collapses to Rust's floor and back on every
	// module start — a visible flicker over the game.
	it('ignores the zero the route reports while it is mounting', () => {
		expect(overlayHeightRequest(0, null)).toBeNull();
	});

	it('ignores a negative height', () => {
		expect(overlayHeightRequest(-4, 212)).toBeNull();
	});

	it('ignores a height that is not a number at all', () => {
		expect(overlayHeightRequest(Number.NaN, 212)).toBeNull();
	});

	// Font metrics settle a fraction of a pixel away from where they were, and
	// without a threshold that is one Rust call per animation frame forever.
	it('ignores sub-pixel jitter around the height it already asked for', () => {
		expect(overlayHeightRequest(212.3, 212)).toBeNull();
	});

	it('asks once the drift reaches a whole pixel', () => {
		expect(overlayHeightRequest(212 + HEIGHT_EPSILON_PX, 212)).toBe(213);
	});

	// Comparing against the last OBSERVED height would let a drift of
	// sub-epsilon steps walk the window anywhere without ever tripping the
	// threshold. This pins that the baseline is the last height SENT.
	it('measures the drift from the last height it asked for, not the last one seen', () => {
		expect(overlayHeightRequest(212.6, 212)).toBeNull();
		expect(overlayHeightRequest(213.1, 212)).toBe(213.1);
	});
});
