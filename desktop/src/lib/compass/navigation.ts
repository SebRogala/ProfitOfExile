/**
 * Lab Compass navigation engine.
 *
 * Tracks player position in the labyrinth using daily layout data and
 * lab-nav events from Rust. Computes optimal routes based on strategy.
 *
 * Pure functions — no side effects, no Svelte reactivity. The overlay
 * page calls these and stores the result in $state.
 */

// --- Types ---

export interface LabLayoutRoom {
	name: string;
	areacode: string;
	id: string;
	x: string;
	y: string;
	contents: string[];
	exits: Record<string, string>;
	dangerous?: string;
	secret_passage?: string;
	content_directions?: string[];
}

export interface LabLayout {
	difficulty: string;
	date: string;
	weapon: string;
	phase1: string;
	phase2: string;
	trap1: string;
	trap2: string;
	rooms: LabLayoutRoom[];
}

export interface LabSection {
	roomIds: string[];
	startRoom: string;
	endRoom: string; // Aspirant's Trial that ends this section
}

export type RouteStrategy = 'shortest' | 'darkshrines' | 'darkshrines-argus' | 'everything';

export interface NavState {
	layout: LabLayout | null;
	inLab: boolean;
	finished: boolean;
	currentRoom: string | null;
	possibleRooms: Set<string>;
	previousRoom: string | null;
	plannedRoute: string[];
	targetRooms: string[];
	strategy: RouteStrategy;
	goldenKeys: number;
	lockedDoors: [string, string][];
	portalRooms: Set<string>;
	sections: LabSection[];
	adjacency: Map<string, string[]>;
	roomById: Map<string, LabLayoutRoom>;
}

export type NavEvent =
	| { type: 'PlazaEntered' }
	| { type: 'RoomChanged'; name: string }
	| { type: 'LabStarted' }
	| { type: 'SectionFinished' }
	| { type: 'LabFinished' }
	| { type: 'IzaroBattleStarted' }
	| { type: 'PortalSpawned' }
	| { type: 'LabExited' };

// --- Factory ---

export function createNavState(): NavState {
	return {
		layout: null,
		inLab: false,
		finished: false,
		currentRoom: null,
		possibleRooms: new Set(),
		previousRoom: null,
		plannedRoute: [],
		targetRooms: [],
		strategy: 'shortest',
		goldenKeys: 0,
		lockedDoors: [],
		portalRooms: new Set(),
		sections: [],
		adjacency: new Map(),
		roomById: new Map(),
	};
}

// --- Layout loading ---

export function loadLayout(state: NavState, layout: LabLayout): NavState {
	const roomById = new Map<string, LabLayoutRoom>();
	const adjacency = new Map<string, string[]>();

	for (const room of layout.rooms) {
		roomById.set(room.id, room);
		if (!adjacency.has(room.id)) adjacency.set(room.id, []);
	}

	// Build bidirectional adjacency from exits
	for (const room of layout.rooms) {
		for (const targetId of Object.values(room.exits)) {
			const neighbors = adjacency.get(room.id)!;
			if (!neighbors.includes(targetId)) neighbors.push(targetId);
			const reverseNeighbors = adjacency.get(targetId);
			if (reverseNeighbors && !reverseNeighbors.includes(room.id)) {
				reverseNeighbors.push(room.id);
			}
		}
	}

	// Detect sections: split at Aspirant's Trial rooms
	const sections = detectSections(layout.rooms, adjacency);

	// Identify golden door/key locations
	const lockedDoors: [string, string][] = [];
	for (const room of layout.rooms) {
		if (room.contents.some((c) => c.toLowerCase().includes('golden-door'))) {
			// Find the connection through the golden door
			for (const neighborId of adjacency.get(room.id) ?? []) {
				const neighbor = roomById.get(neighborId);
				if (neighbor?.contents.some((c) => c.toLowerCase().includes('golden-door'))) {
					const pair: [string, string] = [room.id, neighborId].sort() as [string, string];
					if (!lockedDoors.some(([a, b]) => a === pair[0] && b === pair[1])) {
						lockedDoors.push(pair);
					}
				}
			}
		}
	}

	const newState: NavState = {
		...state,
		layout,
		roomById,
		adjacency,
		sections,
		lockedDoors,
		goldenKeys: 0,
		portalRooms: new Set(),
	};

	newState.plannedRoute = computeRoute(newState, state.strategy);
	newState.targetRooms = getTargetRooms(newState, state.strategy);

	return newState;
}

function detectSections(rooms: LabLayoutRoom[], adjacency: Map<string, string[]>): LabSection[] {
	const sections: LabSection[] = [];
	let currentSection: string[] = [];
	const trialRooms: string[] = [];

	// Rooms are ordered in the poelab JSON — trials separate sections
	for (const room of rooms) {
		if (room.name.toLowerCase() === "aspirant's trial") {
			trialRooms.push(room.id);
			if (currentSection.length > 0) {
				sections.push({
					roomIds: currentSection,
					startRoom: currentSection[0],
					endRoom: room.id,
				});
				currentSection = [];
			}
		} else {
			currentSection.push(room.id);
		}
	}

	// Last section (after last trial) — if any rooms remain
	if (currentSection.length > 0 && trialRooms.length > 0) {
		sections.push({
			roomIds: currentSection,
			startRoom: currentSection[0],
			endRoom: trialRooms[trialRooms.length - 1],
		});
	}

	return sections;
}

// --- Event handling ---

export function handleNavEvent(state: NavState, event: NavEvent): NavState {
	switch (event.type) {
		case 'PlazaEntered':
			return {
				...state,
				inLab: true,
				finished: false,
				currentRoom: null,
				possibleRooms: new Set(),
				previousRoom: null,
				goldenKeys: 0,
				portalRooms: new Set(),
				targetRooms: getTargetRooms(state, state.strategy),
				plannedRoute: computeRoute(state, state.strategy),
			};

		case 'RoomChanged': {
			if (!state.layout) return state;

			const matchingRooms = state.layout.rooms
				.filter((r) => r.name.toLowerCase() === event.name.toLowerCase())
				.map((r) => r.id);

			if (matchingRooms.length === 0) return state;

			let candidates: string[];

			// Priority 1: if following planned route and next room matches
			if (
				state.currentRoom &&
				state.plannedRoute.length >= 2 &&
				matchingRooms.includes(state.plannedRoute[1])
			) {
				candidates = [state.plannedRoute[1]];
			}
			// Priority 2: intersect with rooms connected to current position
			else if (state.currentRoom) {
				const connected = state.adjacency.get(state.currentRoom) ?? [];
				candidates = matchingRooms.filter((id) => connected.includes(id));
				if (candidates.length === 0) candidates = matchingRooms;
			}
			// Priority 3: first room (entering from plaza)
			else {
				candidates = matchingRooms;
			}

			const newState = { ...state };
			newState.possibleRooms = new Set(candidates);

			if (candidates.length === 1) {
				const confirmedId = candidates[0];
				newState.previousRoom = state.currentRoom;
				newState.currentRoom = confirmedId;

				// Golden key pickup
				const room = state.roomById.get(confirmedId);
				if (room?.contents.some((c) => c.toLowerCase().includes('golden-key'))) {
					newState.goldenKeys = state.goldenKeys + 1;
				}

				// Golden door unlock (crossing between locked pair)
				if (state.previousRoom) {
					const pair = [state.previousRoom, confirmedId].sort() as [string, string];
					const doorIdx = state.lockedDoors.findIndex(
						([a, b]) => a === pair[0] && b === pair[1],
					);
					if (doorIdx >= 0) {
						newState.lockedDoors = [...state.lockedDoors];
						newState.lockedDoors.splice(doorIdx, 1);
						newState.goldenKeys = Math.max(0, newState.goldenKeys - 1);
					}
				}

				// Remove from targets
				newState.targetRooms = state.targetRooms.filter((id) => id !== confirmedId);

				// Recompute remaining route from current position
				newState.plannedRoute = computeRouteFrom(newState, confirmedId, state.strategy);
			} else {
				newState.currentRoom = null;
			}

			return newState;
		}

		case 'PortalSpawned': {
			if (!state.currentRoom) return state;
			const portalRooms = new Set(state.portalRooms);
			portalRooms.add(state.currentRoom);
			return { ...state, portalRooms };
		}

		case 'LabFinished':
			return { ...state, finished: true };

		case 'LabExited':
			return {
				...state,
				inLab: false,
				finished: false,
				currentRoom: null,
				possibleRooms: new Set(),
				previousRoom: null,
				goldenKeys: 0,
				portalRooms: new Set(),
			};

		default:
			return state;
	}
}

// --- Routing ---

function getTargetRooms(state: NavState, strategy: RouteStrategy): string[] {
	if (!state.layout) return [];
	const targets: string[] = [];
	for (const room of state.layout.rooms) {
		if (room.name.toLowerCase() === "aspirant's trial") continue;
		const contentsLower = room.contents.map((c) => c.toLowerCase());
		switch (strategy) {
			case 'shortest':
				break;
			case 'darkshrines':
				if (contentsLower.includes('darkshrine')) targets.push(room.id);
				break;
			case 'darkshrines-argus':
				if (contentsLower.includes('darkshrine') || contentsLower.includes('argus'))
					targets.push(room.id);
				break;
			case 'everything':
				if (room.contents.length > 0) targets.push(room.id);
				break;
		}
	}
	return targets;
}

export function computeRoute(state: NavState, strategy: RouteStrategy): string[] {
	if (!state.layout || state.sections.length === 0) return [];
	const firstRoom = state.layout.rooms[0];
	if (!firstRoom) return [];
	return computeRouteFrom(state, firstRoom.id, strategy);
}

function computeRouteFrom(state: NavState, fromRoom: string, strategy: RouteStrategy): string[] {
	if (!state.layout || state.sections.length === 0) return [];

	const route: string[] = [];
	const targets = state.targetRooms;

	// Find which section the fromRoom belongs to
	let startSectionIdx = 0;
	for (let i = 0; i < state.sections.length; i++) {
		if (state.sections[i].roomIds.includes(fromRoom)) {
			startSectionIdx = i;
			break;
		}
	}

	for (let i = startSectionIdx; i < state.sections.length; i++) {
		const section = state.sections[i];
		const start = i === startSectionIdx ? fromRoom : section.startRoom;
		const end = section.endRoom;
		const sectionTargets = targets.filter((t) => section.roomIds.includes(t));

		const sectionRoute = shortestPathThroughTargets(
			state.adjacency,
			start,
			end,
			sectionTargets,
		);
		// Avoid duplicate room at section boundaries
		if (route.length > 0 && sectionRoute.length > 0 && route[route.length - 1] === sectionRoute[0]) {
			route.push(...sectionRoute.slice(1));
		} else {
			route.push(...sectionRoute);
		}
	}

	return route;
}

function shortestPathThroughTargets(
	adjacency: Map<string, string[]>,
	start: string,
	end: string,
	targets: string[],
): string[] {
	if (targets.length === 0) {
		return bfs(adjacency, start, end);
	}

	// For small target sets (typically 1-4 per section), try all permutations
	const perms = permutations(targets);
	let bestRoute: string[] = [];
	let bestLength = Infinity;

	for (const perm of perms) {
		const waypoints = [start, ...perm, end];
		let route: string[] = [];
		let valid = true;

		for (let i = 0; i < waypoints.length - 1; i++) {
			const segment = bfs(adjacency, waypoints[i], waypoints[i + 1]);
			if (segment.length === 0) {
				valid = false;
				break;
			}
			if (route.length > 0) {
				route.push(...segment.slice(1));
			} else {
				route = segment;
			}
		}

		if (valid && route.length < bestLength) {
			bestLength = route.length;
			bestRoute = route;
		}
	}

	return bestRoute;
}

function bfs(adjacency: Map<string, string[]>, start: string, end: string): string[] {
	if (start === end) return [start];
	const visited = new Set<string>([start]);
	const parent = new Map<string, string>();
	const queue = [start];

	while (queue.length > 0) {
		const current = queue.shift()!;
		for (const neighbor of adjacency.get(current) ?? []) {
			if (visited.has(neighbor)) continue;
			visited.add(neighbor);
			parent.set(neighbor, current);
			if (neighbor === end) {
				// Reconstruct path
				const path: string[] = [];
				let node: string | undefined = end;
				while (node !== undefined) {
					path.unshift(node);
					node = parent.get(node);
				}
				return path;
			}
			queue.push(neighbor);
		}
	}

	return []; // No path found
}

function permutations<T>(arr: T[]): T[][] {
	if (arr.length <= 1) return [arr];
	const result: T[][] = [];
	for (let i = 0; i < arr.length; i++) {
		const rest = [...arr.slice(0, i), ...arr.slice(i + 1)];
		for (const perm of permutations(rest)) {
			result.push([arr[i], ...perm]);
		}
	}
	return result;
}

// --- Query helpers ---

/** Get the exit direction from currentRoom to the next room on the planned route. */
export function getNextDirection(state: NavState): string | null {
	if (!state.currentRoom || state.plannedRoute.length < 2) return null;

	const currentIdx = state.plannedRoute.indexOf(state.currentRoom);
	if (currentIdx < 0 || currentIdx >= state.plannedRoute.length - 1) return null;

	const nextRoomId = state.plannedRoute[currentIdx + 1];
	const currentRoom = state.roomById.get(state.currentRoom);
	if (!currentRoom) return null;

	// Check forward exits (current room exits to next room)
	for (const [direction, targetId] of Object.entries(currentRoom.exits)) {
		if (targetId === nextRoomId) return direction;
	}

	// Check reverse exits (next room exits to current room — backtracking)
	// The direction to go is the opposite of the next room's exit to us.
	const nextRoom = state.roomById.get(nextRoomId);
	if (nextRoom) {
		for (const [direction, targetId] of Object.entries(nextRoom.exits)) {
			if (targetId === state.currentRoom) return OPPOSITE_DIR[direction] ?? direction;
		}
	}

	return null;
}

const OPPOSITE_DIR: Record<string, string> = {
	N: 'S', S: 'N', E: 'W', W: 'E',
	NE: 'SW', SW: 'NE', NW: 'SE', SE: 'NW',
};

/** Get the exit text description (e.g., "Northwest Exit To Estate Path"). */
export function getNextExitText(state: NavState): string | null {
	const dir = getNextDirection(state);
	if (!dir || state.plannedRoute.length < 2) return null;

	const currentIdx = state.plannedRoute.indexOf(state.currentRoom!);
	if (currentIdx < 0) return null;

	const nextRoomId = state.plannedRoute[currentIdx + 1];
	const nextRoom = state.roomById.get(nextRoomId);
	if (!nextRoom) return null;

	const dirNames: Record<string, string> = {
		N: 'North', NE: 'Northeast', E: 'East', SE: 'Southeast',
		S: 'South', SW: 'Southwest', W: 'West', NW: 'Northwest', C: 'Secret',
	};

	return `${dirNames[dir] ?? dir} Exit — ${nextRoom.name}`;
}

/** Get contents for a specific room. */
export function getRoomContents(state: NavState, roomId: string): string[] {
	return state.roomById.get(roomId)?.contents ?? [];
}

/** Toggle a room as a manual target and recompute the route. */
export function toggleTargetRoom(state: NavState, roomId: string): NavState {
	const newState = { ...state };
	if (state.targetRooms.includes(roomId)) {
		newState.targetRooms = state.targetRooms.filter((id) => id !== roomId);
	} else {
		newState.targetRooms = [...state.targetRooms, roomId];
	}
	if (state.currentRoom) {
		newState.plannedRoute = computeRouteFrom(newState, state.currentRoom, state.strategy);
	} else {
		newState.plannedRoute = computeRoute(newState, state.strategy);
	}
	return newState;
}

/** Update the routing strategy and recompute the route. */
export function setStrategy(state: NavState, strategy: RouteStrategy): NavState {
	const newState = { ...state, strategy };
	newState.targetRooms = getTargetRooms(newState, strategy);
	if (state.currentRoom) {
		newState.plannedRoute = computeRouteFrom(newState, state.currentRoom, strategy);
	} else {
		newState.plannedRoute = computeRoute(newState, strategy);
	}
	return newState;
}
