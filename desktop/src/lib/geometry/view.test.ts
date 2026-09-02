import { describe, it, expect } from 'vitest';
import { screenGeometryView } from './view';
import type { ScreenSlice } from '$lib/stores/ssot.svelte';

/** The reference measurement — 1920x1200 at 1.0 IS the reference fixture. */
const referenceScreen: ScreenSlice = {
	width: 1920,
	height: 1200,
	uiScale: 1.0,
	source: 'merc-frame',
	measuredAtMs: 1_700_000_000_000,
	verifiedThisSession: true
};

const NOW = new Date(1_700_000_000_000 + 2 * 60 * 60 * 1000);

describe('screenGeometryView', () => {
	it('never prints a number for an unmeasured screen', () => {
		// THE rule of this file. 1.0 is a real measurement, so a card that
		// printed it here would be indistinguishable from one reporting a
		// genuine 1920x1200 monitor — and every rect derived from it on a 1080p
		// machine would be 11% wrong with nothing on screen to say so.
		const view = screenGeometryView(null, NOW);

		expect(view.unmeasured).toBe(true);
		// Not "is not '1.000'" — anything a reader could take for a measurement
		// is wrong here, so the check is that the field does not READ as a
		// number at all. (`Number('')` is 0, so the empty string fails this too.)
		expect(Number.isNaN(Number(view.uiScale))).toBe(true);
		expect(view.resolution).not.toMatch(/\d+×\d+/);
	});

	it('does not claim a verification for a screen nothing has measured', () => {
		// "Yes" here would be the worst cell on the card: a confirmation of a
		// screen the app has never looked at. The unmeasured branch owes a
		// reason like the other three rows, not an answer.
		const view = screenGeometryView(null, NOW);

		expect(view.verified).toBe('Not measured yet');
	});

	it('prints the measured resolution and scale', () => {
		const view = screenGeometryView(referenceScreen, NOW);

		expect(view.unmeasured).toBe(false);
		expect(view.resolution).toBe('1920×1200');
		expect(view.uiScale).toBe('1.000');
	});

	it('keeps three decimals of scale, because the deadband is 0.01', () => {
		// A 1080p machine measures 0.90 and one grid step of the merc fit moves
		// the number by 0.0031. Two decimals would hide a change the publish
		// gate calls real; this pins the resolution of the printed value.
		const view = screenGeometryView({ ...referenceScreen, uiScale: 0.9034 }, NOW);

		expect(view.uiScale).toBe('0.903');
	});

	it('spells out the cue that measured the scale', () => {
		const view = screenGeometryView({ ...referenceScreen, source: 'merc-ocr' }, NOW);

		expect(view.source).toBe('merc OCR line pitch');
	});

	it('shows an unrecognised source verbatim rather than inventing a name for it', () => {
		// A source this table does not know is a Rust-side addition. Mapping it
		// to a friendly guess would hide exactly the fact worth seeing.
		const view = screenGeometryView(
			{ ...referenceScreen, source: 'lab-region' as ScreenSlice['source'] },
			NOW
		);

		expect(view.source).toBe('lab-region');
	});

	it('says the screen was verified when a verifying cue measured it this run', () => {
		const view = screenGeometryView(referenceScreen, NOW);

		expect(view.verified).toBe('Yes');
	});

	it('names where an unverified number came from instead of just saying no', () => {
		// A launch that has not opened a recruit window yet is running on last
		// session's scale — normal, not broken. A bare "No" would read as a
		// fault; the row has to say what the reader is actually looking at.
		const view = screenGeometryView({ ...referenceScreen, verifiedThisSession: false }, NOW);

		expect(view.verified).toBe('No — trusted from last session');
	});

	it('reports how long ago the measurement was taken, against the clock passed in', () => {
		const view = screenGeometryView(referenceScreen, NOW);

		expect(view.measured).toBe('2 h ago');
	});
});
