// @vitest-environment happy-dom
import { afterEach, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
// The app palette, as the main window loads it through app.css: the outline colour is var(--color-lab-blue).
import '../../../tokens.css';
import InfoTooltip from './InfoTooltip.svelte';

const mounted: ReturnType<typeof mount>[] = [];
const tables: HTMLTableElement[] = [];
const jackpot = '<b>Jackpot gems</b><ul><li>Ice Spear of Splitting: 170c</li></ul>';

afterEach(async () => {
	for (const component of mounted.splice(0)) await unmount(component);
	for (const table of tables.splice(0)) table.remove();
});

function mountInCell(): HTMLTableCellElement {
	const table = document.createElement('table');
	table.className = 'ft';
	const cell = table.createTBody().insertRow().insertCell();
	cell.style.textAlign = 'center';
	document.body.append(table);
	tables.push(table);
	flushSync(() => mounted.push(mount(InfoTooltip, { target: cell, props: { text: jackpot } })));
	return cell;
}

function keyboardFocus(cell: HTMLElement): HTMLElement {
	document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }));
	const control = cell.querySelector<HTMLElement>('[role="button"]')!;
	control.focus();
	flushSync();
	expect(document.activeElement).toBe(control);
	expect(control.matches(':focus-visible')).toBe(true);
	return control;
}

it('keep:regression desktop tooltip panel escapes the centred Font cell', () => {
	const cell = mountInCell();
	const control = keyboardFocus(cell);
	const panel = document.getElementById(control.getAttribute('aria-describedby') ?? '')!;
	expect(panel.getAttribute('role')).toBe('tooltip');
	expect(panel.style.visibility).toBe('visible');
	expect(panel.querySelector('li')?.textContent).toBe('Ice Spear of Splitting: 170c');
	expect(cell.contains(panel)).toBe(false);
	expect(getComputedStyle(panel).textAlign).not.toBe('center');
});

it('keep:regression desktop keyboard focus outline follows the circular info control', () => {
	const control = keyboardFocus(mountInCell());
	const style = getComputedStyle(control);
	expect.soft(style.outline, 'component focus outline must reach computed style').toContain('1px');
	expect.soft(style.outline).toContain('solid');
	expect.soft(style.outlineOffset).toBe('2px');
	expect.soft(style.borderRadius).toBe('50%');
});
