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

for (const roomType of roomTypesData as RoomType[]) {
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
