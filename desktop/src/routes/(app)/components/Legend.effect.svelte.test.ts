// @vitest-environment happy-dom
import { afterEach, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import Legend from './Legend.svelte';
import { TIER_RULES } from '$lib/tooltips';

const mounted: ReturnType<typeof mount>[] = [];
afterEach(async () => {
	for (const component of mounted.splice(0)) await unmount(component);
	document.body.replaceChildren();
});

function wi9PriceTierLines(): Element[] {
	const target = document.body.appendChild(document.createElement('div'));
	flushSync(() => mounted.push(mount(Legend, { target })));
	target.querySelector<HTMLButtonElement>('.legend-toggle')!.click();
	flushSync();
	const heading = [...target.querySelectorAll('h4')].find((node) => node.textContent?.startsWith('Price Tiers'))!;
	const lines: Element[] = [];
	for (let node = heading.nextElementSibling; node?.classList.contains('legend-item'); node = node.nextElementSibling) lines.push(node);
	return lines;
}

it('keep:happy WI-9: expanded Legend ends the price tier list with UNRATED', () => {
	const lines = wi9PriceTierLines();
	expect(lines.map((line) => line.querySelector('.sig')?.textContent)).toEqual(['TOP', 'HIGH', 'MID-HIGH', 'MID', 'LOW', 'FLOOR', 'UNRATED']);
});

it('keep:happy WI-9: expanded Legend shows UNRATED in yellow', () => {
	const last = wi9PriceTierLines().at(-1)!;
	expect(last.querySelector('.sig')?.classList.contains('yellow')).toBe(true);
});

it('keep:happy WI-9: expanded Legend glosses UNRATED with the shared tier rule', () => {
	const last = wi9PriceTierLines().at(-1)!;
	expect(last.textContent?.replace(/\s+/g, ' ').trim()).toBe('UNRATED under 4 solid gems');
	expect(last.textContent).toContain((TIER_RULES as Record<string, string>).UNRATED);
});
