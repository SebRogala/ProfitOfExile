/**
 * The direction compass, rendered with `svelte/server` (no DOM): the target
 * arrow and the content markers' colours.
 */
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import DirectionCompass from './DirectionCompass.svelte';
import type { ContentLocation } from './room-presets';

const content = (direction: string, major: boolean, target: boolean): ContentLocation => ({
	direction,
	major,
	target,
	tileRect: { x: 0, y: 0, width: 0, height: 0 },
});

function html(props: Record<string, unknown>): string {
	return render(DirectionCompass, { props: { directions: ['S', 'N'], ...props } }).body;
}

/** `fill` of every element whose class list holds `content-marker`. */
function contentFills(markup: string): string[] {
	return [...markup.matchAll(/<rect\b[^>]*class="[^"]*\bcontent-marker\b[^"]*"[^>]*>/g)].map(
		(m) => /fill="([^"]*)"/.exec(m[0])?.[1] ?? '',
	);
}

describe('DirectionCompass target arrow', () => {
	it('points an arrow at a door direction', () => {
		expect(html({ targetDirection: 'N' })).toContain('<line');
	});

	it('draws no arrow for a secret passage', () => {
		expect(html({ targetDirection: 'C' })).not.toContain('<line');
	});
});

describe('DirectionCompass content markers', () => {
	it('draws one marker per content position, normal ones orange', () => {
		expect(contentFills(html({ contents: [content('SW', false, false), content('NW', true, false)] }))).toEqual([
			'#FFA726',
			'#FFA726',
		]);
	});

	it('draws target content green', () => {
		expect(contentFills(html({ contents: [content('SW', false, true), content('NW', true, false)] }))).toEqual([
			'#8BC34A',
			'#FFA726',
		]);
	});
});
