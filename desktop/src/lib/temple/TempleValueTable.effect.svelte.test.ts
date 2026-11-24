// @vitest-environment happy-dom
/**
 * TempleValueTable mounted in the client runtime under happy-dom, so the room
 * name's `Tooltip` handlers actually run. Each row's `line` is Rust's projected
 * fact set from WI-1's fixture, so the tooltip text is the shipped formatter
 * over the shipped data.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LineView, RoomValueView, TempleValueRow } from './slice';
import valueRowLines from './fixtures/value-row-lines.json';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

const FIXTURE = valueRowLines as unknown as { key: string; name: string; line: LineView }[];

let TempleValueTable: typeof import('./TempleValueTable.svelte').default;
let valueRows: typeof import('./values').valueRows;
let { flushSync, mount, unmount } = await import('svelte');

beforeEach(async () => {
	vi.resetModules();
	({ flushSync, mount, unmount } = await import('svelte'));
	TempleValueTable = (await import('./TempleValueTable.svelte')).default;
	({ valueRows } = await import('./values'));
});

let mounted: ReturnType<typeof mount> | null = null;
afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	document.body.innerHTML = '';
});

function tier(total: number): RoomValueView {
	return {
		total,
		priced: 'fallback',
		guessed: false,
		league: 'Mirage',
		asOf: null,
		scaledFromTier3: null,
		drivers: []
	};
}

function render(): HTMLElement {
	const rows: TempleValueRow[] = FIXTURE.map((entry, i) => ({
		key: entry.key,
		name: entry.name,
		grade: 'B1',
		pickOrder: i,
		tiers: [tier(8), tier(9), tier(10 + i)],
		line: entry.line
	}));
	const target = document.createElement('div');
	document.body.appendChild(target);
	mounted = mount(TempleValueTable, { target, props: { rows: valueRows(rows, null) } });
	flushSync();
	return target;
}

function roomWraps(target: HTMLElement): HTMLElement[] {
	return [...target.querySelectorAll<HTMLElement>('td.room .tooltip-wrap')];
}

/** The newest popup: each `Tooltip` appends its own on first show. */
function newestTip(): HTMLElement {
	const tips = [...document.querySelectorAll<HTMLElement>('[role="tooltip"]')];
	return tips[tips.length - 1];
}

function opensLabelledTip(tip: HTMLElement | undefined): boolean {
	return (
		tip !== undefined &&
		tip.style.visibility === 'visible' &&
		['Effect', 'Drops', 'Source'].every((label) => tip.textContent?.includes(label))
	);
}

function hover(target: HTMLElement, name: string): HTMLElement {
	const wrap = roomWraps(target).find((w) => w.textContent?.trim() === name)!;
	wrap.dispatchEvent(new Event('mouseenter'));
	flushSync();
	return newestTip();
}

describe('TempleValueTable room tooltip', () => {
	// keep:happy — AC-1, rendered.
	it('every room name opens its tooltip on hover', () => {
		const target = render();
		const wraps = roomWraps(target);
		expect(wraps).toHaveLength(25);

		let opened = 0;
		for (const wrap of wraps) {
			// Each room's first hover must create its own popup; a count that
			// stays put means a previous room's popup is the one still showing.
			const before = document.querySelectorAll('[role="tooltip"]').length;
			wrap.dispatchEvent(new Event('mouseenter'));
			flushSync();
			const created = document.querySelectorAll('[role="tooltip"]').length === before + 1;
			if (created && opensLabelledTip(newestTip())) opened++;
		}
		expect(opened).toBe(25);
	});

	// keep:happy — AC-1's keyboard-focus clause, rendered.
	it('every room name opens its tooltip on keyboard focus', () => {
		const target = render();
		const names = roomWraps(target).map((w) => w.querySelector<HTMLElement>('span')!);
		expect(names).toHaveLength(25);

		let opened = 0;
		for (const name of names) {
			expect(name.getAttribute('tabindex')).toBe('0');
			name.focus();
			flushSync();
			const tipId = name.getAttribute('aria-describedby');
			const tip = tipId === null ? undefined : (document.getElementById(tipId) ?? undefined);
			if (opensLabelledTip(tip)) opened++;
		}
		expect(opened).toBe(25);
	});

	// keep:happy — AC-2, rendered.
	it('Locus and Cradle tooltips render the 3.30 figures', () => {
		const target = render();
		const locus = FIXTURE.find((e) => e.name === 'Locus of Corruption')!.line;
		const cradle = FIXTURE.find((e) => e.name === 'Cradle of Creation')!.line;

		const locusTip = hover(target, 'Locus of Corruption').textContent ?? '';
		expect(locusTip).toContain('Rarity 8/12/12 %');
		expect(locusTip).toContain('Pack size 1/2/2 %');
		expect(locusTip).toContain(locus.sources[0].label);

		const cradleTip = hover(target, 'Cradle of Creation').textContent ?? '';
		expect(cradleTip).toContain('Magic packs 10/20/30 %');
		expect(cradleTip).toContain(cradle.sources[0].label);
	});

	// keep:happy — POE-487 AC-1: measured rarity and existing pack sizes,
	// with Factory's atlas drops still attributed to unverified feedback.
	it('keep:happy — Locus and Factory tooltips render measured rarity and its source', () => {
		const target = render();

		const locusTip = hover(target, 'Locus of Corruption').textContent ?? '';
		expect(locusTip).toContain('Rarity 8/12/12 %');
		expect(locusTip).toContain('Pack size 1/2/2 %');
		expect(locusTip).toContain('Rarity: poedb 2026-11-24');
		expect(locusTip).not.toContain('Rarity est.');

		const factoryTip = hover(target, 'Factory').textContent ?? '';
		expect(factoryTip).toContain('Extra item drops; +60% atlas drops');
		expect(factoryTip).toContain('Rarity 8/12/16 %');
		expect(factoryTip).toContain('Pack size 1/2/3 %');
		expect(factoryTip).toContain('Rarity: poedb 2026-11-24');
		expect(factoryTip).not.toContain('Rarity est.');
	});
});
