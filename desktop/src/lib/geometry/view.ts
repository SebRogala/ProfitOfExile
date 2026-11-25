/**
 * Rendering decisions for the screen-geometry SSOT slice (POE-227).
 *
 * Pure, and separate from the Settings card that draws it, because the one rule
 * this file exists to keep is a rule about a MISSING value: `null` means nothing
 * has measured a screen, and it must never be rendered — or defaulted — as 1.0.
 * 1.0 is a real measurement (the 1920x1200 reference), so a card that printed it
 * for an unmeasured screen would look identical to one reporting a genuine
 * reference monitor, and every rect derived from it on a 1080p machine would be
 * 11% wrong with nothing on screen to say so.
 *
 * The lifecycle this surface describes is normative in
 * `desktop/src/lib/README.md` → "Screen Geometry (SSOT)".
 */

import type { OcrRegionsSnapshot, OcrScreenRef, Placements, ScreenSlice } from '$lib/stores/ssot.svelte';
import type { EditableRegion } from '$lib/overlay/ocr-region-editor';
import type { OcrRectView, RegionKey } from '$lib/pages/ocr-preview-request';
// Reused rather than re-implemented: `formatTimeAgo` already has three homes
// with three wordings (README → Conventions → Relative time), and a fourth would
// be the one that finally makes them disagree.
import { formatTimeAgo } from '$lib/exchange/view';

/**
 * The wire strings Rust's `ScreenScaleSource` serialises to, spelled out for a
 * reader.
 *
 * These say what LOOKED, never how much to trust it — how far a reading is
 * allowed to move the standing value is Rust's `ssot::accepts` rule and not a
 * ranking a reader has to apply. How confident to be is the separate `verified`
 * row below. An unrecognised string is shown verbatim rather than mapped to a
 * guess — a source this table does not know is a Rust-side addition, and
 * inventing a friendly name for it would hide that.
 */
const SOURCE_LABELS: Record<string, string> = {
	'merc-frame': 'merc support grid',
	'merc-ocr': 'merc OCR line pitch',
	'temple-anchor': 'temple Entrance plate',
	remembered: 'remembered from a previous run',
	capture: "derived from this screen's resolution"
};

/** The five rows the Settings "Screen geometry" card prints. */
export interface ScreenGeometryView {
	/** `"1920×1200"`, or why there is no answer. */
	resolution: string;
	/** The scale to three decimals, or why there is no answer. */
	uiScale: string;
	/** What measured it, in words. */
	source: string;
	/** How long ago, relative. */
	measured: string;
	/** Whether a verifying cue confirmed this screen in THIS run (POE-240), and
	 *  — when it did not — where the number came from instead. */
	verified: string;
	/** Nothing has measured a screen — every field above is a REASON, not a
	 *  value, and the card renders them muted. */
	unmeasured: boolean;
	/** Capture-relative placements projected alongside the screen slice. */
	placements: {
		labGem: string;
		labFont: string;
		templeEntrance: string;
		mercPanel: string;
	};
}

function rectText(rect: [number, number, number, number] | null): string {
	return rect ? `[${rect.join(', ')}]` : 'Unlocated';
}

function placementText(placements: Placements | null): ScreenGeometryView['placements'] {
	if (!placements) {
		return {
			labGem: 'Not available',
			labFont: 'Not available',
			templeEntrance: 'Not available',
			mercPanel: 'Not available'
		};
	}
	return {
		labGem: rectText(placements.lab.gem),
		labFont: rectText(placements.lab.font),
		templeEntrance: placements.temple
			? `(${placements.temple.entranceOrigin.join(', ')})`
			: 'Unlocated',
		mercPanel: placements.merc ? rectText(placements.merc.panel) : 'Unlocated'
	};
}

/** The `verified` row: whether a cue looked at the game's art, and — when none
 *  did — which of the two unverified states this is. */
function verifiedText(screen: ScreenSlice): string {
	if (screen.verifiedThisSession) return 'Yes';
	if (screen.source === 'capture') return "No — derived from this screen's resolution";
	return 'No — trusted from last session';
}

/**
 * What to print for `screen`, with `now` as the clock the relative age is
 * measured against (passed in so the caller owns the tick and the function stays
 * pure).
 */
export function screenGeometryView(
	screen: ScreenSlice | null,
	now: Date,
	placements: Placements | null = null,
	ocrRegions: OcrRegionsSnapshot | null = null
): ScreenGeometryView {
	const placed = placementText(placements);
	if (ocrRegions?.mode === 'manual') {
		const manual = ocrRegions.manual;
		placed.labGem = manual?.labGem ? rectText(manual.labGem) : 'Not set';
		placed.labFont = manual?.labFont ? rectText(manual.labFont) : 'Not set';
		placed.mercPanel = manual?.mercPanel ? rectText(manual.mercPanel) : 'Not set';
		placed.templeEntrance = manual?.templePanel ? placed.templeEntrance : 'Not set';
	}
	if (screen === null) {
		return {
			resolution: 'Not measured yet',
			// Says the rule out loud on the surface that would otherwise be the
			// easiest place to break it.
			uiScale: 'Not measured yet — never treat as 1.0',
			source: 'Nothing has measured this screen',
			measured: 'Never',
			verified: 'Not measured yet',
			unmeasured: true,
			placements: placed
		};
	}
	return {
		resolution: `${screen.width}×${screen.height}`,
		// Three decimals: the publish deadband is 0.01 and one grid step of the
		// merc fit moves the number by 0.0031, so two decimals would hide a real
		// change and four would print noise.
		uiScale: screen.uiScale.toFixed(3),
		source: SOURCE_LABELS[screen.source] ?? screen.source,
		measured: formatTimeAgo(new Date(screen.measuredAtMs), now),
		// The unverified wording names where the number came from instead, so
		// the row is never a bare "No" the reader has to interpret as a fault:
		// a trusted-from-last-session scale is the normal state of a launch that
		// has not opened a recruit window yet, not a broken one.
		//
		// `capture` needs its own wording rather than that one (POE-278): a
		// Recalibrate press measured THIS run's screen and then derived the
		// scale from its height, so "trusted from last session" would be false
		// on both halves — it was taken now, and no previous session is
		// involved. What it shares with the other unverified sources is only
		// that no cue looked at the game's art.
		verified: verifiedText(screen),
		unmeasured: false,
		placements: placed
	};
}

/**
 * What the Settings OCR rows (`get_ocr_rects`) are projected from, as a string
 * that changes only when a row can.
 *
 * The rows are Rust's projection of the placements plus the slice's scale,
 * dimensions and anchors, so a page that loaded them once kept "unlocated" after
 * a Recalibrate or a module measured the screen, until restart (2026-09-14). The
 * store replaces both objects on every poll and a fresh measurement restamps
 * `measuredAtMs`, so neither identity nor the whole slice is a usable trigger.
 */
export function ocrRectsKey(
	screen: ScreenSlice | null,
	placements: Placements | null,
	ocrRegions: OcrRegionsSnapshot | null = null
): string {
	return JSON.stringify([
		placements,
		screen && [screen.width, screen.height, screen.uiScale, screen.client, screen.anchors],
		ocrRegions && [ocrRegions.mode, ocrRegions.manual,
			ocrRegions.screenWarning && [ocrRegions.screenWarning.live.width,
				ocrRegions.screenWarning.live.height, ocrRegions.screenWarning.live.origin]]
	]);
}

const OCR_ROWS: { key: string; label: string; edit: EditableRegion | null; state: RegionKey }[] = [
	{ key: 'lab.gem', label: 'Gem tooltip', edit: 'labGem', state: 'lab_gem' },
	{ key: 'lab.font', label: 'Font panel', edit: 'labFont', state: 'lab_font' },
	{ key: 'lab.gemSlots', label: 'Gem choice slots', edit: 'labGemSlots', state: 'lab_gem_slots' },
	{ key: 'temple.panel', label: 'Temple panel', edit: 'templePanel', state: 'temple_panel' },
	{ key: 'temple.remaining', label: 'Temple remaining', edit: null, state: 'temple_panel' },
	{ key: 'merc.panel', label: 'Merc OCR crop', edit: 'mercPanel', state: 'merc_panel' }
];

function screenRefText(screen: OcrScreenRef): string {
	return `${screen.width}×${screen.height} @ (${screen.origin.join(', ')})`;
}

function autoSource(source: string | undefined): string {
	if (source === 'seed') return '(seed, scaled from reference)';
	if (source === 'remembered') return '(remembered anchor)';
	if (source === 'unlocated') return '(unlocated)';
	return '';
}

export function ocrRegionsView(
	snapshot: OcrRegionsSnapshot | null,
	rows: OcrRectView[] = [],
	now: Date = new Date()
) {
	const mode = snapshot?.mode ?? 'auto';
	const manual = mode === 'manual';
	const set = snapshot?.manual;
	const warning = manual ? snapshot?.screenWarning : null;
	const missing = OCR_ROWS.filter(row => row.edit && !set?.[row.edit]);
	return {
		mode,
		recalibrateDisabled: manual,
		warning: warning ? `Screen changed · set on ${screenRefText(warning.setOn)} · now ${screenRefText(warning.live)}` : '',
		firstUse: manual && set && !set.edited
			? `Copied from Auto · ${formatTimeAgo(new Date(set.setOn.at_ms), now)}${missing.length
				? ` · not set: ${missing.length === 5 ? 'all regions' : missing.map(row => row.edit === 'mercPanel' ? 'Merc panel' : row.label).join(', ')}` : ''}` : '',
		setOn: manual && set ? `Set on ${screenRefText(set.setOn)} · ${formatTimeAgo(new Date(set.setOn.at_ms), now)}` : '',
		rows: OCR_ROWS.filter(row => manual || row.key !== 'lab.gemSlots').map(config => {
			const previewKey = config.key === 'lab.gemSlots' ? 'lab.font' : config.key;
			const row = rows.find(row => row.key === previewKey);
			const saved = config.edit ? set?.[config.edit] ?? null : null;
			const rect = manual && config.edit
				? config.edit === 'mercPanel' ? row?.rect ?? saved : saved
				: row?.rect ?? null;
			const state = row?.regionStates?.[config.state];
			const pastEdge = manual && (state === 'clipped' || state === 'past-edge');
			const source = !manual ? autoSource(row?.source)
				: !config.edit ? '(follows Temple panel)'
				: !saved ? '(not read)'
				: pastEdge ? '(past the screen edge)'
				: config.edit === 'mercPanel' ? '(from Merc panel)'
				: set?.edited ? '(manual)' : '(manual, copied from Auto)';
			return { ...config, label: row && config.key !== 'lab.gemSlots' ? row.label : config.label,
				previewKey, value: rect ? rectText(rect) : manual ? 'not set' : 'unlocated', source, pastEdge,
				editEnabled: manual && config.edit !== null,
				previewDisabled: !row?.rect || (manual && config.edit !== null && !saved),
				previewTitle: row?.rect ? `Preview ${config.label}` : 'Not located yet — open the panel once' };
		})
	};
}
