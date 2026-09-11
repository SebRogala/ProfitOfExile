/**
 * The door widget's three states, rendered (POE-276).
 *
 * `svelte/server`'s `render()` in vitest's node environment — no DOM, so what
 * is asserted is the markup: which parts of the widget are drawn, and in what
 * order. WHICH state the widget is in is `doorWidget()`'s decision and is
 * tested in `view.test.ts`; this file is the component's half — that each
 * state draws what it says.
 */
import { describe, expect, it } from 'vitest';
import type { ComponentProps } from 'svelte';
import { render } from 'svelte/server';
import TempleDoorDiamond from './TempleDoorDiamond.svelte';
import type { DiamondView } from './slice';

type Props = ComponentProps<typeof TempleDoorDiamond>;

/** A square room with two corridors — the shape only has to be drawable. */
const DIAMOND: DiamondView = {
	corners: [
		[2, 0],
		[0, 2],
		[-2, 0],
		[0, -2]
	],
	seals: [
		{ neighbour: 'C2', edge: 'C1-C2', pos: [2, 0] },
		{ neighbour: 'C0', edge: 'C0-C1', pos: [-2, 0] }
	],
	topIcon: [1, -1],
	bottomIcon: [-1, 1]
};

function body(over: Partial<Props>): string {
	const props: Props = {
		diamond: DIAMOND,
		reading: null,
		layout: null,
		suggested: [],
		secondary: null,
		room: 'Chamber of Iron',
		exit: null,
		offer: null,
		offers: [],
		warning: null,
		...over
	};
	return render(TempleDoorDiamond, { props }).body;
}

/** The markup without Svelte's hydration comments, which carry no content. */
function drawn(html: string): string {
	return html.replace(/<!--[\s\S]*?-->/g, '').trim();
}

describe('TempleDoorDiamond', () => {
	it('draws nothing at all while idle — no empty frame over the game', () => {
		expect(drawn(body({ diamond: null, reading: null }))).toBe('');
	});

	it('draws the reading line alone in its frame on a first read', () => {
		// No room yet: the frame carries the line and nothing else — not the
		// room name or the warning the previous incursion's layout left behind.
		const html = body({ diamond: null, reading: 'reading…', warning: 'low-confidence read' });
		expect(html).toContain('reading…');
		expect(html).not.toContain('<svg');
		expect(html).not.toContain('Chamber of Iron');
		expect(html).not.toContain('low-confidence read');
	});

	it('draws the reading line after the whole previous room on a re-read', () => {
		// Last in the column is the no-layout-jump guarantee: the room name,
		// the shape and the warning are laid out before the line, so it
		// appearing and going cannot move or resize any of them.
		const html = body({ reading: 'reading…', warning: 'low-confidence read' });
		const line = html.indexOf('reading…');
		expect(html.indexOf('Chamber of Iron')).toBeGreaterThanOrEqual(0);
		expect(html.indexOf('</svg>')).toBeGreaterThanOrEqual(0);
		expect(line).toBeGreaterThan(html.indexOf('</svg>'));
		expect(line).toBeGreaterThan(html.indexOf('low-confidence read'));
	});

	it('draws the room without a reading line once the read has landed', () => {
		const html = body({ reading: null });
		expect(html).toContain('<svg');
		expect(html).not.toContain('class="status');
	});
});
