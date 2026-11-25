// @vitest-environment happy-dom
/**
 * The region editor route mounted in the client runtime at the URL its window
 * owner opens, with Tauri's transport and the native window mocked: the drawn
 * frames, the saved set and the window placement are what the player gets.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { currentMonitor } from '@tauri-apps/api/window';
import type { ManualOcrRegions, Placements } from '$lib/stores/ssot.svelte';
import {
	createRegionEditorOwner,
	type EditableRegion,
	type RegionEditorWindowAdapters
} from '$lib/overlay/ocr-region-editor';
import EditorPage from './+page.svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/dpi', () => ({
	PhysicalPosition: class {
		constructor(public x: number, public y: number) {}
	},
	PhysicalSize: class {
		constructor(public width: number, public height: number) {}
	}
}));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
	WebviewWindow: class {
		static getByLabel = vi.fn();
	},
	getCurrentWebviewWindow: vi.fn()
}));
vi.mock('@tauri-apps/api/window', () => ({
	availableMonitors: vi.fn(),
	currentMonitor: vi.fn(),
	primaryMonitor: vi.fn()
}));

type Rect = [number, number, number, number];

interface Scene {
	game: { x: number; y: number; width: number; height: number };
	scaleFactor: number;
	region: EditableRegion;
	manual: ManualOcrRegions;
	auto: Placements | null;
	uiScale: number | null;
	/** One entry per `set_manual_ocr_regions` call: an error rejects it, null resolves. */
	saveErrors: (string | null)[];
}

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

function autoPlacements(font: Rect): Placements {
	return {
		temple: { entranceOrigin: [1100, 900] },
		merc: { panel: [1406, 738, 666, 572] },
		exchange: null,
		lab: {
			gem: [0, 0, 3440, 53],
			font,
			gemSlots: [
				[1100, 734, 64, 64],
				[1232, 734, 64, 64],
				[1364, 734, 64, 64]
			]
		}
	};
}

function scene(overrides: Partial<Scene> = {}): Scene {
	return {
		game: { x: 0, y: 0, width: 3440, height: 1440 },
		scaleFactor: 1.25,
		region: 'labFont',
		manual: manualSet(),
		auto: autoPlacements([397, 293, 888, 908]),
		uiScale: 1.2,
		saveErrors: [],
		...overrides
	};
}

type NativeWindow = {
	url: string;
	positions: { x: number; y: number }[];
	sizes: { width: number; height: number }[];
};

/** Opens the editor through its owner, as Settings' Edit does, and returns the native window it built. */
async function openWindow(s: Scene): Promise<NativeWindow> {
	const monitor = {
		position: { x: s.game.x, y: s.game.y },
		size: { width: s.game.width, height: s.game.height },
		scaleFactor: s.scaleFactor
	};
	const primary = { position: { x: 0, y: 0 }, size: { width: 2560, height: 1440 }, scaleFactor: 1 };
	const handlers = new Map<string, () => Promise<void>>();
	const native: NativeWindow = { url: '', positions: [], sizes: [] };
	let outer = { width: 0, height: 0 };
	const adapters: Partial<RegionEditorWindowAdapters> = {
		primaryMonitor: async () => primary as never,
		currentMonitor: async () => null,
		availableMonitors: async () => [primary, monitor] as never,
		getGameMonitor: async () => ({ id: 2, ...s.game }),
		getWindowByLabel: async () => null,
		wait: async () => {},
		report: () => {},
		createWindow: (label, options) => {
			native.url = options!.url!;
			return {
				label,
				once: async (event: string, handler: () => Promise<void>) => {
					handlers.set(event, handler);
					return () => {};
				},
				setPosition: async (position: { x: number; y: number }) => {
					native.positions.push({ x: position.x, y: position.y });
				},
				setSize: async (size: { width: number; height: number }) => {
					native.sizes.push({ width: size.width, height: size.height });
					outer = { width: size.width, height: size.height };
				},
				outerSize: async () => outer,
				close: async () => {},
				destroy: async () => {}
			} as never;
		}
	};
	await createRegionEditorOwner(adapters).open(s.region);
	const created = handlers.get('tauri://created');
	expect(created).toBeDefined();
	await created!();
	return native;
}

const saveCalls: unknown[] = [];

function mockTauri(s: Scene): void {
	const errors = [...s.saveErrors];
	vi.mocked(invoke).mockImplementation((async (command: string, args?: unknown) => {
		if (command === 'get_ssot') {
			return {
				ocrRegions: { mode: 'manual', manual: s.manual, auto: s.auto, screenWarning: null },
				screen: s.uiScale === null ? null : { uiScale: s.uiScale }
			};
		}
		if (command === 'get_game_monitor') return { id: 2, ...s.game };
		if (command === 'set_manual_ocr_regions') {
			saveCalls.push(args);
			const error = errors.shift();
			if (error) throw error;
		}
		return undefined;
	}) as never);
	vi.mocked(currentMonitor).mockResolvedValue({
		position: { x: s.game.x, y: s.game.y },
		size: { width: s.game.width, height: s.game.height },
		scaleFactor: s.scaleFactor
	} as never);
	vi.mocked(getCurrentWebviewWindow).mockReturnValue({
		scaleFactor: async () => s.scaleFactor,
		close: async () => {}
	} as never);
}

let mounted: ReturnType<typeof mount> | null = null;
afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	document.body.innerHTML = '';
	window.history.replaceState(null, '', '/');
	saveCalls.length = 0;
	vi.resetAllMocks();
});

async function render(s: Scene): Promise<{ target: HTMLElement; native: NativeWindow }> {
	const native = await openWindow(s);
	mockTauri(s);
	window.history.replaceState(null, '', native.url);
	const target = document.createElement('div');
	document.body.appendChild(target);
	mounted = mount(EditorPage, { target });
	await vi.waitFor(() => expect(target.querySelector('.edit-panel:not(.unavailable)')).not.toBeNull());
	flushSync();
	return { target, native };
}

function button(target: HTMLElement, label: string): HTMLButtonElement {
	const found = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
		(b) => b.textContent?.trim() === label || b.getAttribute('aria-label') === label
	);
	if (!found) throw new Error(`no button "${label}"`);
	return found;
}

function pointer(on: EventTarget, type: string, clientX: number, clientY: number): void {
	on.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX, clientY, pointerId: 1, button: 0 }));
	flushSync();
}

/** Press inside the picked frame (or on a handle) and release `dx` CSS px to the right. */
function drag(target: HTMLElement, dx: number, handle?: string): void {
	const from = handle ? button(target, handle) : target.querySelector<HTMLElement>('.region.selected')!;
	pointer(from, 'pointerdown', 300, 300);
	pointer(window, 'pointermove', 300 + dx, 300);
	pointer(window, 'pointerup', 300 + dx, 300);
}

function typeInto(target: HTMLElement, field: string, value: string): void {
	const input = target.querySelector<HTMLInputElement>(`input[aria-label="${field}"]`)!;
	input.value = value;
	input.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
}

function key(name: string): void {
	document.body.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true }));
	flushSync();
}

function click(target: HTMLElement, label: string): void {
	button(target, label).click();
	flushSync();
}

async function save(target: HTMLElement, calls: number): Promise<void> {
	click(target, 'Save');
	await vi.waitFor(() => expect(saveCalls).toHaveLength(calls));
	await vi.waitFor(() => expect(target.querySelector('button.save')?.textContent?.trim()).not.toBe('Saving…'));
	flushSync();
}

/** What the player sees: the panel, its numbers and controls, and the frames drawn over the game. */
function visible(target: HTMLElement) {
	const panel = target.querySelector<HTMLElement>('.edit-panel')!;
	const reset = panel.querySelector<HTMLButtonElement>('button.reset');
	const saveButton = panel.querySelector<HTMLButtonElement>('button.save');
	return {
		panel: panel.getAttribute('aria-label'),
		title: panel.querySelector('.ep-title')?.childNodes[0]?.textContent?.trim(),
		readout: target.querySelector('.readout')?.textContent ?? null,
		hint: panel.querySelector('.config-hint')?.textContent,
		invalid: [...panel.querySelectorAll('input[aria-invalid="true"]')].map((i) => i.getAttribute('aria-label')),
		saveEnabled: saveButton ? !saveButton.disabled : null,
		reset: reset ? { enabled: !reset.disabled, title: reset.title } : null,
		handles: target.querySelectorAll('.region.selected .handle').length,
		frames: [...target.querySelectorAll('.region')].map((r) => r.querySelector('.rname')?.textContent),
		actions: [...panel.querySelectorAll('.ep-actions button')].map((b) => b.textContent?.trim())
	};
}

const FONT_SAVED = {
	set_on: { width: 3440, height: 1440, origin: [0, 0], at_ms: 1_732_400_000_000 },
	edited: true,
	lab_gem: [0, 0, 3440, 64],
	lab_font: [962, 293, 888, 908],
	lab_gem_slots: [1225, 734, 328, 64],
	temple_panel: [1000, 200, 800, 600],
	merc_panel: [1406, 738, 666, 572]
};

describe('region editor route opened on the Font panel', () => {
	// keep:happy
	it('opens its window over the game monitor in physical px', async () => {
		const { native } = await render(scene({ game: { x: -3440, y: 0, width: 3440, height: 1440 } }));

		expect(native.url).toBe('/overlay/region-editor?region=labFont');
		expect(new Set(native.positions.map((p) => `${p.x},${p.y}`))).toEqual(new Set(['-3440,0']));
		expect(native.sizes.at(-1)).toEqual({ width: 3440, height: 1440 });
	});

	// keep:happy
	it('draws only the Font panel, its Auto reference and the carried strip', async () => {
		const { target } = await render(scene());

		expect(visible(target)).toMatchObject({
			title: 'Font panel',
			readout: '[837, 293, 888, 908]',
			handles: 8,
			frames: ['Font panel, Auto [397, 293, 888, 908]', 'Font panel', 'Gem choice slots']
		});
		expect(target.querySelectorAll('.slot')).toHaveLength(3);
	});

	// keep:happy
	it('saves a +100 CSS px drag at scale 1.25 as the whole set with Font and strip moved 125 px', async () => {
		const { target } = await render(scene());

		drag(target, 100);
		await save(target, 1);

		expect(saveCalls).toEqual([{ regions: FONT_SAVED }]);
	});

	// keep:happy
	it('keeps the draft and shows the error after a rejected save, so Save again sends the same set', async () => {
		const { target } = await render(
			scene({ saveErrors: ['settings.json — Access is denied (os error 5)', null] })
		);
		drag(target, 100);

		await save(target, 1);

		expect(visible(target)).toMatchObject({
			panel: 'Edit region',
			readout: '[962, 293, 888, 908]',
			hint: 'Save failed: settings.json — Access is denied (os error 5)',
			saveEnabled: true
		});
		await save(target, 2);
		expect(saveCalls).toEqual([{ regions: FONT_SAVED }, { regions: FONT_SAVED }]);
	});

	// keep:critical — a failure saves every region on a non-primary monitor offset
	// by the monitor origin: a silently wrong crop that is then clipped away.
	it('saves a Font panel dragged on a monitor at (-2560, 0) capture-relative, without the origin', async () => {
		const { target } = await render(
			scene({
				game: { x: -2560, y: 0, width: 2560, height: 1440 },
				scaleFactor: 1,
				manual: manualSet({
					setOn: { width: 2560, height: 1440, origin: [-2560, 0], at_ms: 1 },
					labFont: [737, 293, 888, 908]
				})
			})
		);

		drag(target, 100);
		await save(target, 1);

		expect(saveCalls).toHaveLength(1);
		expect((saveCalls[0] as { regions: { lab_font: Rect } }).regions.lab_font).toEqual([837, 293, 888, 908]);
	});

	type Row = {
		name: string;
		scene?: Partial<Scene>;
		act: (target: HTMLElement) => void;
		expected: Partial<ReturnType<typeof visible>>;
	};

	const rows: Row[] = [
		{
			name: 'typing x = 3000 on a 3440 monitor marks x invalid, disables Save and keeps the last valid frame',
			act: (t) => typeInto(t, 'x', '3000'),
			expected: { invalid: ['x'], hint: 'x: max 2552', saveEnabled: false, readout: '[837, 293, 888, 908]' }
		},
		{
			name: 'a valid typed value after an invalid one re-enables Save',
			act: (t) => {
				typeInto(t, 'x', '3000');
				typeInto(t, 'x', '2552');
			},
			expected: { invalid: [], saveEnabled: true, readout: '[2552, 293, 888, 908]' }
		},
		{
			name: 'an East-handle resize below the minimum stops at 24 CSS px (30 physical)',
			act: (t) => drag(t, -1000, 'Resize East'),
			expected: { readout: '[837, 293, 30, 908]' }
		},
		{
			name: 'with no Auto slice no ghost is drawn and Reset to Auto is disabled',
			scene: { auto: null, uiScale: null },
			act: () => {},
			expected: {
				frames: ['Font panel', 'Gem choice slots'],
				reset: { enabled: false, title: 'No Auto rect for Font panel' }
			}
		},
		{
			name: 'a Merc panel corner resize keeps its aspect ratio on 4 corner handles',
			scene: { region: 'mercPanel', scaleFactor: 1 },
			act: (t) => drag(t, -333, 'Resize SouthEast'),
			expected: { title: 'Merc panel', handles: 4, readout: '[1406, 738, 333, 286]' }
		},
		{
			name: 'Esc, ArrowDown and Enter walk the picker to the Gem choice slots edit',
			act: () => {
				key('Escape');
				key('ArrowDown');
				key('Enter');
			},
			expected: { panel: 'Edit region', title: 'Gem choice slots', readout: '[1100, 734, 328, 64]' }
		},
		{
			name: 'Back with a draft offers Keep editing | Discard | Save',
			act: (t) => {
				drag(t, 100);
				click(t, 'Back');
			},
			expected: {
				panel: 'Edit region',
				hint: 'Unsaved: Font panel, Gem choice slots',
				actions: ['Keep editing', 'Discard', 'Save']
			}
		},
		{
			name: 'Keep editing returns to the draft',
			act: (t) => {
				drag(t, 100);
				click(t, 'Back');
				click(t, 'Keep editing');
			},
			expected: { readout: '[962, 293, 888, 908]', actions: ['Reset to Auto', 'Back', 'Save'] }
		},
		{
			name: 'Discard drops the draft and returns to the picker',
			act: (t) => {
				drag(t, 100);
				click(t, 'Back');
				click(t, 'Discard');
			},
			expected: { panel: 'Region picker', readout: null, actions: ['Close'] }
		}
	];

	// keep:happy
	it.each(rows)('validation and transitions: $name', async ({ scene: overrides = {}, act, expected }) => {
		const { target } = await render(scene(overrides));

		act(target);

		expect(visible(target)).toMatchObject(expected);
		expect(saveCalls).toEqual([]);
	});
});
