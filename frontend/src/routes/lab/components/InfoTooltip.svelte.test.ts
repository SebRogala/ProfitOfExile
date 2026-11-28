import { afterEach, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
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

it('keep:regression web tooltip panel owns left alignment inside a centred Font cell', () => {
	const cell = mountInCell();
	const control = keyboardFocus(cell);
	control.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
	flushSync();
	const panel = cell.querySelector<HTMLElement>('.info-panel')!;
	expect(panel.querySelector('li')?.textContent).toBe('Ice Spear of Splitting: 170c');
	expect(getComputedStyle(panel).textAlign).toBe('left');
});

it('keep:regression web keyboard focus outline follows the circular info control', () => {
	const control = keyboardFocus(mountInCell());
	const style = getComputedStyle(control);
	expect.soft(style.outline, 'component focus outline must reach computed style').toContain('1px');
	expect.soft(style.outline).toContain('solid');
	expect.soft(style.outlineOffset).toBe('2px');
	expect.soft(style.borderRadius).toBe('50%');
});
