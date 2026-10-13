/**
 * The room editor's variant thumbnails, rendered with `svelte/server` (no DOM):
 * each draws through `RoomMinimap` and marks the route's exit and entry door
 * from `resolveRouteDoors`. WHICH door is which is the resolver's decision
 * (`navigation.test.ts`); these check that the editor draws what it returns.
 */
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import RoomEditor from './RoomEditor.svelte';
import RoomMinimap from './RoomMinimap.svelte';
import { getDoorExitLocations, getPresetByAreaCode } from './room-presets';
import type { LabLayout } from './navigation';
import uber20260728 from './__fixtures__/poelab-prod-2026-07-28-uber.json';

// 2026-07-28 Uber, planned route 1>3>4>3>5>6>8>10>11>14>16.
const july28 = uber20260728 as unknown as LabLayout;
const route = ['1', '3', '4', '3', '5', '6', '8', '10', '11', '14', '16'];

function editorHtml(roomId: string, plannedRoute = route, previousRoom: string | null = null): string {
	const room = july28.rooms.find((r) => r.id === roomId)!;
	return render(RoomEditor, { props: { room, rooms: july28.rooms, route: plannedRoute, previousRoom, serverUrl: '', difficulty: 'uber', onSave: () => {}, onClose: () => {} } }).body;
}

/** The markup of the thumbnail button for one variant. */
function thumb(markup: string, areaCode: string): string {
	const button = [...markup.matchAll(/<button\b[^>]*class="[^"]*\bvariant-thumb\b[\s\S]*?<\/button>/g)]
		.map((m) => m[0])
		.find((b) => b.includes(`title="${areaCode}"`));
	expect(button, areaCode).toBeDefined();
	return button!;
}

/** Labels of the door markers whose class list holds `cls`. */
function doorLabels(markup: string, cls: string): string[] {
	const re = new RegExp(`<div\\b[^>]*class="[^"]*\\bdoor-marker\\b[^"]*\\b${cls}\\b[^"]*"[^>]*>\\s*<span[^>]*>([^<]*)</span>`, 'g');
	return [...markup.matchAll(re)].map((m) => m[1].trim());
}

/** The inline style of every door marker, in order. */
function doorStyles(markup: string): string[] {
	return [...markup.matchAll(/<div\b[^>]*class="[^"]*\bdoor-marker\b[^"]*"[^>]*style="([^"]*)"/g)].map((m) => m[1]);
}

describe('RoomEditor variant thumbnails', () => {
	it('marks the exit and the entry door on the resolved variant of a route room', () => {
		// Room 8 resolves to c_branch_bottleneck_1 (S, NW, N): enters from 6 by S, leaves to 10 by N.
		const markup = thumb(editorHtml('8'), 'c_branch_bottleneck_1');
		expect(doorLabels(markup, 'target')).toEqual(['N']);
		expect(doorLabels(markup, 'entry')).toEqual(['S']);
	});

	it('takes the entry from the previous room when the route was replanned from the room', () => {
		// The live planner shape: the route starts at the current room 8, entered from 6.
		const markup = thumb(editorHtml('8', route.slice(route.indexOf('8')), '6'), 'c_branch_bottleneck_1');
		expect(doorLabels(markup, 'target')).toEqual(['N']);
		expect(doorLabels(markup, 'entry')).toEqual(['S']);
	});

	it('leaves the room name out of every thumbnail; the editor header names the room', () => {
		expect(editorHtml('8')).not.toMatch(/class="[^"]*\broom-badge\b/);
	});

	it('marks the doors normalized onto another variant of the same room', () => {
		// c_branch_door (SE, NW, NE): NE>10 stays NE, the unknown reverse from 6 takes SE.
		const markup = thumb(editorHtml('8'), 'c_branch_door');
		expect(doorLabels(markup, 'target')).toEqual(['NE']);
		expect(doorLabels(markup, 'entry')).toEqual(['SE']);
	});

	it('marks neither door for a room off the route', () => {
		const markup = thumb(editorHtml('7'), 'eh_end_bottleneck');
		expect(doorLabels(markup, 'target')).toEqual([]);
		expect(doorLabels(markup, 'entry')).toEqual([]);
		expect(doorStyles(markup).length).toBeGreaterThan(0);
	});

	it('places door markers exactly where the compass minimap places them', () => {
		const preset = getPresetByAreaCode('c_branch_door')!;
		const compass = render(RoomMinimap, {
			props: { areaCode: preset.areaCode, doors: getDoorExitLocations(preset) },
		}).body;
		expect(doorStyles(thumb(editorHtml('8'), 'c_branch_door'))).toEqual(doorStyles(compass));
	});
});

describe('RoomMinimap entry door', () => {
	it('styles the entry door apart from the exit and keeps both labels', () => {
		const preset = getPresetByAreaCode('c_branch_bottleneck_1')!;
		const markup = render(RoomMinimap, {
			props: { areaCode: preset.areaCode, doors: getDoorExitLocations(preset), targetDirection: 'N', entryDirection: 'S' },
		}).body;
		expect(doorLabels(markup, 'target')).toEqual(['N']);
		expect(doorLabels(markup, 'entry')).toEqual(['S']);
	});
});
