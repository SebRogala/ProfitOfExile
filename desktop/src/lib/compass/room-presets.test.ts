import { describe, it, expect } from 'vitest';
import {
	normalizeDirections,
	getPresetByAreaCode,
	getPresetsByName,
	resolvePreset,
	getVisibleContentLocations,
	hasSecretPassage,
	isDoorDirection,
	defaultEditorAreaCode,
	hasGoldenDoor,
} from './room-presets';
import { createNavState, loadLayout, getNextDirection, type LabLayout, type NavState } from './navigation';
import uber20260728 from './__fixtures__/poelab-prod-2026-07-28-uber.json';
import uber20260729 from './__fixtures__/poelab-prod-2026-07-29-uber.json';
import uber20260802 from './__fixtures__/poelab-prod-2026-08-02-uber.json';
import merciless20180109 from './__fixtures__/poelab-2018-01-09-merciless.json';

// Fixture JSON infers exit records TypeScript will not cast straight to LabLayout.
const july28 = uber20260728 as unknown as LabLayout;
const july29 = uber20260729 as unknown as LabLayout;
const aug02 = uber20260802 as unknown as LabLayout;
const merciless2018 = merciless20180109 as unknown as LabLayout;

const allFixtures = import.meta.glob<{ default: LabLayout }>('./__fixtures__/*.json', { eager: true });

/** Normalized exits of one room as `target room id → direction`. */
function exitsOf(layout: LabLayout, roomId: string): Record<string, string> {
	const state = loadLayout(createNavState(), layout);
	return Object.fromEntries((state.exitDirections.get(roomId) ?? []).map((c) => [c.roomId, c.direction]));
}

describe('normalizeDirections', () => {
	it('assigns two exits that share a nearest door to different doors', () => {
		const result = normalizeDirections(
			[
				{ roomId: 'a', direction: 'N' },
				{ roomId: 'b', direction: 'NE' },
			],
			['N', 'E'],
		);
		expect(result).toEqual([
			{ roomId: 'a', direction: 'N' },
			{ roomId: 'b', direction: 'E' },
		]);
	});

	it('leaves a secret passage untouched without spending a door on it', () => {
		const result = normalizeDirections(
			[
				{ roomId: 'a', direction: 'N' },
				{ roomId: 'b', direction: 'C' },
				{ roomId: 'c', direction: '' },
			],
			['S', 'N'],
		);
		// The unknown reverse takes the one unused door: 'C' did not count.
		expect(result).toEqual([
			{ roomId: 'a', direction: 'N' },
			{ roomId: 'c', direction: 'S' },
			{ roomId: 'b', direction: 'C' },
		]);
	});

	it('lists regular directions before a secret passage before an unknown', () => {
		const result = normalizeDirections(
			[
				{ roomId: 'a', direction: '' },
				{ roomId: 'a', direction: 'C' },
				{ roomId: 'b', direction: 'N' },
				{ roomId: 'c', direction: '' },
			],
			['N', 'S', 'E', 'W'],
		);
		expect(result.map((c) => c.direction)).toEqual(['N', 'C', '', '']);
	});

	it('keeps an unknown reverse direction unknown when more than one door is unused', () => {
		const result = normalizeDirections(
			[
				{ roomId: 'a', direction: 'N' },
				{ roomId: 'b', direction: '' },
			],
			['N', 'S', 'E'],
		);
		expect(result[1]).toEqual({ roomId: 'b', direction: '' });
	});

	it('keeps two unknown reverse directions unknown even when they fill the door count', () => {
		const result = normalizeDirections(
			[
				{ roomId: 'a', direction: 'N' },
				{ roomId: 'b', direction: '' },
				{ roomId: 'c', direction: '' },
			],
			['N', 'S', 'E'],
		);
		expect(result.map((c) => c.direction)).toEqual(['N', '', '']);
	});

	it('breaks an equal-cost tie toward the earlier preset door', () => {
		const result = normalizeDirections([{ roomId: 'a', direction: 'N' }], ['NW', 'NE']);
		expect(result).toEqual([{ roomId: 'a', direction: 'NW' }]);
	});

	it('drops a connection whose direction is not a compass point, C or unknown', () => {
		const result = normalizeDirections(
			[
				{ roomId: 'a', direction: 'N' },
				{ roomId: 'b', direction: 'P' },
			],
			['N', 'S'],
		);
		expect(result).toEqual([{ roomId: 'a', direction: 'N' }]);
	});

	it('returns the directions unchanged when the room has more exits than preset doors', () => {
		const input = [
			{ roomId: 'a', direction: 'N' },
			{ roomId: 'b', direction: 'NE' },
			{ roomId: 'c', direction: 'S' },
		];
		expect(normalizeDirections(input, ['N', 'S'])).toEqual(input);
	});
});

describe('normalized exits at layout load', () => {
	it('maps the p_quad exits N/NE/NW one-to-one (2026-07-29 Uber room 7)', () => {
		// Preset doors [SE, W, NW, N]; min total cost 3: NE→N, N→NW, NW→W.
		// The reverse from room 6 is the single unknown and takes the unused SE.
		expect(exitsOf(july29, '7')).toEqual({ '9': 'NW', '8': 'N', '10': 'W', '6': 'SE' });
	});

	it('maps the c_quad_door exits N/NE/NW one-to-one (2026-08-02 Uber room 6)', () => {
		// Preset doors [SE, W, NW, NE]; min total cost 2: NE→NE, N→NW, NW→W.
		// Room 5 is the trial opening this section: no reverse door back into it.
		expect(exitsOf(aug02, '6')).toEqual({ '8': 'NW', '9': 'NE', '7': 'W' });
	});

	it('maps Sepulchre Halls c_branch_bottleneck_1 and deduces its reverse door (2026-07-28 Uber room 8)', () => {
		expect(exitsOf(july28, '8')).toEqual({ '10': 'N', '9': 'NW', '6': 'S' });
	});

	it('assigns every regular exit of every preset room in every fixture to its own preset door', () => {
		let checkedRooms = 0;
		for (const [file, mod] of Object.entries(allFixtures)) {
			const layout = mod.default;
			const state = loadLayout(createNavState(), layout);
			for (const room of layout.rooms) {
				// Upstream: an area code is looked up as given (the 2018 layouts'
				// '3_labyrinth_…' codes match nothing); only an empty one falls back
				// to a name with exactly one variant.
				const byName = getPresetsByName(room.name, room.contents.includes('golden-door'));
				const preset = room.areacode
					? getPresetByAreaCode(room.areacode)
					: byName.length === 1
						? byName[0]
						: null;
				if (!preset) continue;
				const regular = (state.exitDirections.get(room.id) ?? [])
					.map((c) => c.direction)
					.filter((d) => d !== '' && d !== 'C');
				if (regular.length > preset.doorLocations.length) continue;
				checkedRooms++;
				const where = `${file} room ${room.id} (${preset.areaCode})`;
				expect(new Set(regular).size, where).toBe(regular.length);
				for (const d of regular) expect(preset.doorLocations, where).toContain(d);
			}
		}
		// Every fixture room that upstream gives a preset; none has more exits than doors.
		expect(checkedRooms).toBe(46);
	});
});

describe('getNextDirection', () => {
	function at(layout: LabLayout, current: string, next: string): NavState {
		return { ...loadLayout(createNavState(), layout), currentRoom: current, plannedRoute: [current, next] };
	}

	it('returns the deduced preset door when backtracking into the room the player came from', () => {
		expect(getNextDirection(at(july28, '8', '6'))).toBe('S');
	});

	it('returns the normalized door for a forward exit', () => {
		expect(getNextDirection(at(july29, '7', '9'))).toBe('NW');
	});

	it('returns null when backtracking through a door whose direction is unknown', () => {
		const layout: LabLayout = {
			difficulty: 'Uber', date: '', weapon: '', phase1: '', phase2: '', trap1: '', trap2: '',
			rooms: [
				{ id: '1', name: 'Nowhere A', areacode: '', x: '0', y: '0', contents: [], exits: { E: '2' } },
				{ id: '2', name: 'Nowhere B', areacode: '', x: '1', y: '0', contents: [], exits: {} },
			],
		};
		expect(getNextDirection(at(layout, '2', '1'))).toBeNull();
	});
});

describe('resolvePreset', () => {
	const room = (areacode: string, name: string, contents: string[] = []) => ({ areacode, name, contents });

	it('finds the preset by an exact area code', () => {
		expect(resolvePreset(room('c_quad_door', 'Sepulchre Atrium'))?.areaCode).toBe('c_quad_door');
	});

	it('gives no preset for an area code that matches none, without falling back to the name (2018 Merciless room 11)', () => {
		// Its name alone has exactly one variant, so a name fallback would find one.
		const crossing = merciless2018.rooms.find((r) => r.id === '11')!;
		expect(crossing.areacode).toBe('3_labyrinth_oh_quad_#');
		expect(getPresetsByName(crossing.name, false)).toHaveLength(1);
		expect(resolvePreset(crossing)).toBeNull();
	});

	it('falls back to the name when the area code is empty and the name has one variant', () => {
		expect(resolvePreset(room('', 'Estate Crossing'))?.areaCode).toBe('oh_quad');
	});

	it('gives no preset for an empty area code whose name has several variants', () => {
		expect(getPresetsByName('Sepulchre Halls', false).length).toBeGreaterThan(1);
		expect(resolvePreset(room('', 'Sepulchre Halls'))).toBeNull();
	});

	it('picks the golden-door variant set from a golden-door content entry', () => {
		const golden = getPresetsByName('Basilica Halls', true);
		expect(golden).toHaveLength(1);
		expect(resolvePreset(room('', 'Basilica Halls', ['golden-door']))?.areaCode).toBe(golden[0].areaCode);
	});
});

describe('content markers (upstream instructionmodel.cpp:114-145)', () => {
	/** Visible content markers of a fixture room, with the player heading to `next`. */
	function markersOf(layout: LabLayout, roomId: string, next: string | null = null) {
		const state = loadLayout(createNavState(), layout);
		const room = state.roomById.get(roomId)!;
		const connections = state.exitDirections.get(roomId) ?? [];
		const nextIsSecret = next !== null && getNextDirection({ ...state, currentRoom: roomId, plannedRoute: [roomId, next] }) === 'C';
		return getVisibleContentLocations(resolvePreset(room)!, room.contents, hasSecretPassage(connections), nextIsSecret).map(
			(c) => ({ direction: c.direction, major: c.major, target: c.target }),
		);
	}

	it('shows major and minor positions when the room holds both kinds of loot (2026-07-29 Uber room 8)', () => {
		// c_end_bottleneck: major NW, minor SW; contents silver-door (major) + Switch puzzle (minor).
		expect(markersOf(july29, '8')).toEqual([
			{ direction: 'NW', major: true, target: false },
			{ direction: 'SW', major: false, target: false },
		]);
	});

	it('shows only the minor position when the only loot is a secret passage (2026-07-29 Uber room 4)', () => {
		// eh_end_bottleneck with no contents and a 'C' exit to room 3.
		expect(markersOf(july29, '4')).toEqual([{ direction: 'SW', major: false, target: false }]);
	});

	it('shows nothing when the room holds no loot', () => {
		const preset = getPresetByAreaCode('c_quad_door')!;
		expect(getVisibleContentLocations(preset, ['golden-door'], false, false)).toEqual([]);
	});

	it('shows only the major position when the room holds only major loot', () => {
		const preset = getPresetByAreaCode('c_end_bottleneck')!;
		expect(getVisibleContentLocations(preset, ['golden-key'], false, false).map((c) => c.direction)).toEqual(['NW']);
	});

	it('shows generic positions whenever the room holds loot (2026-08-02 Uber room 6)', () => {
		// c_quad_door generic content, contents golden-door + Trap gauntlet.
		const generic = getPresetByAreaCode('c_quad_door')!.contentLocations.generic!;
		expect(markersOf(aug02, '6').map((c) => c.direction)).toEqual(generic);
	});

	it('keeps content normal when a room has both a door and a secret passage to the next room (2026-07-29 Uber room 4 → 3)', () => {
		// Room 4's connections to 3 are its deduced door N, then 'C': the door is listed first.
		expect(markersOf(july29, '4', '3')).toEqual([{ direction: 'SW', major: false, target: false }]);
	});

	it('marks non-major content as the target when a secret passage is the only way on', () => {
		// No fixture has a preset room whose only connection onward is 'C', so
		// this one is built from real preset codes.
		const layout: LabLayout = {
			difficulty: 'Uber', date: '', weapon: '', phase1: '', phase2: '', trap1: '', trap2: '',
			rooms: [
				{ id: '1', name: 'Sepulchre Annex', areacode: 'c_end_bottleneck', x: '0', y: '0', contents: ['darkshrine'], exits: { C: '2' } },
				{ id: '2', name: 'Estate Path', areacode: 'oh_straight', x: '1', y: '0', contents: [], exits: {} },
			],
		};
		expect(markersOf(layout, '1', '2')).toEqual([{ direction: 'SW', major: false, target: true }]);
	});

	it('never marks major content as the target, even on a secret passage', () => {
		const preset = getPresetByAreaCode('c_end_bottleneck')!;
		const markers = getVisibleContentLocations(preset, ['golden-key', 'darkshrine'], true, true);
		expect(markers.map((c) => [c.direction, c.target])).toEqual([
			['NW', false],
			['SW', true],
		]);
	});
});

describe('secret passage connections', () => {
	it('counts a room with a C connection as having a secret passage (2026-07-29 Uber room 3)', () => {
		const state = loadLayout(createNavState(), july29);
		expect(hasSecretPassage(state.exitDirections.get('3')!)).toBe(true);
	});

	it('does not count a room without a C connection (2026-07-29 Uber room 2)', () => {
		const state = loadLayout(createNavState(), july29);
		expect(hasSecretPassage(state.exitDirections.get('2')!)).toBe(false);
	});

	it('never treats C as a door direction', () => {
		expect(isDoorDirection('C')).toBe(false);
		expect(isDoorDirection('')).toBe(false);
		expect(isDoorDirection('NE')).toBe(true);
	});
});

describe('defaultEditorAreaCode', () => {
	it('takes the room area code when it matches a preset', () => {
		expect(defaultEditorAreaCode({ areacode: 'c_quad_door', name: 'Sepulchre Atrium', contents: [] })).toBe('c_quad_door');
	});

	it('leaves the choice empty for an area code that matches no preset (2018 Merciless room 11)', () => {
		const crossing = merciless2018.rooms.find((r) => r.id === '11')!;
		expect(defaultEditorAreaCode(crossing)).toBe('');
	});

	it('takes the only variant of the room name when the area code is empty', () => {
		expect(defaultEditorAreaCode({ areacode: '', name: 'Estate Crossing', contents: [] })).toBe('oh_quad');
	});

	it('leaves the choice empty when the room name has several variants', () => {
		expect(defaultEditorAreaCode({ areacode: '', name: 'Sepulchre Halls', contents: [] })).toBe('');
	});
});

describe('hasGoldenDoor', () => {
	it('counts the exact golden-door entry', () => {
		expect(hasGoldenDoor(['Trap gauntlet', 'golden-door'])).toBe(true);
	});

	it('does not count another spelling or a golden key', () => {
		expect(hasGoldenDoor(['Golden-Door', 'golden-key'])).toBe(false);
	});
});
