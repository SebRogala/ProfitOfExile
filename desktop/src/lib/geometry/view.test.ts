import { describe, it, expect } from 'vitest';
import { ocrRectsKey, screenGeometryView } from './view';
import type { ManualOcrRegions, OcrRegionsSnapshot, Placements, ScreenSlice } from '$lib/stores/ssot.svelte';

/** The reference measurement — 1920x1200 at 1.0 IS the reference fixture. */
const referenceScreen: ScreenSlice = {
	width: 1920,
	height: 1200,
	uiScale: 1.0,
	source: 'merc-frame',
	measuredAtMs: 1_700_000_000_000,
	verifiedThisSession: true,
	monitorId: 65_537,
	origin: [0, 0],
	client: [0, 0, 1920, 1200],
	anchors: null
};

const NOW = new Date(1_700_000_000_000 + 2 * 60 * 60 * 1000);

const placements: Placements = {
	temple: { entranceOrigin: [960, 713] },
	merc: { panel: [698, 615, 555, 477] },
	exchange: { panel: [240, 115, 775, 850] },
	lab: {
		gem: [30, 45, 550, 75],
		font: [460, 270, 530, 350],
		gemSlots: [
			[506, 373, 48, 48],
			[605, 373, 48, 48],
			[704, 373, 48, 48]
		]
	}
};

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

	it('renders the capture-relative placements projected with the screen', () => {
		const view = screenGeometryView(referenceScreen, NOW, placements);

		expect(view.placements).toEqual({
			labGem: '[30, 45, 550, 75]',
			labFont: '[460, 270, 530, 350]',
			templeEntrance: '(960, 713)',
			mercPanel: '[698, 615, 555, 477]'
		});
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

	it('names the temple as a cue of its own, not as a merc one', () => {
		// The temple became a writer of this slice in POE-234, and the card is
		// where a user sees WHICH module answered. A source Rust emits that this
		// table does not know falls through to the verbatim branch below, which
		// would print the wire string — legible, but it would stop saying what
		// looked at the screen.
		const view = screenGeometryView({ ...referenceScreen, source: 'temple-anchor' }, NOW);

		expect(view.source).toBe('temple Entrance plate');
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

	it('names the capture derivation rather than a previous run for a Recalibrate press', () => {
		// A press measured THIS run's screen and derived the scale from its
		// height, so 'trusted from last session' would be false on both halves.
		// What it shares with the other unverified sources is only that no cue
		// looked at the game's art.
		const view = screenGeometryView(
			{ ...referenceScreen, source: 'capture', verifiedThisSession: false },
			NOW
		);

		expect(view.verified).toBe("No — derived from this screen's resolution");
	});

	it('reads the capture source in words', () => {
		const view = screenGeometryView({ ...referenceScreen, source: 'capture' }, NOW);

		expect(view.source).toBe("derived from this screen's resolution");
	});

	it('reports how long ago the measurement was taken, against the clock passed in', () => {
		const view = screenGeometryView(referenceScreen, NOW);

		expect(view.measured).toBe('2 h ago');
	});
});

describe('ocrRectsKey', () => {
	// The bug: rows loaded while nothing was measured stayed "unlocated" after
	// Recalibrate measured the screen.
	it('changes when an unmeasured screen is measured', () => {
		expect(ocrRectsKey(referenceScreen, placements)).not.toBe(ocrRectsKey(null, null));
	});

	it('changes when the scale changes', () => {
		expect(ocrRectsKey({ ...referenceScreen, uiScale: 0.9 }, placements)).not.toBe(
			ocrRectsKey(referenceScreen, placements)
		);
	});

	it('changes when a module remembers an anchor', () => {
		const anchored = { ...referenceScreen, anchors: { templeEntrance: [961, 714] as [number, number], mercPanel: null } };

		expect(ocrRectsKey(anchored, placements)).not.toBe(ocrRectsKey(referenceScreen, placements));
	});

	// The store restamps a re-measured slice without moving any row; reloading on
	// that would spend an IPC per merc tick while Settings is open.
	it('stays the same when only the measurement time changes', () => {
		expect(ocrRectsKey({ ...referenceScreen, measuredAtMs: 1_800_000_000_000 }, placements)).toBe(
			ocrRectsKey(referenceScreen, placements)
		);
	});

	describe('at an unchanged Auto slice', () => {
		const savedSet: ManualOcrRegions = {
			setOn: { width: 1920, height: 1200, origin: [0, 0], at_ms: 1_700_000_000_000 },
			edited: true,
			labGem: [30, 45, 550, 75],
			labFont: [460, 270, 530, 350],
			labGemSlots: [506, 373, 246, 48],
			templePanel: null,
			mercPanel: [698, 615, 555, 477]
		};
		const autoMode: OcrRegionsSnapshot = {
			mode: 'auto',
			manual: savedSet,
			auto: placements,
			screenWarning: null
		};
		const manualMode: OcrRegionsSnapshot = { ...autoMode, mode: 'manual' };

		// keep:critical
		// Consequence (silent): the Settings OCR rows keep the geometry and sources
		// of the previous mode or set, so the player trusts stale rects and Preview
		// frames that the readers no longer use.
		it.each<[string, OcrRegionsSnapshot, OcrRegionsSnapshot]>([
			['the mode switches from Auto to Manual', autoMode, manualMode],
			['the mode switches from Manual to Auto', manualMode, autoMode],
			[
				'a Manual save moves one region',
				manualMode,
				{ ...manualMode, manual: { ...savedSet, labFont: [500, 290, 530, 350] } }
			],
			[
				'a Manual save places a region that was not set',
				manualMode,
				{ ...manualMode, manual: { ...savedSet, templePanel: [1100, 4, 405, 338] } }
			],
			[
				'the live game monitor stops matching the Manual set',
				manualMode,
				{
					...manualMode,
					screenWarning: {
						setOn: savedSet.setOn,
						live: { width: 2560, height: 1440, origin: [0, 0], at_ms: 1_700_000_100_000 }
					}
				}
			]
		])('changes when %s', (_scenario, before, after) => {
			expect(ocrRectsKey(referenceScreen, placements, after)).not.toBe(
				ocrRectsKey(referenceScreen, placements, before)
			);
		});
	});
});
