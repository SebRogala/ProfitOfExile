import { describe, expect, it } from 'vitest';
import type { ManualOcrRegions, OcrRegionsSnapshot, Placements } from '$lib/stores/ssot.svelte';
import {
	createRegionEditorDraft,
	editorView,
	reduceRegionEditor,
	savePayload,
	type EditorMonitor,
	type RegionEditorAction,
	type RegionEditorInput,
	type RegionEditorView
} from './ocr-region-editor';

const MONITOR_3440: EditorMonitor = { x: 0, y: 0, width: 3440, height: 1440, scaleFactor: 1.25 };

function manualSet(overrides: Partial<ManualOcrRegions> = {}): ManualOcrRegions {
	return {
		setOn: { width: 3440, height: 1440, origin: [0, 0], at_ms: 1_732_400_000_000 },
		edited: true,
		labGem: [0, 0, 3440, 64],
		labFont: [837, 293, 888, 908],
		labGemSlots: [1100, 734, 328, 64],
		templePanel: [1000, 200, 800, 600],
		mercPanel: [1406, 738, 666, 572],
		...overrides
	};
}

function autoPlacements(fontRect: [number, number, number, number]): Placements {
	return {
		temple: { entranceOrigin: [1100, 900] },
		merc: { panel: [1406, 738, 666, 572] },
		exchange: null,
		lab: {
			gem: [0, 0, 3440, 53],
			font: fontRect,
			gemSlots: [
				[1100, 734, 64, 64],
				[1232, 734, 64, 64],
				[1364, 734, 64, 64]
			]
		}
	};
}

function snapshot(
	manual: ManualOcrRegions = manualSet(),
	auto: Placements | null = autoPlacements([837, 293, 888, 908])
): OcrRegionsSnapshot {
	return { mode: 'manual', manual, auto, screenWarning: null };
}

function editor(overrides: Partial<RegionEditorInput> = {}) {
	return createRegionEditorDraft({
		snapshot: snapshot(),
		monitor: MONITOR_3440,
		uiScale: 1.2,
		select: 'labFont',
		...overrides
	});
}

function run(input: Partial<RegionEditorInput>, actions: readonly RegionEditorAction[]) {
	return actions.reduce(reduceRegionEditor, editor(input));
}

const ESC: RegionEditorAction = { type: 'key', key: 'Escape', shift: false, alt: false };
const TAB: RegionEditorAction = { type: 'key', key: 'Tab', shift: false, alt: false };
const FONT_DRAG: RegionEditorAction = { type: 'drag', from: [837, 293, 888, 908], dx: 100, dy: 0 };

describe('region editor draft', () => {
	// keep:happy
	it('saves a Font panel dragged +100 CSS px at scale 1.25 as +125 physical px, carrying its strip', () => {
		const draft = run({}, [FONT_DRAG]);

		expect(savePayload(draft)).toMatchObject({
			lab_font: [962, 293, 888, 908],
			lab_gem_slots: [1225, 734, 328, 64],
			lab_gem: [0, 0, 3440, 64],
			temple_panel: [1000, 200, 800, 600],
			merc_panel: [1406, 738, 666, 572]
		});
	});

	// keep:happy
	it('keeps the draft after a failed save so a second Save sends the same set', () => {
		const failed = run({}, [FONT_DRAG, { type: 'saveFailed', error: 'disk full' }]);

		expect(editorView(failed)).toMatchObject({
			view: 'edit',
			selected: 'labFont',
			hint: 'Save failed: disk full',
			saveEnabled: true
		});
		expect(savePayload(failed)).toMatchObject({
			lab_font: [962, 293, 888, 908],
			lab_gem_slots: [1225, 734, 328, 64]
		});
	});

	// keep:happy
	it('resets only the selected Font panel draft to its Auto rect', () => {
		const draft = run(
			{
				snapshot: snapshot(
					manualSet({ labFont: [1820, 293, 888, 908] }),
					autoPlacements([837, 293, 888, 908])
				)
			},
			[{ type: 'resetToAuto' }]
		);

		expect(savePayload(draft)).toMatchObject({
			lab_font: [837, 293, 888, 908],
			lab_gem_slots: [1100, 734, 328, 64],
			lab_gem: [0, 0, 3440, 64],
			temple_panel: [1000, 200, 800, 600],
			merc_panel: [1406, 738, 666, 572]
		});
	});

	// keep:critical — a failure saves every region on a non-primary monitor
	// offset by the monitor origin: a silently wrong crop that is then clipped away.
	it('saves a region drawn on a monitor at (-2560, 0) capture-relative, without the origin', () => {
		const draft = run(
			{
				snapshot: snapshot(
					manualSet({
						setOn: { width: 2560, height: 1440, origin: [-2560, 0], at_ms: 1 },
						labFont: [737, 293, 888, 908]
					})
				),
				monitor: { x: -2560, y: 0, width: 2560, height: 1440, scaleFactor: 1 }
			},
			[{ type: 'drag', from: [737, 293, 888, 908], dx: 100, dy: 0 }]
		);

		expect(editorView(draft).frameCss).toEqual({ x: 837, y: 293, w: 888, h: 908 });
		expect(savePayload(draft).lab_font).toEqual([837, 293, 888, 908]);
	});

	type Row = {
		name: string;
		input?: Partial<RegionEditorInput>;
		actions: RegionEditorAction[];
		expected: Partial<RegionEditorView>;
	};

	const rows: Row[] = [
		{
			name: 'a typed x past the monitor marks x invalid and keeps the last valid rect',
			actions: [
				{ type: 'typeField', field: 'x', value: '900' },
				{ type: 'typeField', field: 'x', value: '3000' }
			],
			expected: { frame: [900, 293, 888, 908], invalidField: 'x', hint: 'x: max 2552', saveEnabled: false }
		},
		{
			name: 'a valid typed value after an invalid one re-enables Save',
			actions: [
				{ type: 'typeField', field: 'x', value: '3000' },
				{ type: 'typeField', field: 'x', value: '2552' }
			],
			expected: { frame: [2552, 293, 888, 908], invalidField: null, saveEnabled: true }
		},
		{
			name: 'a resize below the minimum side stops at 24 CSS px (30 physical at 1.25)',
			actions: [{ type: 'resize', from: [837, 293, 888, 908], edge: 'East', dx: -1000, dy: 0 }],
			expected: { frame: [837, 293, 30, 908], carried: [1100, 734, 328, 64], tag: 'resized' }
		},
		{
			name: 'a Font panel drag carries the strip by the same physical delta',
			actions: [FONT_DRAG],
			expected: { frame: [962, 293, 888, 908], carried: [1225, 734, 328, 64], tag: 'moved', saveEnabled: true }
		},
		{
			name: 'a Temple panel corner resize keeps its aspect ratio',
			input: { monitor: { ...MONITOR_3440, scaleFactor: 1 }, select: 'templePanel' },
			actions: [{ type: 'resize', from: [1000, 200, 800, 600], edge: 'SouthEast', dx: 80, dy: 0 }],
			expected: { frame: [1000, 200, 880, 660] }
		},
		{
			name: 'a Merc panel corner resize keeps its aspect ratio',
			input: { monitor: { ...MONITOR_3440, scaleFactor: 1 }, select: 'mercPanel' },
			actions: [{ type: 'resize', from: [1406, 738, 666, 572], edge: 'SouthEast', dx: -333, dy: 0 }],
			expected: { frame: [1406, 738, 333, 286] }
		},
		{
			name: 'Shift+Arrow nudges 10 physical px',
			actions: [{ type: 'key', key: 'ArrowRight', shift: true, alt: false }],
			expected: { frame: [847, 293, 888, 908], carried: [1110, 734, 328, 64] }
		},
		{
			name: 'Alt+Arrow resizes from the bottom edge by 1 physical px',
			actions: [{ type: 'key', key: 'ArrowDown', shift: false, alt: true }],
			expected: { frame: [837, 293, 888, 909] }
		},
		{
			name: 'picking a null region places it centred at reference size x slice scale',
			input: {
				snapshot: snapshot(manualSet({ mercPanel: null }), {
					...autoPlacements([837, 293, 888, 908]),
					merc: null
				}),
				monitor: { ...MONITOR_3440, scaleFactor: 1 },
				select: null
			},
			actions: [{ type: 'pick', region: 'mercPanel' }],
			expected: {
				view: 'edit',
				selected: 'mercPanel',
				frame: [1387, 434, 666, 572],
				tag: 'new',
				autoGhost: null,
				resetEnabled: false,
				resetTitle: 'No Auto rect for Merc panel',
				saveEnabled: true
			}
		},
		{
			name: 'with no Auto slice the ghost is hidden and Reset to Auto is disabled',
			input: { snapshot: snapshot(manualSet(), null), uiScale: null },
			actions: [],
			expected: {
				selected: 'labFont',
				autoGhost: null,
				resetEnabled: false,
				resetTitle: 'No Auto rect for Font panel'
			}
		},
		{
			name: 'picking a row from the picker opens that one region',
			input: { select: null },
			actions: [{ type: 'pick', region: 'templePanel' }],
			expected: { view: 'edit', selected: 'templePanel', frame: [1000, 200, 800, 600], carried: null }
		},
		{
			name: 'Esc without a draft returns to the picker at once',
			actions: [ESC],
			expected: { view: 'picker', selected: 'labFont', leavePrompt: false }
		},
		{
			name: 'Tab without a draft edits the next region',
			actions: [TAB],
			expected: { view: 'edit', selected: 'labGemSlots', frame: [1100, 734, 328, 64] }
		},
		{
			name: 'Esc with a draft opens the leave prompt',
			actions: [FONT_DRAG, ESC],
			expected: {
				view: 'edit',
				selected: 'labFont',
				leavePrompt: true,
				hint: 'Unsaved: Font panel, Gem choice slots'
			}
		},
		{
			name: 'Back with a draft opens the leave prompt',
			actions: [FONT_DRAG, { type: 'back' }],
			expected: { view: 'edit', leavePrompt: true }
		},
		{
			name: 'Keep editing closes the prompt and keeps the draft',
			actions: [FONT_DRAG, ESC, { type: 'leaveChoice', choice: 'keep' }],
			expected: { view: 'edit', leavePrompt: false, frame: [962, 293, 888, 908] }
		},
		{
			name: 'Discard drops the draft and goes where the player was heading',
			actions: [FONT_DRAG, TAB, { type: 'leaveChoice', choice: 'discard' }],
			expected: { view: 'edit', selected: 'labGemSlots', leavePrompt: false, frame: [1100, 734, 328, 64] }
		},
		{
			name: 'a discarded Font draft reopens at its stored rect',
			actions: [
				FONT_DRAG,
				ESC,
				{ type: 'leaveChoice', choice: 'discard' },
				{ type: 'pick', region: 'labFont' }
			],
			expected: { view: 'edit', selected: 'labFont', frame: [837, 293, 888, 908], tag: null, saveEnabled: false }
		},
		{
			name: 'Save from the leave prompt goes where the player was heading',
			actions: [
				FONT_DRAG,
				ESC,
				{
					type: 'saved',
					snapshot: snapshot(manualSet({ labFont: [962, 293, 888, 908], labGemSlots: [1225, 734, 328, 64] }))
				}
			],
			expected: { view: 'picker', selected: 'labFont', leavePrompt: false }
		}
	];

	// keep:happy
	it.each(rows)('validation and transitions: $name', ({ input = {}, actions, expected }) => {
		expect(editorView(run(input, actions))).toMatchObject(expected);
	});
});
