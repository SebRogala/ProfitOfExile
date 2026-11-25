/**
 * The region editor's draft (POE-411, ADR-036): one picked Manual OCR region
 * at a time, edited on the game monitor and saved as the whole set in one
 * `set_manual_ocr_regions` call.
 *
 * Every rect held here is capture-relative PHYSICAL px. The editor window
 * covers the game monitor, so a window-local physical rect is already
 * capture-relative; the monitor's virtual-desktop origin never enters a rect.
 */
import type { OcrRegionsSnapshot } from '$lib/stores/ssot.svelte';
import type { ResizeEdge, WidgetRect } from './widgets/widget-geometry';

export type RegionRect = [number, number, number, number];

/** The five Manual regions, named as `ManualOcrRegions` names them. */
export type EditableRegion = 'labGem' | 'labFont' | 'labGemSlots' | 'templePanel' | 'mercPanel';

export type RectField = 'x' | 'y' | 'w' | 'h';

/** The game monitor the editor window covers: physical px, virtual-desktop origin. */
export interface EditorMonitor {
	x: number;
	y: number;
	width: number;
	height: number;
	scaleFactor: number;
}

export interface RegionEditorInput {
	snapshot: OcrRegionsSnapshot;
	monitor: EditorMonitor;
	/** The Auto slice's UI scale, or null with no slice. */
	uiScale: number | null;
	/** Open straight in this region's edit (a Settings row's Edit), else the picker. */
	select?: EditableRegion | null;
}

/** The editor's state. Read it through `editorView` and `savePayload`. */
export interface RegionEditorDraft {
	readonly monitor: EditorMonitor;
	readonly view: 'picker' | 'edit';
	readonly selected: EditableRegion | null;
	readonly rects: Readonly<Record<EditableRegion, RegionRect | null>>;
}

export type RegionEditorAction =
	| { type: 'pick'; region: EditableRegion }
	/** A move gesture: CSS px the pointer travelled since it started on `from`. */
	| { type: 'drag'; from: RegionRect; dx: number; dy: number }
	/** A resize gesture: CSS px the `edge` travelled since it started on `from`. */
	| { type: 'resize'; from: RegionRect; edge: ResizeEdge; dx: number; dy: number }
	/** Enter in a field: apply the typed physical value. */
	| { type: 'typeField'; field: RectField; value: string }
	| { type: 'key'; key: string; shift: boolean; alt: boolean }
	| { type: 'back' }
	| { type: 'resetToAuto' }
	| { type: 'leaveChoice'; choice: 'keep' | 'discard' }
	| { type: 'saveFailed'; error: string }
	/** `set_manual_ocr_regions` returned Ok; `snapshot` is the stored set after it. */
	| { type: 'saved'; snapshot: OcrRegionsSnapshot };

/** What the route draws and enables. */
export interface RegionEditorView {
	view: 'picker' | 'edit';
	/** The edited region; in the picker, the focused row. */
	selected: EditableRegion | null;
	/** The picked region's draft rect, capture-relative physical px. */
	frame: RegionRect | null;
	/** Where the route draws `frame`: window-local CSS px. */
	frameCss: WidgetRect | null;
	/** The Gem choice slots strip a Font panel edit carries. */
	carried: RegionRect | null;
	/** The picked region's Auto rect, drawn as the ghost. */
	autoGhost: RegionRect | null;
	/** The draft-vs-stored change tag in the edit panel title. */
	tag: 'moved' | 'resized' | 'new' | null;
	invalidField: RectField | null;
	/** The hint line: data, or the error. */
	hint: string;
	saveEnabled: boolean;
	resetEnabled: boolean;
	resetTitle: string;
	/** The Keep editing | Discard | Save row is up. */
	leavePrompt: boolean;
}

/** Rust `ManualRegions`, as `set_manual_ocr_regions` receives it under `regions`. */
export interface ManualRegionsWire {
	set_on: { width: number; height: number; origin: [number, number]; at_ms: number };
	edited: boolean;
	lab_gem: RegionRect | null;
	lab_font: RegionRect | null;
	lab_gem_slots: RegionRect | null;
	temple_panel: RegionRect | null;
	merc_panel: RegionRect | null;
}

const NO_RECTS: Readonly<Record<EditableRegion, RegionRect | null>> = {
	labGem: null,
	labFont: null,
	labGemSlots: null,
	templePanel: null,
	mercPanel: null
};

export function createRegionEditorDraft(input: RegionEditorInput): RegionEditorDraft {
	return { monitor: input.monitor, view: 'picker', selected: null, rects: NO_RECTS };
}

export function reduceRegionEditor(
	draft: RegionEditorDraft,
	_action: RegionEditorAction
): RegionEditorDraft {
	return draft;
}

export function editorView(draft: RegionEditorDraft): RegionEditorView {
	return {
		view: draft.view,
		selected: draft.selected,
		frame: null,
		frameCss: null,
		carried: null,
		autoGhost: null,
		tag: null,
		invalidField: null,
		hint: '',
		saveEnabled: false,
		resetEnabled: false,
		resetTitle: '',
		leavePrompt: false
	};
}

export function savePayload(draft: RegionEditorDraft): ManualRegionsWire {
	return {
		set_on: { width: draft.monitor.width, height: draft.monitor.height, origin: [0, 0], at_ms: 0 },
		edited: false,
		lab_gem: draft.rects.labGem,
		lab_font: draft.rects.labFont,
		lab_gem_slots: draft.rects.labGemSlots,
		temple_panel: draft.rects.templePanel,
		merc_panel: draft.rects.mercPanel
	};
}
