/**
 * The compass overlay in minimap mode, rendered with `svelte/server` (no DOM):
 * the live widget's entry and exit doors reach the minimap through it.
 */
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import CompassOverlay from './CompassOverlay.svelte';
import { getDoorExitLocations, getPresetByAreaCode } from './room-presets';

/** Labels of the door markers whose class list holds `cls`. */
function doorLabels(markup: string, cls: string): string[] {
	const re = new RegExp(`<div\\b[^>]*class="[^"]*\\bdoor-marker\\b[^"]*\\b${cls}\\b[^"]*"[^>]*>\\s*<span[^>]*>([^<]*)</span>`, 'g');
	return [...markup.matchAll(re)].map((m) => m[1].trim());
}

describe('CompassOverlay minimap', () => {
	const preset = getPresetByAreaCode('c_branch_bottleneck_1')!;
	const html = () =>
		render(CompassOverlay, {
			props: {
				mode: 'minimap',
				areaCode: preset.areaCode,
				doors: getDoorExitLocations(preset),
				targetDirection: 'N',
				entryDirection: 'S',
				roomName: 'sepulchre halls',
			},
		}).body;

	it('marks the exit and the entry door it is given', () => {
		expect(doorLabels(html(), 'target')).toEqual(['N']);
		expect(doorLabels(html(), 'entry')).toEqual(['S']);
	});

	it('keeps the room name badge in the live compass', () => {
		expect(html()).toMatch(/class="[^"]*\broom-badge\b[^"]*"[^>]*>sepulchre halls</);
	});
});
