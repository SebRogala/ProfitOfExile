import { afterEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import FontEVCompare from './FontEVCompare.svelte';

const mounted: ReturnType<typeof mount>[] = [];
afterEach(async () => {
	for (const component of mounted.splice(0)) await unmount(component);
	document.body.replaceChildren();
	vi.unstubAllGlobals();
});

it.each([
	{ name: 'no covered cell', best: null, expected: [] },
	{ name: 'served BLUE 1/20 despite larger RED 20/20', best: { variant: '1/20', color: 'BLUE' }, expected: ['1/20:BLUE'] }
])('keep:happy: the BEST ring sits on exactly the served best cell ($name)', async ({ best, expected }) => {
	vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request) => {
		const url = new URL(String(input), window.location.origin);
		if (url.pathname !== '/api/analysis/font') throw new Error(`Unexpected request ${url.pathname}`);
		const variant = url.searchParams.get('variant');
		const rows = ['RED', 'GREEN', 'BLUE'].map((color) => {
			const ev = best === null ? null : variant === '20/20' && color === 'RED' ? 90 : 14;
			return { color, ev, evRaw: ev, profit: ev, pool: 88, pricedGems: best === null ? 0 : 88, coverage: best === null ? 0 : 1, winners: 0 };
		});
		return Response.json({ safe: rows, premium: rows, jackpot: rows, best });
	}));
	const target = document.body.appendChild(document.createElement('div'));
	flushSync(() => mounted.push(mount(FontEVCompare, { target, props: { league: 'Probe League' } })));
	await vi.waitFor(() => {
		flushSync();
		expect(target.querySelectorAll('table.ft tbody tr')).toHaveLength(4);
	});
	const cells = [...target.querySelectorAll('table.ft tbody tr')].flatMap((row) =>
		['RED', 'GREEN', 'BLUE'].map((color, index) => ({
			key: `${row.firstElementChild?.textContent?.trim()}:${color}`,
			cell: row.children[index + 1]
		}))
	);
	expect(cells.filter(({ cell }) => cell.matches('.w-red, .w-green, .w-blue')).map(({ key }) => key)).toEqual(expected);
	expect(cells.filter(({ cell }) => cell.querySelector('.best-red, .best-green, .best-blue')).map(({ key }) => key)).toEqual(expected);
});
