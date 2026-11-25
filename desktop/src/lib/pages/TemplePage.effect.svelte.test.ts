// @vitest-environment happy-dom
/**
 * TemplePage mounted in the client runtime under happy-dom, so the
 * Consolidation of Power row in its Settings card is clicked and hovered for
 * real. Rust is the stubbed `invoke`: `get_ssot` serves the temple slice, and
 * `temple_set_config` records the config it was sent and echoes it onto that
 * slice, which is what the real command does before the page re-fetches.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { templeSliceDefault } from '$lib/temple/slice';

const rust = vi.hoisted(() => ({
	calls: [] as [string, unknown][],
	temple: null as Record<string, unknown> | null,
	rejectConfig: null as string | null
}));

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (cmd: string, args?: unknown) => {
		rust.calls.push([cmd, args]);
		if (cmd === 'get_ssot') return { temple: rust.temple };
		if (cmd === 'temple_set_config') {
			if (rust.rejectConfig !== null) throw rust.rejectConfig;
			rust.temple = { ...rust.temple, config: (args as { config: unknown }).config };
			return null;
		}
		if (cmd === 'temple_value_table') return [];
		return null;
	})
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

/** The glosses Rust generates, as test data: distinct from the shipped copy, so
 *  a page that printed its own strings instead of the slice's fails here. */
const GLOSSES = {
	nonResidentExtraTier: 'fixture: option one lands high',
	residentCarry: 'fixture: option two carries',
	notTaken: 'fixture: no passive lands low',
	cradle: 'fixture: cradle always three',
	unsetPlays: 'nonResidentExtraTier'
};

const SEGMENT_TITLE = 'Atlas passive — the Consolidation of Power option you took.';

let { flushSync, mount, unmount } = await import('svelte');
let TemplePage: typeof import('./TemplePage.svelte').default;
let fetchSsot: typeof import('$lib/stores/ssot.svelte').fetchSsot;

// ssot.svelte.ts holds the slice at module level; fresh modules per case keep
// one case's echo from showing up as the next case's starting state.
beforeEach(async () => {
	vi.resetModules();
	({ flushSync, mount, unmount } = await import('svelte'));
	TemplePage = (await import('./TemplePage.svelte')).default;
	({ fetchSsot } = await import('$lib/stores/ssot.svelte'));
});

let app: ReturnType<typeof mount> | null = null;

afterEach(() => {
	if (app) unmount(app);
	app = null;
	document.body.innerHTML = '';
	rust.calls.length = 0;
	rust.temple = null;
	rust.rejectConfig = null;
});

async function settle(): Promise<void> {
	for (let i = 0; i < 10; i++) await Promise.resolve();
	flushSync();
}

/** Mount the page over a slice whose config is unset and whose other two flags
 *  are both off their defaults, so a write that dropped either is visible. */
async function mountUnset(glosses: typeof GLOSSES | null = GLOSSES): Promise<void> {
	rust.temple = {
		...templeSliceDefault(),
		config: { artefactsOfTheVaal: false, scarabOfTimelines: true, consolidation: null },
		consolidationGlosses: glosses
	};
	app = mount(TemplePage, { target: document.body });
	await fetchSsot('initial');
	await settle();
}

/** The Consolidation row: the element holding the segment group. */
function consolidationRow(): HTMLElement {
	const group = document.querySelector<HTMLElement>(`[role="group"][title="${SEGMENT_TITLE}"]`);
	const row = group?.closest<HTMLElement>('.choice');
	if (!group || !row) throw new Error('the Consolidation of Power row did not render');
	return row;
}

function segment(label: string): HTMLButtonElement {
	const button = [...consolidationRow().querySelectorAll<HTMLButtonElement>('button')].find(
		(b) => b.textContent?.trim() === label
	);
	if (!button) throw new Error(`the "${label}" segment did not render`);
	return button;
}

const pressed = () =>
	['Option 1', 'Option 2', 'Not taken'].filter((l) => segment(l).getAttribute('aria-pressed') === 'true');

const notSetMarks = () =>
	[...consolidationRow().querySelectorAll<HTMLElement>('*')].filter(
		(el) => el.children.length === 0 && el.textContent?.trim() === 'not set'
	);

/** The text of the tooltip portal currently shown, or '' when none is. */
const visibleTip = () =>
	[...document.querySelectorAll<HTMLElement>('[role="tooltip"]')].find(
		(t) => t.style.visibility === 'visible'
	)?.textContent ?? '';

const configWrites = () =>
	rust.calls.filter(([cmd]) => cmd === 'temple_set_config').map(([, args]) => args);

describe('TemplePage Consolidation of Power row', () => {
	// keep:happy
	it('keep:happy choosing Option 2 writes residentCarry with both flags carried and shows the echo', async () => {
		await mountUnset();

		segment('Option 2').click();
		flushSync();
		await settle();

		expect(configWrites()).toEqual([
			{ config: { artefactsOfTheVaal: false, scarabOfTimelines: true, consolidation: 'residentCarry' } }
		]);
		expect(pressed()).toEqual(['Option 2']);
		expect(notSetMarks()).toEqual([]);
	});

	// keep:happy
	it('keep:happy an unset option marks the row not set and its tooltip glosses each option from the slice', async () => {
		await mountUnset();

		const wrap = consolidationRow().querySelector<HTMLElement>('.tooltip-wrap');
		if (!wrap) throw new Error('the Consolidation row has no InfoTooltip');
		wrap.dispatchEvent(new MouseEvent('mouseenter'));
		flushSync();
		const tips = [...document.querySelectorAll<HTMLElement>('[role="tooltip"]')];
		const tip = tips.find((t) => t.style.visibility === 'visible')?.textContent ?? '';

		expect(pressed()).toEqual([]);
		expect(notSetMarks()).toHaveLength(1);
		expect(tip).toContain(`Option 1 — ${GLOSSES.nonResidentExtraTier}`);
		expect(tip).toContain(`Option 2 — ${GLOSSES.residentCarry}`);
		expect(tip).toContain(`Not taken — ${GLOSSES.notTaken}`);
		expect(tip).toContain(`Cradle of Creation — ${GLOSSES.cradle}`);
		expect(tip).toContain('advisor plays Option 1 until you pick');
	});
	// keep:happy
	it('keep:happy a reset echo after Option 2 clears the active segment and brings back not set', async () => {
		await mountUnset();
		segment('Option 2').click();
		flushSync();
		await settle();

		rust.temple = {
			...rust.temple,
			config: { artefactsOfTheVaal: false, scarabOfTimelines: true, consolidation: null }
		};
		await fetchSsot('poll');
		await settle();

		expect(configWrites()).toEqual([
			{ config: { artefactsOfTheVaal: false, scarabOfTimelines: true, consolidation: 'residentCarry' } }
		]);
		expect(pressed()).toEqual([]);
		expect(notSetMarks()).toHaveLength(1);
	});

	// keep:happy
	it('keep:happy null glosses leave the tooltip with option names and header only', async () => {
		await mountUnset(null);

		const wrap = consolidationRow().querySelector<HTMLElement>('.tooltip-wrap');
		if (!wrap) throw new Error('the Consolidation row has no InfoTooltip');
		wrap.dispatchEvent(new MouseEvent('mouseenter'));
		flushSync();
		const tip = visibleTip();

		expect(tip).toContain('Consolidation of Power');
		expect(tip).toContain('Option 1');
		expect(tip).toContain('Option 2');
		expect(tip).toContain('Not taken');
		expect(tip).toContain('not set');
		expect(tip).not.toContain(' — ');
		expect(tip).not.toContain('fixture:');
		expect(tip).not.toContain('Cradle');
		expect(tip).not.toContain('until you pick');
	});

	// keep:happy
	it('keep:happy choosing Not taken writes notTaken and shows it active without the not set mark', async () => {
		await mountUnset();

		segment('Not taken').click();
		flushSync();
		await settle();

		expect(configWrites()).toEqual([
			{ config: { artefactsOfTheVaal: false, scarabOfTimelines: true, consolidation: 'notTaken' } }
		]);
		expect(pressed()).toEqual(['Not taken']);
		expect(notSetMarks()).toEqual([]);
	});

	// keep:critical — silent: a refused save that rendered the chosen option
	// active would tell the player the advisor plays it while settings.json does not.
	it('keep:critical a refused save shows the rejection and keeps the row unset', async () => {
		await mountUnset();
		rust.rejectConfig = 'invalid args `config` for command `temple_set_config`: unknown variant';

		segment('Option 2').click();
		flushSync();
		await settle();
		await fetchSsot('poll');
		await settle();

		const errors = [...document.querySelectorAll<HTMLElement>('.settings-card p.error')].map((p) =>
			p.textContent?.trim()
		);
		expect(errors).toEqual(['invalid args `config` for command `temple_set_config`: unknown variant']);
		expect(pressed()).toEqual([]);
		expect(notSetMarks()).toHaveLength(1);
	});

	// keep:happy
	it('keep:happy keyboard focus on the info icon opens the tooltip portal with the glosses', async () => {
		await mountUnset();

		const icon = consolidationRow().querySelector<HTMLElement>('.tooltip-wrap .icon-circle');
		if (!icon) throw new Error('the Consolidation row has no InfoTooltip icon');
		icon.focus();
		flushSync();
		const tip = visibleTip();

		expect(document.activeElement).toBe(icon);
		expect(tip).toContain(`Option 1 — ${GLOSSES.nonResidentExtraTier}`);
		expect(tip).toContain(`Option 2 — ${GLOSSES.residentCarry}`);
		expect(tip).toContain(`Not taken — ${GLOSSES.notTaken}`);
		expect(tip).toContain(`Cradle of Creation — ${GLOSSES.cradle}`);
	});
});
