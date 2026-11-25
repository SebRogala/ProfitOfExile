// @vitest-environment happy-dom
/**
 * SettingsPage mounted in the client runtime under happy-dom, so the Display
 * all-feeders switch is clicked for real. Rust is the stubbed `invoke`: every
 * call it records is an IPC write the page sent.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
	calls: [] as [string, unknown][],
	/** Per-case Rust answers, consulted before ANSWERS; a throwing handler is a rejected command. */
	handlers: {} as Record<string, (args: unknown) => unknown>
}));
/** What the page asked the editor and preview windows to open. */
const overlays = vi.hoisted(() => ({ editorOpens: [] as (string | null)[], previews: [] as string[] }));

/** What the stubbed Rust answers the page's mount reads with; everything else answers null. */
const ANSWERS: Record<string, unknown> = {
	get_proxy_settings: { saved: '', inEffect: '', savedTrade: true, tradeInEffect: true, updaterProxy: '', blocked: '' },
	get_ocr_rects: [],
	get_widget_geometries: [],
	get_webview_gpu_disabled: false
};

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (cmd: string, args?: unknown) => {
		ipc.calls.push([cmd, args]);
		if (Object.hasOwn(ipc.handlers, cmd)) return ipc.handlers[cmd](args);
		return Object.hasOwn(ANSWERS, cmd) ? ANSWERS[cmd] : null;
	})
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.0.0-test' }));
vi.mock('@tauri-apps/api/window', () => ({
	availableMonitors: async () => [],
	currentMonitor: async () => null,
	primaryMonitor: async () => ({ scaleFactor: 1 })
}));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
	getCurrentWebviewWindow: () => ({ listen: async () => () => {} })
}));
vi.mock('$lib/overlay/ocr-region-editor', async () => {
	const actual = await vi.importActual<typeof import('$lib/overlay/ocr-region-editor')>(
		'$lib/overlay/ocr-region-editor'
	);
	return {
		...actual,
		createRegionEditorOwner: () => ({
			open: async (select: string | null = null) => {
				overlays.editorOpens.push(select);
			},
			destroy: async () => {}
		})
	};
});
vi.mock('$lib/overlay/ocr-preview', async () => {
	const actual = await vi.importActual<typeof import('$lib/overlay/ocr-preview')>('$lib/overlay/ocr-preview');
	return {
		...actual,
		createOcrPreviewOwner: () => ({
			hasPreview: false,
			destroy: async () => {},
			preview: async (request: { key: string }) => {
				overlays.previews.push(request.key);
			}
		})
	};
});
vi.mock('$lib/updater/settings-update.svelte', async () => {
	const actual = await vi.importActual<typeof import('$lib/updater/settings-update.svelte')>(
		'$lib/updater/settings-update.svelte'
	);
	return {
		...actual,
		createSettingsUpdateController: (snapshot: Parameters<typeof actual.createSettingsUpdateController>[0]) =>
			actual.createSettingsUpdateController(snapshot, {
				checkForUpdate: async () => null,
				relaunch: async () => {}
			})
	};
});

let { flushSync, mount, unmount } = await import('svelte');
let SettingsPage: typeof import('./SettingsPage.svelte').default;

// ssot.svelte.ts holds the choice at module level; fresh modules per case keep
// one case's click from showing up as the next case's stored choice.
beforeEach(async () => {
	vi.resetModules();
	({ flushSync, mount, unmount } = await import('svelte'));
	SettingsPage = (await import('./SettingsPage.svelte')).default;
});

let app: ReturnType<typeof mount> | null = null;

afterEach(() => {
	if (app) unmount(app);
	app = null;
	document.body.innerHTML = '';
	ipc.calls.length = 0;
});

async function settle(): Promise<void> {
	for (let i = 0; i < 10; i++) await Promise.resolve();
	flushSync();
}

/** The switch in the Display row whose label reads "Facetor's Lens: all feeders". */
function lensFeedersSwitch(): HTMLButtonElement {
	const row = [...document.querySelectorAll<HTMLElement>('.setting-row')].find(
		(r) => r.querySelector('.setting-label')?.textContent?.replaceAll('’', "'").trim() === "Facetor's Lens: all feeders"
	);
	const toggle = row?.querySelector<HTMLButtonElement>('button[role="switch"]');
	if (!toggle) throw new Error('the "Facetor\'s Lens: all feeders" switch did not render');
	return toggle;
}

const writes = () => ipc.calls.filter(([cmd]) => cmd === 'set_show_all_lens_feeders').map(([, args]) => args);

describe('SettingsPage all-feeders switch click', () => {
	// keep:happy
	it('each click writes the flipped choice through set_show_all_lens_feeders', async () => {
		app = mount(SettingsPage, { target: document.body });
		await settle();

		lensFeedersSwitch().click();
		flushSync();
		await settle();
		const afterOn = lensFeedersSwitch().getAttribute('aria-checked');
		lensFeedersSwitch().click();
		flushSync();
		await settle();

		expect(writes()).toEqual([{ show: true }, { show: false }]);
		expect([afterOn, lensFeedersSwitch().getAttribute('aria-checked')]).toEqual(['true', 'false']);
	});
});

// --- OCR Regions: exclusive Auto | Manual (POE-411) ---

type Rect = [number, number, number, number];
type ScreenRef = { width: number; height: number; origin: [number, number]; at_ms: number };
type RegionStates = Record<string, 'read' | 'clipped' | 'past-edge' | 'unset'>;

const HOUR_MS = 60 * 60 * 1000;
const at = (agoMs: number) => Date.now() - agoMs;
const screenRef = (width: number, height: number, atMs: number): ScreenRef => ({
	width,
	height,
	origin: [0, 0],
	at_ms: atMs
});

function screenSlice(width: number, height: number) {
	return {
		width,
		height,
		uiScale: 1.2,
		source: 'capture',
		measuredAtMs: at(2 * HOUR_MS),
		verifiedThisSession: false,
		monitorId: 65_537,
		origin: [0, 0],
		client: [0, 0, width, height],
		anchors: null
	};
}

function placements(lab: { gem: Rect; font: Rect; slots: Rect }, temple: [number, number] | null, merc: Rect | null) {
	const [x, y, , h] = lab.slots;
	return {
		lab: { gem: lab.gem, font: lab.font, gemSlots: [[x, y, h, h], [x + 132, y, h, h], [x + 264, y, h, h]] },
		temple: temple && { entranceOrigin: temple },
		merc: merc && { panel: merc },
		exchange: null
	};
}

const AUTO_3440 = placements({ gem: [0, 0, 3440, 53], font: [837, 293, 888, 908], slots: [1114, 497, 328, 64] }, [1720, 951], null);
const AUTO_2560 = placements({ gem: [0, 0, 2560, 53], font: [517, 293, 888, 908], slots: [794, 497, 328, 64] }, [1280, 951], null);
/** Rust `placements_for(None)`: the assumed 1080p lab, nothing else placed. */
const UNMEASURED = placements({ gem: [0, 0, 1920, 40], font: [298, 220, 666, 681], slots: [583, 403, 246, 48] }, null, null);

function rectRow(key: string, label: string, rect: Rect | null, source: string, regionStates?: RegionStates) {
	return { key, label, rect, rows: [], source, ...(regionStates ? { regionStates } : {}) };
}

/** `get_ocr_rects` in Auto: seeds, temple only on a measured screen, merc not located yet. */
function autoRows(lab: { gem: Rect; font: Rect } | null) {
	const temple = lab ? 'seed' : 'unlocated';
	return [
		rectRow('lab.gem', 'Gem tooltip', lab?.gem ?? [0, 0, 1920, 40], 'seed'),
		rectRow('lab.font', 'Font panel', lab?.font ?? [298, 220, 666, 681], 'seed'),
		rectRow('temple.panel', 'Temple panel', lab ? [1948, 6, 725, 605] : null, temple),
		rectRow('temple.remaining', 'Temple remaining', lab ? [1520, 1028, 400, 62] : null, temple),
		rectRow('merc.panel', 'Merc OCR crop', null, 'unlocated')
	];
}

const SEED = '(seed, scaled from reference)';
const AUTO_LABELS = ['Gem tooltip', 'Font panel', 'Temple panel', 'Temple remaining', 'Merc OCR crop'];

type ManualRects = { gem: Rect | null; font: Rect | null; temple: Rect | null; remaining: Rect | null; merc: Rect | null };

/** `get_ocr_rects` in Manual: every row `manual`, an unread region's rect withheld (Rust `assemble_region_rects`). */
function manualRows(rects: ManualRects, states: RegionStates) {
	const only = (...keys: string[]) => Object.fromEntries(keys.map((key) => [key, states[key]]));
	return [
		rectRow('lab.gem', 'Gem tooltip', rects.gem, 'manual', only('lab_gem')),
		rectRow('lab.font', 'Font panel', rects.font, 'manual', only('lab_font', 'lab_gem_slots')),
		rectRow('temple.panel', 'Temple panel', rects.temple, 'manual', only('temple_panel')),
		rectRow('temple.remaining', 'Temple remaining', rects.remaining, 'manual', only('temple_panel')),
		rectRow('merc.panel', 'Merc OCR crop', rects.merc, 'manual', only('merc_panel'))
	];
}

const SAVED = {
	labGem: [0, 0, 3440, 53] as Rect,
	labFont: [1820, 293, 888, 908] as Rect,
	labGemSlots: [2097, 497, 328, 64] as Rect,
	templePanel: [1948, 6, 725, 605] as Rect,
	mercPanel: [1387, 434, 666, 572] as Rect
};
const REGION_KEYS = ['lab_gem', 'lab_font', 'lab_gem_slots', 'temple_panel', 'merc_panel'];
const ALL_READ: RegionStates = Object.fromEntries(REGION_KEYS.map((key) => [key, 'read' as const]));
const ALL_UNSET: RegionStates = Object.fromEntries(REGION_KEYS.map((key) => [key, 'unset' as const]));
const NOTHING_SET = { labGem: null, labFont: null, labGemSlots: null, templePanel: null, mercPanel: null };

type ExpectedRow = { label: string; value?: string; source: string; edit: boolean };
const editable = (label: string, value: string, source: string): ExpectedRow => ({ label, value, source, edit: true });
const remaining: ExpectedRow = { label: 'Temple remaining', source: '(follows Temple panel)', edit: false };
const notRead = (label: string) => editable(label, 'not set', '(not read)');

type ManualCase = {
	state: string;
	screen: ReturnType<typeof screenSlice> | null;
	auto: typeof AUTO_3440 | null;
	manual: Record<string, unknown>;
	warning: { setOn: ScreenRef; live: ScreenRef } | null;
	/** Rust's resolved placements for the Manual set (`regions::resolve`). */
	placements: typeof AUTO_3440;
	rows: ReturnType<typeof manualRows>;
	expectedRows: ExpectedRow[];
	geometry: Record<string, string>;
	shows: string[];
	hides: string[];
	slotsPreview: string[];
	/** After the switch to Auto. */
	autoPlacements: typeof AUTO_3440;
	autoRows: ReturnType<typeof autoRows>;
	autoSources: string[];
};

const SET_ON_3440 = 'Set on 3440×1440 @ (0, 0)';
const COPIED = 'Copied from Auto';
const SCREEN_CHANGED = 'Screen changed';

const MANUAL_CASES: ManualCase[] = [
	{
		state: 'saved set on the live monitor',
		screen: screenSlice(3440, 1440),
		auto: AUTO_3440,
		manual: { setOn: screenRef(3440, 1440, at(2 * HOUR_MS)), edited: true, ...SAVED },
		warning: null,
		placements: placements({ gem: SAVED.labGem, font: SAVED.labFont, slots: SAVED.labGemSlots }, [1720, 951], SAVED.mercPanel),
		rows: manualRows(
			{ gem: SAVED.labGem, font: SAVED.labFont, temple: SAVED.templePanel, remaining: [1520, 1028, 400, 62], merc: SAVED.mercPanel },
			ALL_READ
		),
		expectedRows: [
			editable('Gem tooltip', '[0, 0, 3440, 53]', '(manual)'),
			editable('Font panel', '[1820, 293, 888, 908]', '(manual)'),
			editable('Gem choice slots', '[2097, 497, 328, 64]', '(manual)'),
			editable('Temple panel', '[1948, 6, 725, 605]', '(manual)'),
			remaining,
			editable('Merc OCR crop', '[1387, 434, 666, 572]', '(from Merc panel)')
		],
		geometry: {
			Resolution: '3440×1440',
			'Lab gem': '[0, 0, 3440, 53]',
			'Lab font': '[1820, 293, 888, 908]',
			'Temple Entrance': '(1720, 951)',
			'Merc panel': '[1387, 434, 666, 572]'
		},
		shows: [`${SET_ON_3440} · 2 h ago`],
		hides: [COPIED, SCREEN_CHANGED],
		slotsPreview: ['lab.font'],
		autoPlacements: AUTO_3440,
		autoRows: autoRows(AUTO_3440.lab),
		autoSources: [SEED, SEED, SEED, SEED, '(unlocated)']
	},
	{
		state: 'set saved on 3440×1440 while the game is on 2560×1440',
		screen: screenSlice(2560, 1440),
		auto: AUTO_2560,
		manual: { setOn: screenRef(3440, 1440, at(72 * HOUR_MS)), edited: true, ...SAVED },
		warning: { setOn: screenRef(3440, 1440, at(72 * HOUR_MS)), live: screenRef(2560, 1440, at(0)) },
		// Free crops are clipped at the live edge; scale-bearing Temple is skipped.
		placements: placements({ gem: [0, 0, 2560, 53], font: [1820, 293, 740, 908], slots: SAVED.labGemSlots }, [1720, 951], SAVED.mercPanel),
		rows: manualRows(
			{ gem: [0, 0, 2560, 53], font: [1820, 293, 740, 908], temple: null, remaining: null, merc: SAVED.mercPanel },
			{ ...ALL_READ, lab_gem: 'clipped', lab_font: 'clipped', temple_panel: 'past-edge' }
		),
		expectedRows: [
			editable('Gem tooltip', '[0, 0, 3440, 53]', '(past the screen edge)'),
			editable('Font panel', '[1820, 293, 888, 908]', '(past the screen edge)'),
			editable('Gem choice slots', '[2097, 497, 328, 64]', '(manual)'),
			editable('Temple panel', '[1948, 6, 725, 605]', '(past the screen edge)'),
			remaining,
			editable('Merc OCR crop', '[1387, 434, 666, 572]', '(from Merc panel)')
		],
		geometry: {
			Resolution: '2560×1440',
			'Lab gem': '[0, 0, 3440, 53]',
			'Lab font': '[1820, 293, 888, 908]',
			'Temple Entrance': '(1720, 951)',
			'Merc panel': '[1387, 434, 666, 572]'
		},
		shows: [`${SCREEN_CHANGED} · set on 3440×1440 @ (0, 0) · now 2560×1440 @ (0, 0)`],
		hides: [COPIED],
		slotsPreview: ['lab.font'],
		autoPlacements: AUTO_2560,
		autoRows: autoRows(AUTO_2560.lab),
		autoSources: [SEED, SEED, SEED, SEED, '(unlocated)']
	},
	{
		state: 'first-use set copied from Auto with Merc panel not set',
		screen: screenSlice(3440, 1440),
		auto: AUTO_3440,
		manual: {
			setOn: screenRef(3440, 1440, at(0)),
			edited: false,
			labGem: [0, 0, 3440, 53],
			labFont: [837, 293, 888, 908],
			labGemSlots: [1114, 497, 328, 64],
			templePanel: [1948, 6, 725, 605],
			mercPanel: null
		},
		warning: null,
		// Copied values equal Auto, so the resolved placements do too.
		placements: AUTO_3440,
		rows: manualRows(
			{ gem: [0, 0, 3440, 53], font: [837, 293, 888, 908], temple: [1948, 6, 725, 605], remaining: [1520, 1028, 400, 62], merc: null },
			{ ...ALL_READ, merc_panel: 'unset' }
		),
		expectedRows: [
			editable('Gem tooltip', '[0, 0, 3440, 53]', '(manual, copied from Auto)'),
			editable('Font panel', '[837, 293, 888, 908]', '(manual, copied from Auto)'),
			editable('Gem choice slots', '[1114, 497, 328, 64]', '(manual, copied from Auto)'),
			editable('Temple panel', '[1948, 6, 725, 605]', '(manual, copied from Auto)'),
			remaining,
			notRead('Merc OCR crop')
		],
		geometry: {
			Resolution: '3440×1440',
			'Lab gem': '[0, 0, 3440, 53]',
			'Lab font': '[837, 293, 888, 908]',
			'Temple Entrance': '(1720, 951)',
			'Merc panel': 'Not set'
		},
		shows: [`${COPIED} · just now · not set: Merc panel`],
		hides: [SCREEN_CHANGED],
		slotsPreview: ['lab.font'],
		autoPlacements: AUTO_3440,
		autoRows: autoRows(AUTO_3440.lab),
		autoSources: [SEED, SEED, SEED, SEED, '(unlocated)']
	},
	{
		state: 'set with only the Font panel placed on an unmeasured screen',
		screen: null,
		auto: null,
		manual: { setOn: screenRef(3440, 1440, at(2 * HOUR_MS)), edited: true, ...NOTHING_SET, labFont: [1820, 293, 888, 908] },
		warning: null,
		placements: placements({ gem: [0, 0, 1920, 40], font: [1820, 293, 888, 908], slots: [583, 403, 246, 48] }, null, null),
		rows: manualRows(
			{ gem: null, font: [1820, 293, 888, 908], temple: null, remaining: null, merc: null },
			{ ...ALL_UNSET, lab_font: 'read' }
		),
		expectedRows: [
			notRead('Gem tooltip'),
			editable('Font panel', '[1820, 293, 888, 908]', '(manual)'),
			notRead('Gem choice slots'),
			notRead('Temple panel'),
			remaining,
			notRead('Merc OCR crop')
		],
		geometry: {
			Resolution: 'Not measured yet',
			'Lab gem': 'Not set',
			'Lab font': '[1820, 293, 888, 908]',
			'Temple Entrance': 'Not set',
			'Merc panel': 'Not set'
		},
		shows: [`${SET_ON_3440} · 2 h ago`],
		hides: [COPIED, SCREEN_CHANGED],
		slotsPreview: [],
		autoPlacements: UNMEASURED,
		autoRows: autoRows(null),
		autoSources: [SEED, SEED, '(unlocated)', '(unlocated)', '(unlocated)']
	},
	{
		state: 'first-use set with every region not set',
		screen: null,
		auto: null,
		manual: { setOn: screenRef(3440, 1440, at(0)), edited: false, ...NOTHING_SET },
		warning: null,
		placements: UNMEASURED,
		rows: manualRows({ gem: null, font: null, temple: null, remaining: null, merc: null }, ALL_UNSET),
		expectedRows: [
			notRead('Gem tooltip'),
			notRead('Font panel'),
			notRead('Gem choice slots'),
			notRead('Temple panel'),
			remaining,
			notRead('Merc OCR crop')
		],
		geometry: {
			Resolution: 'Not measured yet',
			'Lab gem': 'Not set',
			'Lab font': 'Not set',
			'Temple Entrance': 'Not set',
			'Merc panel': 'Not set'
		},
		shows: [`${COPIED} · just now · not set: all regions`],
		hides: [SCREEN_CHANGED],
		slotsPreview: [],
		autoPlacements: UNMEASURED,
		autoRows: autoRows(null),
		autoSources: [SEED, SEED, '(unlocated)', '(unlocated)', '(unlocated)']
	}
];

/** The stubbed Rust side: `get_ssot` and `get_ocr_rects` answer whatever it currently holds. */
function serve(snapshot: Record<string, unknown>, rows: unknown[]) {
	const backend: { snapshot: Record<string, unknown>; rows: unknown[] } = { snapshot, rows };
	ipc.handlers.get_ssot = () => backend.snapshot;
	ipc.handlers.get_ocr_rects = () => backend.rows;
	return backend;
}

function ssotSnapshot(
	mode: 'auto' | 'manual',
	c: { screen: unknown; auto: unknown; manual: unknown; warning: unknown },
	placed: unknown
) {
	return {
		screen: c.screen,
		placements: placed,
		ocrRegions: { mode, manual: c.manual, auto: c.auto, screenWarning: c.warning }
	};
}

/** Mount Settings as the open view, with Temple and Merc entitled and the backend's snapshot applied. */
async function mountSettings(): Promise<void> {
	const { entitlements, TEMPLE_FEATURE, MERC_FEATURE } = await import('$lib/stores/entitlements.svelte');
	entitlements.features = [TEMPLE_FEATURE, MERC_FEATURE];
	(await import('$lib/stores/navigation.svelte')).nav.go('/settings');
	app = mount(SettingsPage, { target: document.body });
	await settle();
	const { fetchSsot } = await import('$lib/stores/ssot.svelte');
	await fetchSsot();
	await settle();
	await settle();
}

const squash = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

function section(title: string): HTMLElement | undefined {
	return [...document.querySelectorAll<HTMLElement>('section')].find(
		(s) => squash(s.querySelector('h2')).toLowerCase() === title.toLowerCase()
	);
}

function rowsOf(title: string): HTMLElement[] {
	return [...(section(title)?.querySelectorAll<HTMLElement>('.setting-row') ?? [])];
}

function labelled(title: string, label: string): HTMLElement | undefined {
	return rowsOf(title).find((row) => squash(row.querySelector('.setting-label')) === label);
}

function buttonIn(scope: ParentNode | undefined, text: string): HTMLButtonElement | undefined {
	return [...(scope?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => squash(b) === text);
}

function press(scope: ParentNode | undefined, text: string): void {
	const button = buttonIn(scope, text);
	if (!button) throw new Error(`no "${text}" button to press`);
	button.click();
}

/** The OCR Regions rows below Region mode, as the player reads them. */
function regionRows() {
	return rowsOf('OCR Regions')
		.filter((row) => squash(row.querySelector('.setting-label')) !== 'Region mode')
		.map((row) => ({
			label: squash(row.querySelector('.setting-label')),
			value: squash(row.querySelector('.setting-value')),
			source: squash(row.querySelector('.region-source')),
			edit: buttonIn(row, 'Edit') !== undefined
		}));
}

/** The pressed Region mode segment ('' when the control is absent). */
function regionMode(): string {
	return squash(labelled('OCR Regions', 'Region mode')?.querySelector('button[aria-pressed="true"]'));
}

function recalibrate(): { disabled: boolean; title: string } | null {
	const button = buttonIn(section('Screen geometry'), 'Recalibrate');
	return button ? { disabled: button.disabled, title: button.title } : null;
}

function geometryRows(labels: string[]): Record<string, string> {
	return Object.fromEntries(
		labels.map((label) => [label, squash(labelled('Screen geometry', label)?.querySelector('.setting-value'))])
	);
}

const modeWrites = () => ipc.calls.filter(([cmd]) => cmd === 'set_ocr_regions_mode').map(([, args]) => args);

describe('SettingsPage OCR Regions mode', () => {
	afterEach(() => {
		ipc.handlers = {};
		overlays.editorOpens.length = 0;
		overlays.previews.length = 0;
	});

	// keep:happy
	it.each(MANUAL_CASES)('a Manual $state renders as Manual until the player picks Auto', async (c) => {
		const backend = serve(ssotSnapshot('manual', c, c.placements), c.rows);
		ipc.handlers.set_ocr_regions_mode = (args) => {
			if ((args as { mode?: string })?.mode !== 'auto') throw new Error(`unexpected mode write ${JSON.stringify(args)}`);
			backend.snapshot = ssotSnapshot('auto', c, c.autoPlacements);
			backend.rows = c.autoRows;
			return null;
		};
		await mountSettings();

		expect(regionRows()).toEqual(c.expectedRows.map((row) => expect.objectContaining(row)));
		expect(regionMode()).toBe('Manual');
		expect(recalibrate()).toEqual({ disabled: true, title: 'Off in Manual mode' });
		expect(geometryRows(Object.keys(c.geometry))).toEqual(c.geometry);
		const text = squash(section('OCR Regions'));
		for (const shown of c.shows) expect(text).toContain(shown);
		for (const hidden of c.hides) expect(text).not.toContain(hidden);

		for (const row of rowsOf('OCR Regions')) {
			if (buttonIn(row, 'Edit')) press(row, 'Edit');
		}
		press(section('OCR Regions'), 'Edit regions');
		expect(overlays.editorOpens).toEqual(['labGem', 'labFont', 'labGemSlots', 'templePanel', 'mercPanel', null]);

		const slotsPreview = buttonIn(labelled('OCR Regions', 'Gem choice slots'), 'Preview');
		if (slotsPreview && !slotsPreview.disabled) slotsPreview.click();
		await settle();
		expect(overlays.previews).toEqual(c.slotsPreview);

		press(labelled('OCR Regions', 'Region mode'), 'Auto');
		await settle();
		await settle();
		await settle();

		expect(modeWrites()).toEqual([{ mode: 'auto' }]);
		expect(regionMode()).toBe('Auto');
		expect(regionRows()).toEqual(
			AUTO_LABELS.map((label, i) => expect.objectContaining({ label, source: c.autoSources[i], edit: false }))
		);
		expect(buttonIn(section('OCR Regions'), 'Edit regions')).toBeUndefined();
		expect(squash(section('OCR Regions'))).not.toContain(SCREEN_CHANGED);
		expect(recalibrate()?.disabled).toBe(false);
	});

	// keep:happy
	it('Auto shows seeds without Edit, and a refused Manual switch stays Auto until a retry succeeds', async () => {
		const auto = { screen: screenSlice(3440, 1440), auto: AUTO_3440, manual: null, warning: null };
		const backend = serve(ssotSnapshot('auto', auto, AUTO_3440), autoRows(AUTO_3440.lab));
		const copied = MANUAL_CASES[2];
		let monitorKnown = false;
		ipc.handlers.set_ocr_regions_mode = (args) => {
			if ((args as { mode?: string })?.mode !== 'manual') throw new Error(`unexpected mode write ${JSON.stringify(args)}`);
			if (!monitorKnown) throw 'game monitor unknown: focus the game once, then save';
			backend.snapshot = ssotSnapshot('manual', copied, copied.placements);
			backend.rows = copied.rows;
			return null;
		};
		await mountSettings();

		expect(regionMode()).toBe('Auto');
		expect(regionRows()).toEqual(
			AUTO_LABELS.map((label, i) =>
				expect.objectContaining({ label, source: [SEED, SEED, SEED, SEED, '(unlocated)'][i], edit: false })
			)
		);
		expect(buttonIn(section('OCR Regions'), 'Edit regions')).toBeUndefined();
		expect(recalibrate()).toEqual({ disabled: false, title: '' });

		press(labelled('OCR Regions', 'Region mode'), 'Manual');
		await settle();
		await settle();

		expect(modeWrites()).toEqual([{ mode: 'manual' }]);
		expect(squash(section('OCR Regions'))).toContain('Game monitor unknown — focus the game once');
		expect(regionMode()).toBe('Auto');
		expect(regionRows().some((row) => row.edit)).toBe(false);

		monitorKnown = true;
		press(labelled('OCR Regions', 'Region mode'), 'Manual');
		await settle();
		await settle();
		await settle();

		expect(modeWrites()).toEqual([{ mode: 'manual' }, { mode: 'manual' }]);
		expect(squash(section('OCR Regions'))).not.toContain('Game monitor unknown');
		expect(regionMode()).toBe('Manual');
	});
});
