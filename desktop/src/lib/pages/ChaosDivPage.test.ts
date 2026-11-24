// @vitest-environment happy-dom
/**
 * ChaosDivPage mounted in the client runtime under happy-dom with the real
 * calculator. Only the divine-rate store (cold), the clipboard boundary and
 * Tauri's `listen` are stubbed; every recorded `writeText` call is what Copy put
 * on the clipboard, and every exchange press reaches the page through the real
 * exchange-read store's registered listener, as the app layout starts it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Component } from 'svelte';

const mocks = vi.hoisted(() => ({
	/** The handlers registered through Tauri's `listen`, by event name. */
	handlers: {} as Record<string, (event: { payload: unknown }) => void>
}));

vi.mock('$lib/stores/divine-rate.svelte', () => ({ currentDivineRate: () => null }));
vi.mock('@tauri-apps/api/event', () => ({
	listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
		mocks.handlers[name] = handler;
		return () => {
			delete mocks.handlers[name];
		};
	})
}));

let mounted: Record<string, unknown> | null = null;
let stopStore: (() => void) | null = null;
let writeText: ReturnType<typeof vi.fn>;
let svelte: typeof import('svelte');
let ChaosDivPage: Component;

// A module reset gives each case an empty exchange-read store; the page, the store and
// mount/flushSync are then loaded from that same fresh graph so they share one Svelte runtime.
// `svelte` is re-mocked after each reset because a hoisted `vi.mock` factory result outlives
// the reset, which would leave the page's `onDestroy` and mount on the previous runtime.
beforeEach(async () => {
	vi.resetModules();
	// The `unit` project resolves no browser condition, and vite-plugin-svelte inlines `svelte`,
	// so NODE_OPTIONS conditions never reach it: bare `svelte` is the server entry, whose `mount` throws.
	vi.doMock('svelte', () => vi.importActual<typeof import('svelte')>('../../../node_modules/svelte/src/index-client.js'));
	mocks.handlers = {};
	svelte = await import('svelte');
	const store = await import('$lib/tools/exchange-read.svelte');
	ChaosDivPage = (await import('./ChaosDivPage.svelte')).default as Component;
	stopStore = store.startExchangeReadStore();
	writeText = vi.fn(async () => {});
	Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
});

afterEach(() => {
	if (mounted) svelte.unmount(mounted);
	mounted = null;
	stopStore?.();
	stopStore = null;
	document.body.innerHTML = '';
	vi.restoreAllMocks();
});

function render(): HTMLElement {
	mounted = svelte.mount(ChaosDivPage, { target: document.body });
	svelte.flushSync();
	return document.body;
}

function click(button: HTMLElement): void {
	button.click();
	svelte.flushSync();
}

function buttonLabelled(root: ParentNode, label: string): HTMLButtonElement {
	const button = [...root.querySelectorAll('button')].find((candidate) => candidate.textContent?.trim() === label);
	if (!button) throw new Error(`no ${label} button`);
	return button;
}

function type(id: string, value: string): void {
	const input = document.getElementById(id) as HTMLInputElement;
	input.value = value;
	input.dispatchEvent(new Event('input', { bubbles: true }));
	svelte.flushSync();
}

/** Delivers one native `exchange-read` payload through the store's listener. */
async function press(payload: unknown): Promise<void> {
	await vi.waitFor(() => expect(mocks.handlers['exchange-read']).toBeTypeOf('function'));
	mocks.handlers['exchange-read']({ payload });
	svelte.flushSync();
}

/** Epoch ms of today at a local wall-clock time, so the rendered HH:MM is timezone-independent. */
function localPress(hours: number, minutes: number): number {
	const at = new Date();
	at.setHours(hours, minutes, 30, 0);
	return at.getTime();
}

function pressed(group: string): string | undefined {
	return document.querySelector(`[role="group"][title="${group}"] button[aria-pressed="true"]`)?.textContent?.trim();
}

const STATUS_PREFIX = /^\d{2}:\d{2} · /;

/** The one rendered line that starts with a press time, whitespace-collapsed; null when none shows. */
function statusLine(): string | null {
	const text = (element: Element) => (element.textContent ?? '').replace(/\s+/g, ' ').trim();
	const lines = [...document.body.querySelectorAll('*')].filter(
		(element) => STATUS_PREFIX.test(text(element)) && ![...element.children].some((child) => STATUS_PREFIX.test(text(child)))
	);
	if (lines.length > 1) throw new Error(`${lines.length} status lines`);
	return lines.length ? text(lines[0]) : null;
}

/** The calculator inputs as the page renders them, plus the status line. */
function companion() {
	const ratio = document.getElementById('chaos-div-ratio') as HTMLInputElement;
	return {
		mode: pressed('Mode'),
		currency: pressed('Currency'),
		ratio: ratio.value,
		oneSide: ratio.closest('.ratio-control')?.querySelector('.fixed-one')?.textContent?.trim(),
		amount: (document.getElementById('chaos-div-amount') as HTMLInputElement).value,
		status: statusLine()
	};
}

/** The approved Ember-for-chaos top row as `exchange_read::finish` serializes it. */
const EMBER_FOR_CHAOS = {
	mode: 'sell',
	currency: 'chaos',
	ratio: 130,
	oneSide: 'right',
	left: 130,
	right: 1,
	stock: 1,
	wantName: 'Chaos Orb',
	haveName: 'Allflame Ember of the Ethereal'
} as const;

const EMBER_FILLED = {
	mode: 'Selling',
	currency: 'Chaos',
	ratio: '130',
	oneSide: ': 1',
	amount: '1',
	status: '21:04 · 130 : 1 · stock 1 · Chaos Orb ⇄ Allflame Ember of the Ethereal'
};

/** A `1 : b` top row: one Divine Orb sells for 6.12 Orbs of Alchemy. */
const DIVINE_ONE_LEFT = {
	mode: 'sell',
	currency: 'div',
	ratio: 6.12,
	oneSide: 'left',
	left: 1,
	right: 6.12,
	stock: 3,
	wantName: 'Divine Orb',
	haveName: 'Orb of Alchemy'
} as const;

describe('Companion totals', () => {
	it('shows a copyable item amount beside both currency totals with a cold rate', () => {
		const totals = render().querySelector('section[aria-label="Totals"]') as HTMLElement;
		expect(totals.querySelectorAll('.total-card')).toHaveLength(3);
		expect(totals.textContent).toContain('Item amount');
		expect(totals.querySelector('button[title="Copy item count"]')?.textContent?.trim()).toBe('Copy');
		expect((totals.querySelector('button[title="Copy divine total"]') as HTMLButtonElement).disabled).toBe(true);
	});
	it('provides locale-independent decimal entry for every numeric field', () => {
		const page = render();
		for (const id of ['chaos-div-amount', 'chaos-div-ratio']) {
			const input = page.querySelector(`#${id}`) as HTMLInputElement;
			expect(input.getAttribute('type')).toBe('text');
			expect(input.getAttribute('inputmode')).toBe('decimal');
		}
	});
});

describe('scaled order Copy', () => {
	it.each([
		{ title: 'Copy I want value', copied: '558' },
		{ title: 'Copy I have value', copied: '4' }
	])('keep:critical: $title on the scaled 139.50 : 1 undercut copies $copied, since an unscaled clipboard number silently places the wrong trade', ({ title, copied }) => {
		const page = render();
		click(buttonLabelled(page, 'Selling'));
		click(buttonLabelled(page, 'Chaos'));
		type('chaos-div-amount', '4');
		type('chaos-div-ratio', '140');
		const row = [...page.querySelectorAll('tbody tr')].find((candidate) => candidate.querySelector('.order-ratio')?.textContent === '139.50 : 1');
		if (!row) throw new Error('no 139.50 : 1 row');
		const cells = row.querySelectorAll('td');
		expect([cells[2].querySelector('span')?.textContent, cells[3].querySelector('span')?.textContent, cells[4].textContent])
			.toEqual(['558c', '4', '—']);
		(row.querySelector(`button[title="${title}"]`) as HTMLButtonElement).click();
		expect(writeText.mock.calls).toEqual([[copied]]);
	});
});

describe('exchange read in the Companion', () => {
	it('keep:happy: an Ember-for-chaos press fills the calculator with its timestamped status', async () => {
		render();

		await press({ pressedAtMs: localPress(21, 4), selection: EMBER_FOR_CHAOS });

		expect(companion()).toEqual(EMBER_FILLED);
	});

	it('keep:happy: a press read before the Companion mounts fills it on mount', async () => {
		await press({ pressedAtMs: localPress(21, 4), selection: EMBER_FOR_CHAOS });

		render();

		expect(companion()).toEqual(EMBER_FILLED);
	});

	it('keep:happy: a 1 : 6.12 press keeps the game\'s one-on-the-left orientation', async () => {
		render();

		await press({ pressedAtMs: localPress(9, 7), selection: DIVINE_ONE_LEFT });

		expect(companion()).toEqual({
			mode: 'Selling',
			currency: 'Divine',
			ratio: '6.12',
			oneSide: '1 :',
			amount: '1',
			status: '09:07 · 1 : 6.12 · stock 3 · Divine Orb ⇄ Orb of Alchemy'
		});
	});

	// keep:critical — silent: flipping oneSide through the mode switch fills the
	// inverse price, which the calculator then trusts for every total and Copy.
	it('keep:critical: a read over a prior buy/left state takes its mode and side as read, not flipped', async () => {
		const page = render();
		click(buttonLabelled(page, 'Buying'));
		type('chaos-div-amount', '5');
		expect([pressed('Mode'), companion().oneSide]).toEqual(['Buying', '1 :']);

		await press({ pressedAtMs: localPress(9, 7), selection: DIVINE_ONE_LEFT });

		expect(companion()).toEqual({
			mode: 'Selling',
			currency: 'Divine',
			ratio: '6.12',
			oneSide: '1 :',
			amount: '5',
			status: '09:07 · 1 : 6.12 · stock 3 · Divine Orb ⇄ Orb of Alchemy'
		});
	});

	// keep:critical — silent: a failed press that changes any typed input leaves a
	// plausible wrong trade in the calculator that the player then copies.
	it.each([
		{ error: 'panel-not-found', status: '09:07 · Exchange panel not found' },
		{ error: 'ratio-unreadable', status: '09:07 · Ratio unreadable' },
		{ error: 'names-unreadable', status: '09:07 · Item names unreadable' },
		{ error: 'no-currency-side', status: '09:07 · No Chaos or Divine side' },
		{ error: 'grab-failed', status: '09:07 · Screen grab failed' },
		{ error: 'privacy-refused', status: '09:07 · Data & privacy notice not accepted' }
	])('keep:critical: a $error press keeps every typed input, showing only "$status"', async ({ error, status }) => {
		const page = render();
		click(buttonLabelled(page, 'Buying'));
		click(buttonLabelled(page, 'Divine'));
		click(page.querySelector('button[aria-label="Swap which side is 1"]') as HTMLButtonElement);
		type('chaos-div-ratio', '77');
		type('chaos-div-amount', '5');

		await press({ pressedAtMs: localPress(9, 7), error });

		expect(companion()).toEqual({ mode: 'Buying', currency: 'Divine', ratio: '77', oneSide: ': 1', amount: '5', status });
	});

	it.each([
		{ action: 'typing a Ratio', act: () => type('chaos-div-ratio', '50'), fields: { mode: 'Selling', currency: 'Chaos', ratio: '50', oneSide: ': 1' } },
		{ action: 'choosing Buying', act: () => click(buttonLabelled(document.body, 'Buying')), fields: { mode: 'Buying', currency: 'Chaos', ratio: '130', oneSide: '1 :' } },
		{ action: 'choosing Divine', act: () => click(buttonLabelled(document.body, 'Divine')), fields: { mode: 'Selling', currency: 'Divine', ratio: '130', oneSide: ': 1' } },
		{ action: 'swapping the 1 side', act: () => click(document.querySelector('button[aria-label="Swap which side is 1"]') as HTMLButtonElement), fields: { mode: 'Selling', currency: 'Chaos', ratio: '130', oneSide: '1 :' } },
		{ action: 'Reset', act: () => click(buttonLabelled(document.body, 'Reset')), fields: { mode: 'Selling', currency: 'Chaos', ratio: '1', oneSide: ': 1' } }
	])('keep:happy: $action after a filled read clears the status, keeping the manual value', async ({ act, fields }) => {
		render();
		await press({ pressedAtMs: localPress(21, 4), selection: EMBER_FOR_CHAOS });
		expect(statusLine()).toBe(EMBER_FILLED.status);

		act();

		expect(companion()).toEqual({ ...fields, amount: '1', status: null });
	});
});
