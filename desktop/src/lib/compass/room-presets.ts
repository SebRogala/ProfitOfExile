/**
 * Room preset data and coordinate math for Lab Compass.
 * Ported from legacy LabCompass (yznpku/LabCompass, GPL-3.0).
 *
 * Loads room-presets.json at import time and provides O(1) lookups
 * by area code and case-insensitive lookups by room name.
 */

import roomTypesData from './room-presets.json';

export type Direction = 'N' | 'NE' | 'E' | 'SE' | 'S' | 'SW' | 'W' | 'NW';

export interface TileRect {
	x: number;
	y: number;
	width: number;
	height: number;
}

export interface RoomPreset {
	areaCode: string;
	doorLocations: Direction[];
	contentLocations: { generic?: Direction[]; major?: Direction[]; minor?: Direction[] };
	minimap: {
		rows: number;
		columns: number;
		directions: Record<string, [number, number]>;
	};
}

export interface RoomType {
	roomName: string;
	goldenDoor: boolean;
	variants: RoomPreset[];
}

export interface DoorExitLocation {
	direction: string;
	tileRect: TileRect;
}

export interface ContentLocation {
	direction: string;
	major: boolean;
	tileRect: TileRect;
}

// --- Lookup maps (built once at import) ---

const byAreaCode = new Map<string, RoomPreset>();
const byNameAndDoor = new Map<string, RoomPreset[]>();

for (const roomType of roomTypesData as unknown as RoomType[]) {
	const key = `${roomType.roomName.toLowerCase()}|${roomType.goldenDoor}`;
	if (!byNameAndDoor.has(key)) {
		byNameAndDoor.set(key, []);
	}
	for (const variant of roomType.variants) {
		byAreaCode.set(variant.areaCode, variant);
		byNameAndDoor.get(key)!.push(variant);
	}
}

// --- Public API ---

/** O(1) lookup by area code (e.g. "oh_straight"). */
export function getPresetByAreaCode(areaCode: string): RoomPreset | null {
	return byAreaCode.get(areaCode) ?? null;
}

/** Case-insensitive lookup by room name + golden door flag. Returns all variants. */
export function getPresetsByName(roomName: string, goldenDoor: boolean): RoomPreset[] {
	const key = `${roomName.toLowerCase()}|${goldenDoor}`;
	return byNameAndDoor.get(key) ?? [];
}

/**
 * The preset a layout room is drawn and normalized with. Upstream rule
 * (labyrinthdata.cpp:216-226, :94): a non-empty area code is looked up as
 * given, so one that matches nothing gives no preset; an empty one falls back
 * to the room name, but only when exactly one variant carries it. The golden
 * door flag is the exact 'golden-door' content entry, as poelab writes it.
 */
export function resolvePreset(room: { areacode: string; name: string; contents: string[] }): RoomPreset | null {
	if (room.areacode) return getPresetByAreaCode(room.areacode);
	const byName = getPresetsByName(room.name, room.contents.includes('golden-door'));
	return byName.length === 1 ? byName[0] : null;
}

/** Set of all valid area codes for validation. */
export const VALID_AREA_CODES: ReadonlySet<string> = new Set(byAreaCode.keys());

/**
 * Convert a grid coordinate to a normalized screen rect.
 * Direct port of RoomPresetHelper::getTileRect() from legacy C++.
 *
 * Returns rect in [0,1] normalized coordinates relative to the minimap container.
 */
export function getTileRect(preset: RoomPreset, direction: string): TileRect {
	const coord = preset.minimap.directions[direction];
	if (!coord) return { x: 0, y: 0, width: 0, height: 0 };

	const [row, column] = coord;
	const { rows, columns } = preset.minimap;
	const rowD = row - (rows - 1) / 2;
	const colD = column - (columns - 1) / 2;
	const scale = 10 / Math.max(Math.max(rows, columns), 7);
	const dx = (colD - rowD) * 0.05 * scale;
	const dy = (colD + rowD) * 0.05 * scale;
	const cx = 0.5 + dx;
	const cy = 0.5 + dy;
	const half = 0.05 * scale;

	return { x: cx - half, y: cy - half, width: half * 2, height: half * 2 };
}

/** Get all door exit positions with computed tile rects. */
export function getDoorExitLocations(preset: RoomPreset): DoorExitLocation[] {
	return preset.doorLocations.map((direction) => ({
		direction,
		tileRect: getTileRect(preset, direction),
	}));
}

/** One connection of a room: the neighbouring room and the direction it is reached by. */
export interface RoomConnection {
	roomId: string;
	/** A compass direction, 'C' for a secret passage, or '' when unknown (a reverse door). */
	direction: string;
}

const REGULAR_DIRECTIONS = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];

/** Squared step distance around the eight-point compass (upstream costBetweenDirections). */
function costBetweenDirections(d1: string, d2: string): number {
	const diff = Math.abs(REGULAR_DIRECTIONS.indexOf(d1) - REGULAR_DIRECTIONS.indexOf(d2));
	const steps = diff <= 4 ? diff : 8 - diff;
	return steps * steps;
}

/**
 * Map a room's connections onto its preset's door slots, the whole room at once.
 * Port of upstream DirectionNormalizer::normalize (directionnormalizer.cpp:11-46).
 *
 * Regular exits get the one-to-one door assignment of least total cost. Exactly
 * one unknown ('') connection, when regular + unknown equals the door count,
 * takes the one door left over; any other unknown stays ''. 'C' passes through
 * and spends no door. Any other direction code is dropped, as upstream's
 * breakdown drops it. The result lists regular directions, then 'C', then '',
 * each group in input order (upstream reconstructConnections).
 *
 * Tie-break: upstream leaves equal-cost assignments to its Hungarian solver's
 * order. Here the first minimum wins when assignments are enumerated with the
 * earlier exit taking the earlier door first. No fixture room has a tie.
 *
 * A room with more regular exits than doors is returned unchanged: upstream
 * does not guard that case.
 */
export function normalizeDirections(connections: RoomConnection[], pattern: string[]): RoomConnection[] {
	connections = connections.filter(
		(c) => REGULAR_DIRECTIONS.includes(c.direction) || c.direction === 'C' || c.direction === '',
	);
	const regular = connections.flatMap((c, i) => (REGULAR_DIRECTIONS.includes(c.direction) ? [i] : []));
	const unknown = connections.flatMap((c, i) => (c.direction === '' ? [i] : []));
	if (regular.length > pattern.length) return connections;

	let best: number[] = [];
	let bestCost = Infinity;
	const used = new Array<boolean>(pattern.length).fill(false);
	const chosen: number[] = [];
	const search = (k: number, cost: number) => {
		if (cost >= bestCost) return;
		if (k === regular.length) {
			bestCost = cost;
			best = [...chosen];
			return;
		}
		for (let j = 0; j < pattern.length; j++) {
			if (used[j]) continue;
			used[j] = true;
			chosen.push(j);
			search(k + 1, cost + costBetweenDirections(connections[regular[k]].direction, pattern[j]));
			chosen.pop();
			used[j] = false;
		}
	};
	search(0, 0);

	const result = connections.map((c) => ({ ...c }));
	regular.forEach((i, k) => (result[i].direction = pattern[best[k]]));

	if (unknown.length === 1 && unknown.length + regular.length === pattern.length) {
		const unused = pattern.filter((_, j) => !best.includes(j));
		result[unknown[0]].direction = unused[0];
	}
	const rank = (d: string) => (d === '' ? 2 : d === 'C' ? 1 : 0);
	return result.sort((a, b) => rank(a.direction) - rank(b.direction));
}

/** Get all content positions with computed tile rects. */
export function getContentLocations(preset: RoomPreset): ContentLocation[] {
	const result: ContentLocation[] = [];
	const { contentLocations } = preset;

	if (contentLocations.generic) {
		for (const direction of contentLocations.generic) {
			result.push({ direction, major: false, tileRect: getTileRect(preset, direction) });
		}
	}
	if (contentLocations.major) {
		for (const direction of contentLocations.major) {
			result.push({ direction, major: true, tileRect: getTileRect(preset, direction) });
		}
	}
	if (contentLocations.minor) {
		for (const direction of contentLocations.minor) {
			result.push({ direction, major: false, tileRect: getTileRect(preset, direction) });
		}
	}

	return result;
}
