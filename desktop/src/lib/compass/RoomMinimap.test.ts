/**
 * The room minimap, rendered with `svelte/server` (no DOM): content marker
 * colours and the no-preset direction text. WHICH markers are visible is
 * `getVisibleContentLocations`'s decision (`room-presets.test.ts`).
 */
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import RoomMinimap from './RoomMinimap.svelte';
import type { ContentLocation, DoorExitLocation } from './room-presets';

const DOOR: DoorExitLocation = { direction: 'NE', tileRect: { x: 0.1, y: 0.1, width: 0.1, height: 0.1 } };
const content = (target: boolean): ContentLocation => ({
	direction: 'SW',
	major: false,
	target,
	tileRect: { x: 0.5, y: 0.5, width: 0.1, height: 0.1 },
});

function html(props: Record<string, unknown>): string {
	return render(RoomMinimap, { props: { areaCode: 'c_end_bottleneck', roomName: 'Sepulchre Annex', ...props } }).body;
}

/** Inner text of every `<tag>` whose class list holds `cls`. */
function texts(markup: string, tag: string, cls: string): string[] {
	const re = new RegExp(`<${tag}\\b[^>]*class="[^"]*\\b${cls}\\b[^"]*"[^>]*>([\\s\\S]*?)</${tag}>`, 'g');
	return [...markup.matchAll(re)].map((m) => m[1].replace(/<!--[\s\S]*?-->/g, '').trim());
}

describe('RoomMinimap content colours', () => {
	it('draws normal content orange', () => {
		expect(html({ doors: [DOOR], contents: [content(false)] })).toContain('--content-color: #FFA726');
	});

	it('draws target content green', () => {
		const markup = html({ doors: [DOOR], contents: [content(true)] });
		expect(markup).toContain('--content-color: #4caf50');
		expect(markup).not.toContain('#FFA726');
	});
});

describe('RoomMinimap without a preset', () => {
	it('names a secret passage instead of printing C', () => {
		expect(texts(html({ areaCode: '', targetDirection: 'C' }), 'div', 'fallback-dir')).toEqual(['Secret Passage']);
	});

	it('prints a regular direction as its compass code', () => {
		expect(texts(html({ areaCode: '', targetDirection: 'NE' }), 'div', 'fallback-dir')).toEqual(['NE']);
	});
});
