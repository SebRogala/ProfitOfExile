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
import { clampToHost, dragged, resized, MIN_WIDGET_SIDE_CSS, type ResizeEdge, type WidgetRect } from './widgets/widget-geometry';
import { physicalHotRect } from './hot-rects';
import { CONFIG_BAR_GAP } from './widgets/widget-config-bar';
import { invoke } from '@tauri-apps/api/core';
import { PhysicalPosition, PhysicalSize } from '@tauri-apps/api/dpi';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { availableMonitors, currentMonitor, primaryMonitor } from '@tauri-apps/api/window';
import { chooseMonitor, type GameMonitorInfo } from './monitor-choice';

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
	readonly snapshot: OcrRegionsSnapshot;
	readonly uiScale: number | null;
	readonly stored: Readonly<Record<EditableRegion, RegionRect | null>>;
	readonly invalid: Partial<Record<RectField, string>>;
	readonly error: string;
	readonly pending: EditableRegion | 'picker' | null;
	readonly notice: string;
}

export type RegionEditorAction =
	| { type: 'pick'; region: EditableRegion }
	| { type: 'focus'; region: EditableRegion }
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

export const REGION_NAMES: Record<EditableRegion, string> = {
	labGem: 'Gem tooltip', labFont: 'Font panel', labGemSlots: 'Gem choice slots',
	templePanel: 'Temple panel', mercPanel: 'Merc panel'
};
export const EDITABLE_REGIONS = Object.keys(REGION_NAMES) as EditableRegion[];
export const RECT_FIELDS: RectField[] = ['x', 'y', 'w', 'h'];

export function shapeLocked(region: EditableRegion | null): boolean {
	return region === 'labGemSlots' || region === 'templePanel' || region === 'mercPanel';
}

function storedRects(snapshot: OcrRegionsSnapshot): Record<EditableRegion, RegionRect | null> {
	return Object.fromEntries(EDITABLE_REGIONS.map(key => [key, snapshot.manual?.[key]?.slice() ?? null])) as Record<EditableRegion, RegionRect | null>;
}

export function autoRegion(draft: RegionEditorDraft, region: EditableRegion): RegionRect | null {
	const auto = draft.snapshot.auto;
	if (!auto) return null;
	if (region === 'labGem') return auto.lab.gem;
	if (region === 'labFont') return auto.lab.font;
	if (region === 'labGemSlots') {
		const slots = auto.lab.gemSlots;
		return [slots[0][0], slots[0][1], slots[2][0] + slots[2][2] - slots[0][0], slots[0][3]];
	}
	if (region === 'mercPanel') return auto.merc?.panel ?? null;
	if (!auto.temple || !draft.uiScale) return null;
	// Mirrors ssot::panel_rect and TEMPLE_SCALE_PER_UI_SCALE for the Auto reference.
	const scale = draft.uiScale * 1.1111;
	const [x, y] = auto.temple.entranceOrigin;
	return [x + Math.round(171 * scale), y + Math.round(-709 * scale),
		Math.round(715 * scale) - Math.round(171 * scale), Math.round(-255 * scale) - Math.round(-709 * scale)];
}

export function templeRemaining(panel: RegionRect): RegionRect {
	// Mirrors ssot::origin_scale_for_panel and temple::run::remaining_rect.
	const scale = panel[2] / 544;
	const x = panel[0] - Math.round(171 * scale);
	const y = panel[1] - Math.round(-709 * scale);
	return [x - Math.round(150 * scale), y + Math.round(58 * scale),
		2 * Math.round(150 * scale), Math.round(104 * scale) - Math.round(58 * scale)];
}

export function stripSlots(strip: RegionRect): RegionRect[] {
	const scale = strip[2] / 273;
	const side = Math.round(53 * scale);
	return [0, 1, 2].map(i => [strip[0] + Math.round(i * 110 * scale), strip[1], side, side]);
}

function initialRect(draft: RegionEditorDraft, region: EditableRegion): RegionRect {
	const scale = draft.uiScale ?? draft.monitor.height / 1200;
	// Reference sizes mirror lib.rs Lab constants, ssot::panel_rect and MERC_PANEL_REF.
	const sizes: Record<EditableRegion, [number, number]> = {
		labGem: [draft.monitor.width / scale, 44], labFont: [740, 757],
		labGemSlots: [273, 53], templePanel: [544 * 1.1111, 454 * 1.1111], mercPanel: [555, 477]
	};
	const [rw, rh] = sizes[region];
	const fit = Math.min(scale, draft.monitor.width / rw, draft.monitor.height / rh);
	const w = Math.round(rw * fit), h = Math.round(rh * fit);
	return [Math.round((draft.monitor.width - w) / 2), Math.round((draft.monitor.height - h) / 2), w, h];
}

function pick(draft: RegionEditorDraft, region: EditableRegion): RegionEditorDraft {
	return { ...draft, view: 'edit', selected: region, invalid: {}, error: '', pending: null,
		rects: { ...draft.stored, [region]: draft.stored[region] ?? initialRect(draft, region) } };
}

export function createRegionEditorDraft(input: RegionEditorInput): RegionEditorDraft {
	const stored = storedRects(input.snapshot);
	const draft: RegionEditorDraft = { monitor: input.monitor, snapshot: input.snapshot, uiScale: input.uiScale,
		view: 'picker', selected: null, rects: stored, stored, invalid: {}, error: '', pending: null,
		notice: input.snapshot.manual?.edited === false ? 'Copied from Auto · just now' : '' };
	return input.select ? pick(draft, input.select) : draft;
}

function sameRect(a: RegionRect | null, b: RegionRect | null): boolean {
	return a === b || (!!a && !!b && a.every((value, i) => value === b[i]));
}

function changedRegions(draft: RegionEditorDraft): EditableRegion[] {
	return EDITABLE_REGIONS.filter(key => !sameRect(draft.rects[key], draft.stored[key]));
}

function go(draft: RegionEditorDraft, target: EditableRegion | 'picker'): RegionEditorDraft {
	return target === 'picker' ? { ...draft, view: 'picker', rects: draft.stored, invalid: {}, pending: null, error: '' }
		: pick(draft, target);
}

function leave(draft: RegionEditorDraft, target: EditableRegion | 'picker'): RegionEditorDraft {
	return changedRegions(draft).length || Object.keys(draft.invalid).length
		? { ...draft, pending: target } : go(draft, target);
}

function updateRect(draft: RegionEditorDraft, rect: RegionRect, carry: boolean): RegionEditorDraft {
	if (!draft.selected) return draft;
	const previous = draft.rects[draft.selected];
	const rects = { ...draft.rects, [draft.selected]: rect };
	const strip = rects.labGemSlots;
	if (carry && draft.selected === 'labFont' && previous && strip) {
		rects.labGemSlots = [strip[0] + rect[0] - previous[0], strip[1] + rect[1] - previous[1], strip[2], strip[3]];
	}
	return { ...draft, rects, invalid: {}, error: '' };
}

function css(draft: RegionEditorDraft, r: RegionRect): WidgetRect {
	const sf = draft.monitor.scaleFactor;
	return { x: r[0] / sf, y: r[1] / sf, w: r[2] / sf, h: r[3] / sf };
}

function gesture(draft: RegionEditorDraft, from: RegionRect, edge: ResizeEdge | null, dx: number, dy: number): RegionEditorDraft {
	const sf = draft.monitor.scaleFactor;
	const host = { width: draft.monitor.width / sf, height: draft.monitor.height / sf };
	const start = css(draft, from);
	let result = edge ? resized(start, edge, dx, dy, host) : dragged(start, dx, dy, host);
	if (edge && shapeLocked(draft.selected)) {
		const horizontal = edge.includes('East') || edge.includes('West');
		const anchorX = edge.includes('West') ? start.x + start.w : start.x;
		const anchorY = edge.includes('North') ? start.y + start.h : start.y;
		const maxW = edge.includes('West') ? anchorX : host.width - anchorX;
		const maxH = edge.includes('North') ? anchorY : host.height - anchorY;
		const factor = Math.min(maxW / start.w, maxH / start.h, Math.max(
			MIN_WIDGET_SIDE_CSS / start.w, MIN_WIDGET_SIDE_CSS / start.h,
			horizontal ? result.w / start.w : result.h / start.h));
		const w = start.w * factor, h = start.h * factor;
		result = { x: edge.includes('West') ? anchorX - w : anchorX,
			y: edge.includes('North') ? anchorY - h : anchorY, w, h };
	}
	const physical = physicalHotRect({ left: result.x, top: result.y, right: result.x + result.w, bottom: result.y + result.h }, sf);
	return physical ? updateRect(draft, [physical.x, physical.y, physical.w, physical.h], edge === null) : draft;
}

function typed(draft: RegionEditorDraft, field: RectField, value: string): RegionEditorDraft {
	const frame = draft.selected && draft.rects[draft.selected];
	if (!frame) return draft;
	const rect: RegionRect = [...frame];
	const index = RECT_FIELDS.indexOf(field);
	const number = Number(value);
	const min = index < 2 ? 0 : Math.ceil(MIN_WIDGET_SIDE_CSS * draft.monitor.scaleFactor);
	const max = [draft.monitor.width - frame[2], draft.monitor.height - frame[3],
		draft.monitor.width - frame[0], draft.monitor.height - frame[1]][index];
	let error = '';
	if (!value.trim() || !Number.isSafeInteger(number)) error = `${field}: integer required`;
	else if (number < min) error = `${field}: min ${min}`;
	else if (number > max) error = `${field}: max ${max}`;
	if (error) return { ...draft, error: '', invalid: { ...draft.invalid, [field]: error } };
	rect[index] = number;
	if (shapeLocked(draft.selected) && index >= 2) {
		const other = index === 2 ? 3 : 2;
		rect[other] = Math.round(frame[other] * number / frame[index]);
		const limit = other === 2 ? draft.monitor.width - rect[0] : draft.monitor.height - rect[1];
		if (rect[other] < min || rect[other] > limit) {
			const bound = rect[other] < min ? Math.ceil(min * frame[index] / frame[other]) : Math.floor(limit * frame[index] / frame[other]);
			return { ...draft, invalid: { ...draft.invalid, [field]: `${field}: ${rect[other] < min ? 'min' : 'max'} ${bound}` } };
		}
	}
	const invalid = { ...draft.invalid };
	delete invalid[field];
	return { ...updateRect(draft, rect, index < 2), invalid };
}

export function reduceRegionEditor(
	draft: RegionEditorDraft,
	action: RegionEditorAction
): RegionEditorDraft {
	if (action.type === 'saveFailed') return { ...draft, error: `Save failed: ${action.error}` };
	if (action.type === 'saved') {
		const names = changedRegions(draft).map(key => REGION_NAMES[key]).join(', ');
		const stored = storedRects(action.snapshot);
		return go({ ...draft, snapshot: action.snapshot, stored, rects: stored,
			notice: `Saved: ${names} · just now`, pending: null }, draft.pending ?? 'picker');
	}
	if (action.type === 'focus') return draft.view === 'picker' ? { ...draft, selected: action.region } : draft;
	if (action.type === 'leaveChoice') {
		if (action.choice === 'keep') return { ...draft, pending: null };
		return go(draft, draft.pending ?? 'picker');
	}
	if (draft.pending) return draft;
	if (action.type === 'pick') return draft.view === 'edit' ? leave(draft, action.region) : pick(draft, action.region);
	if (action.type === 'key' && draft.view === 'picker') {
		const index = Math.max(0, EDITABLE_REGIONS.indexOf(draft.selected ?? 'labGem'));
		if (action.key === 'Enter') return pick(draft, EDITABLE_REGIONS[index]);
		if (action.key === 'ArrowDown' || action.key === 'ArrowUp') return { ...draft,
			selected: EDITABLE_REGIONS[(index + (action.key === 'ArrowDown' ? 1 : 4)) % 5] };
		return draft;
	}
	if (action.type === 'back' || (action.type === 'key' && action.key === 'Escape')) return leave(draft, 'picker');
	if (action.type === 'key' && action.key === 'Tab') {
		const index = EDITABLE_REGIONS.indexOf(draft.selected ?? 'labGem');
		return leave(draft, EDITABLE_REGIONS[(index + (action.shift ? 4 : 1)) % 5]);
	}
	if (draft.view !== 'edit' || !draft.selected) return draft;
	if (action.type === 'resetToAuto') {
		const auto = autoRegion(draft, draft.selected);
		return auto ? updateRect(draft, [...auto], false) : draft;
	}
	if (action.type === 'typeField') return typed(draft, action.field, action.value);
	if (action.type === 'drag') return gesture(draft, action.from, null, action.dx, action.dy);
	if (action.type === 'resize') return gesture(draft, action.from, action.edge, action.dx, action.dy);
	if (action.type === 'key') {
		const from = draft.rects[draft.selected];
		if (!from || !action.key.startsWith('Arrow')) return draft;
		const step = (action.shift ? 10 : 1) / draft.monitor.scaleFactor;
		const dx = action.key === 'ArrowRight' ? step : action.key === 'ArrowLeft' ? -step : 0;
		const dy = action.key === 'ArrowDown' ? step : action.key === 'ArrowUp' ? -step : 0;
		return gesture(draft, from, action.alt ? (dx ? 'East' : 'South') : null, dx, dy);
	}
	return draft;
}

export function editorView(draft: RegionEditorDraft): RegionEditorView {
	const frame = draft.view === 'edit' && draft.selected ? draft.rects[draft.selected] : null;
	const before = draft.selected ? draft.stored[draft.selected] : null;
	const changed = changedRegions(draft);
	const invalidField = (Object.keys(draft.invalid)[0] as RectField | undefined) ?? null;
	const auto = draft.selected ? autoRegion(draft, draft.selected) : null;
	const tag = frame && !before ? 'new' : frame && before && !sameRect(frame, before)
		? frame[2] !== before[2] || frame[3] !== before[3] ? 'resized' : 'moved' : null;
	return {
		view: draft.view,
		selected: draft.selected,
		frame,
		frameCss: frame ? css(draft, frame) : null,
		carried: frame && draft.selected === 'labFont' ? draft.rects.labGemSlots : null,
		autoGhost: frame ? auto : null,
		tag,
		invalidField,
		hint: invalidField ? draft.invalid[invalidField]! : draft.error || (draft.view === 'edit' && changed.length
			? `Unsaved: ${changed.map(key => REGION_NAMES[key]).join(', ')}` : draft.view === 'picker' ? draft.notice : ''),
		saveEnabled: !!frame && changed.length > 0 && !invalidField,
		resetEnabled: !!auto,
		resetTitle: auto ? 'Reset to Auto' : `No Auto rect for ${draft.selected ? REGION_NAMES[draft.selected] : 'region'}`,
		leavePrompt: draft.pending !== null
	};
}

export function savePayload(draft: RegionEditorDraft): ManualRegionsWire {
	return {
		set_on: draft.snapshot.manual?.setOn ?? { width: draft.monitor.width, height: draft.monitor.height, origin: [draft.monitor.x, draft.monitor.y], at_ms: 0 },
		edited: true,
		lab_gem: draft.rects.labGem,
		lab_font: draft.rects.labFont,
		lab_gem_slots: draft.rects.labGemSlots,
		temple_panel: draft.rects.templePanel,
		merc_panel: draft.rects.mercPanel
	};
}

export function editorPanelPosition(draft: RegionEditorDraft, bar: { width: number; height: number }): { x: number; y: number } {
	const host = { width: draft.monitor.width / draft.monitor.scaleFactor, height: draft.monitor.height / draft.monitor.scaleFactor };
	const frame = editorView(draft).frameCss;
	const gap = CONFIG_BAR_GAP;
	let x = gap, y = host.height / 5;
	if (frame) {
		const right = frame.x + frame.w + gap;
		const left = frame.x - gap - bar.width;
		const ghostRect = editorView(draft).autoGhost;
		const ghost = ghostRect ? css(draft, ghostRect) : null;
		const clear = (at: number) => !ghost || at + bar.width <= ghost.x || at >= ghost.x + ghost.w || frame.y + bar.height <= ghost.y || frame.y >= ghost.y + ghost.h;
		if (right + bar.width <= host.width && clear(right)) x = right;
		else if (left >= 0 && clear(left)) x = left;
		else if (right + bar.width <= host.width || left >= 0) x = host.width - right >= frame.x ? right : left;
		else { x = gap; y = frame.y + frame.h + gap; }
		if (x !== gap) y = frame.y;
	}
	const placed = clampToHost({ x, y, w: bar.width + gap, h: bar.height + gap }, host);
	return { x: Math.max(gap, placed.x), y: Math.max(gap, placed.y) };
}

type EditorWindow = Pick<WebviewWindow, 'label' | 'once' | 'setPosition' | 'setSize' | 'outerSize' | 'close' | 'destroy'>;
export type RegionEditorWindowAdapters = {
	primaryMonitor: typeof primaryMonitor;
	currentMonitor: typeof currentMonitor;
	availableMonitors: typeof availableMonitors;
	getGameMonitor: () => Promise<GameMonitorInfo | null>;
	createWindow: (label: string, options: ConstructorParameters<typeof WebviewWindow>[1]) => EditorWindow;
	getWindowByLabel: (label: string) => Promise<EditorWindow | null>;
	wait: (ms: number) => Promise<void>;
	report: (message: string) => void;
};

export function createRegionEditorOwner(overrides: Partial<RegionEditorWindowAdapters> = {}) {
	const adapters: RegionEditorWindowAdapters = {
		primaryMonitor, currentMonitor, availableMonitors,
		getGameMonitor: () => invoke('get_game_monitor'),
		createWindow: (label, options) => new WebviewWindow(label, options),
		getWindowByLabel: label => WebviewWindow.getByLabel(label),
		wait: ms => new Promise(resolve => setTimeout(resolve, ms)),
		report: message => {
			console.error(message);
			void invoke('app_log_from_frontend', { msg: message }).catch(console.error);
		}, ...overrides
	};
	let owned: EditorWindow | null = null;
	let generation = 0;
	async function removeExisting(): Promise<void> {
		for (let i = 0; i < 5; i++) {
			const existing = await adapters.getWindowByLabel('overlay-region-editor');
			if (!existing) return;
			await existing.close().catch(() => {});
			await existing.destroy().catch(() => {});
			if (i < 4) await adapters.wait(100);
		}
		if (await adapters.getWindowByLabel('overlay-region-editor')) throw new Error('Region editor could not close');
	}
	async function destroy() {
		generation++;
		owned = null;
		await removeExisting();
	}
	async function open(select: EditableRegion | null = null): Promise<void> {
		const ticket = ++generation;
		try {
			const primary = await adapters.primaryMonitor().catch(() => null)
				?? await adapters.currentMonitor().catch(() => null);
			const game = await adapters.getGameMonitor().catch(() => null);
			const listed = await adapters.availableMonitors().catch(() => []);
			const monitor = chooseMonitor(game, listed, primary);
			if (!monitor) throw new Error('Region editor has no monitor');
			if (game && (monitor.position.x !== game.x || monitor.position.y !== game.y)) {
				adapters.report('Region editor game monitor unavailable; using primary monitor');
			}
			if (ticket !== generation) return;
			owned = null;
			await removeExisting();
			if (ticket !== generation) return;
			const sf = monitor.scaleFactor;
			if (!(sf > 0)) throw new Error('Region editor monitor scale unknown');
			const { width, height } = monitor.size;
			const win = adapters.createWindow('overlay-region-editor', {
				url: `/overlay/region-editor${select ? `?region=${select}` : ''}`,
				transparent: true, decorations: false, alwaysOnTop: true,
				resizable: false, shadow: false, skipTaskbar: true, focus: true,
				width: width / sf, height: height / sf
			});
			owned = win;
			void win.once('tauri://created', async () => {
				if (owned !== win || ticket !== generation) return;
				try {
					const position = new PhysicalPosition(monitor.position.x, monitor.position.y);
					const size = new PhysicalSize(width, height);
					await win.setPosition(position);
					await win.setSize(size);
					const outer = await win.outerSize();
					await win.setSize(new PhysicalSize(outer.width + 1, outer.height + 1));
					await win.setSize(new PhysicalSize(outer.width, outer.height));
					if (owned !== win || ticket !== generation) return;
					await win.setPosition(position);
					await win.setSize(size);
					if (owned !== win || ticket !== generation) return;
				} catch (error) {
					adapters.report(`Region editor placement failed: ${error}`);
					if (owned === win && ticket === generation) await destroy();
				}
			});
			void win.once('tauri://error', event => {
				adapters.report(`Region editor creation failed: ${String(event.payload)}`);
				if (owned === win && ticket === generation) void destroy().catch(error => adapters.report(String(error)));
			});
		} catch (error) {
			adapters.report(`Region editor failed: ${error}`);
			throw error;
		}
	}
	return { open, destroy };
}
