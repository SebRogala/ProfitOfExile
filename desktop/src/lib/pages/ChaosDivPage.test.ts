// @vitest-environment happy-dom
/**
 * ChaosDivPage mounted in the client runtime under happy-dom with the real
 * calculator. Only the divine-rate store (cold) and the clipboard boundary are
 * stubbed; every recorded `writeText` call is what Copy put on the clipboard.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import ChaosDivPage from './ChaosDivPage.svelte';

vi.mock('$lib/stores/divine-rate.svelte', () => ({ currentDivineRate: () => null }));

let mounted: Record<string, unknown> | null = null;
let writeText: ReturnType<typeof vi.fn>;

beforeEach(() => {
	writeText = vi.fn(async () => {});
	Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	document.body.innerHTML = '';
	vi.restoreAllMocks();
});

function render(): HTMLElement {
	mounted = mount(ChaosDivPage, { target: document.body });
	flushSync();
	return document.body;
}

function click(button: HTMLElement): void {
	button.click();
	flushSync();
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
	flushSync();
}

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
