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

	// Rooms opening a new section — the first room past each Aspirant's Trial.
	// Needed before the adjacency build, to keep the trial gate one-way (below).
	const sectionFirstRooms = new Set<string>();
	let previousWasTrial = false;
	for (const room of layout.rooms) {
		const isTrial = room.name.toLowerCase() === "aspirant's trial";
		if (previousWasTrial && !isTrial) sectionFirstRooms.add(room.id);
		previousWasTrial = isTrial;
	}

	// Build adjacency from exits. Normal exits are bidirectional, with two
	// one-way exceptions — both of them doors the game only opens from one side.
	//
	// SECRET PASSAGES ('C'): a hidden door openable from one side. Walking into
	// the destination room does not let you walk back out through it, so adding
	// the reverse edge invents a traversal the game does not allow — the router
	// would then hand the player a route that dead-ends.
	//
	// TRIAL GATES: once Izaro is beaten and the player steps into the next
	// section, they cannot walk back into the trial room. Without this, the
	// already-beaten trial behind the player counts as adjacent to the room they
	// are standing in — and since all three Izaro rooms share the name
	// "Aspirant's Trial", the RoomChanged handler resolves that name by
	// adjacency. On a zig-zag layout (where the x-coordinate fallback cannot
	// break the tie either) it goes ambiguous and drops the tracked position.
	//
	// LabCompass gates its reverse-edge pass on both conditions in one
	// expression (labyrinthdata.cpp:271).
	for (const room of layout.rooms) {
		for (const [direction, targetId] of Object.entries(room.exits)) {
			const neighbors = adjacency.get(room.id)!;
			if (!neighbors.includes(targetId)) neighbors.push(targetId);

			if (direction === 'C') continue;
			const isTrialGate =
				room.name.toLowerCase() === "aspirant's trial" && sectionFirstRooms.has(targetId);
			if (isTrialGate) continue;

			const reverseNeighbors = adjacency.get(targetId);
			if (reverseNeighbors && !reverseNeighbors.includes(room.id)) {
				reverseNeighbors.push(room.id);
			}
		}
	}

	// Detect sections: split at Aspirant's Trial rooms
	const sections = detectSections(layout.rooms, adjacency);

	// --- Golden door / golden key locking ---
	//
	// CORE RULE:
	//   When a section contains a golden door, the golden-key room MUST be part
	//   of the planned route — the key is needed to cross the door and reach
	//   the trial / treasure.
	//
	// EXCEPTION:
	//   When a secret passage (or any alternate adjacency) lets the player
	//   reach the section end without crossing the golden door at all, the
	//   key room is skipped and the bypass route is used instead.
	//
	// The mechanism below encodes this by marking the door room's FORWARD
	// edges (toward the trial) as "locked" until the key is collected.
	// `routeWithGoldenDoor` then:
	//   - builds an unlocked adjacency (door forward-edges removed),
	//   - tries to find a bypass (the exception); if one exists, uses it,
	//   - otherwise requires the key room as a mandatory waypoint.
	//
	// FORWARD DETECTION:
	//   A door neighbor is "forward" (past the door) iff its shortest path
	//   from the lab start is LONGER through the door than around it — i.e.
	//   it's topologically deeper in the lab than the door itself. We use
	//   BFS distance from the start room for this, NOT raw x-coordinate.
	//
	//   (Zig-zag layouts — like uber-2026-04-17 "mansion halls" — can place
	//   the entry room at a higher x than the door room. A pure x-coordinate
	//   heuristic mis-locks the entry edge, strands the key room behind the
	//   "locked" entry, and the router silently falls back to a plain cheapest
	//   path that waltzes through the door without picking up the key. The test
	//   `should route through key room on zig-zag layouts ...` guards this.)
	const startRoomId = layout.rooms[0]?.id;
	const distFromStart = startRoomId ? bfsDistances(adjacency, startRoomId) : new Map<string, number>();

	const lockedDoors: [string, string][] = [];
	for (const room of layout.rooms) {
		if (!room.contents.some((c) => c.toLowerCase().includes('golden-door'))) continue;

		const doorDist = distFromStart.get(room.id) ?? 0;
		const keyNeighbors = new Set<string>();
		for (const neighborId of adjacency.get(room.id) ?? []) {
			const neighbor = roomById.get(neighborId);
			if (neighbor?.contents.some((c) => c.toLowerCase().includes('golden-key'))) {
				keyNeighbors.add(neighborId);
			}
		}

		for (const neighborId of adjacency.get(room.id) ?? []) {
			if (keyNeighbors.has(neighborId)) continue;
			const neighborDist = distFromStart.get(neighborId);
			// Lock only forward edges — neighbor strictly deeper from start than door.
			if (neighborDist === undefined || neighborDist <= doorDist) continue;
			const pair: [string, string] = [room.id, neighborId].sort() as [string, string];
			if (!lockedDoors.some(([a, b]) => a === pair[0] && b === pair[1])) {
				lockedDoors.push(pair);
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

	// Rooms after the last trial are side rooms of the final section — absorb them.
	// Don't create a phantom section that routes back to the last trial.
	if (currentSection.length > 0 && sections.length > 0) {
		sections[sections.length - 1].roomIds.push(...currentSection);
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
			// Priority 3: first room (entering from plaza) — pick the earliest
			// matching room on the planned route to disambiguate duplicate names.
			else {
				const firstOnRoute = state.plannedRoute.find((id) => matchingRooms.includes(id));
				candidates = firstOnRoute ? [firstOnRoute] : matchingRooms;
			}

			// Priority 4: exclude current room — if we're already in a room with
			// this name, the event must be for a different room with the same name.
			if (candidates.length > 1 && state.currentRoom) {
				const filtered = candidates.filter((id) => id !== state.currentRoom);
				if (filtered.length > 0) candidates = filtered;
			}

			// Priority 5: prefer forward movement — if exactly one candidate has a
			// higher x than current room, pick it. Multiple forward candidates
			// remain ambiguous.
			if (candidates.length > 1) {
				const withX = candidates
					.map((id) => ({ id, x: parseFloat(state.roomById.get(id)?.x ?? '0') }))
					.sort((a, b) => b.x - a.x);
				// If current room exists, prefer rooms further forward than it
				if (state.currentRoom) {
					const currentX = parseFloat(state.roomById.get(state.currentRoom)?.x ?? '0');
					const forward = withX.filter((r) => r.x > currentX);
					if (forward.length === 1) candidates = [forward[0].id];
				}
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

		// Golden door handling: if route must cross a locked door, split into
		// pre-key (unlocked adjacency) and post-key (full adjacency) phases.
		const goldenRoute = routeWithGoldenDoor(state, section, start, end, sectionTargets);
		const sectionRoute = goldenRoute ?? bestRouteThroughTargets(
			state.adjacency,
			start,
			end,
			sectionTargets,
			state.roomById,
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

/**
 * Golden door routing rules:
 *
 *  1. The golden key room is ALWAYS a mandatory routing target when a golden
 *     door exists in the section — the key opens the door and gives access
 *     to the treasure chest at the end.
 *  2. Exception: if a secret passage (or other alternate path) reaches the
 *     section end without crossing the golden door, the key is skipped and
 *     the bypass route is used instead.
 *  3. When routing through the key, the path is split into two phases:
 *       Phase 1: start → key room (using unlocked adjacency only)
 *       Phase 2: key room → end   (using full adjacency, door now open)
 *  4. If the door room and key room are different, Phase 2 includes the door
 *     room as a mandatory waypoint so the player backtracks through it.
 *
 * GAME INVARIANT: a lab run contains AT MOST ONE golden door — not one per
 * section, one in total. Taking the first matching key room and door room and
 * hardcoding a single two-phase route is therefore complete, not a simplifying
 * assumption to be generalised later. (LabCompass solves N doors generically by
 * folding key state into its search; that generality is unreachable in practice.)
 *
 * RETURN CONTRACT — the two empty-ish returns mean opposite things:
 *   null → no locked door blocks this section; the caller routes normally.
 *   []   → a locked door DOES block it and no legal route exists. The caller
 *          must treat this as final. It previously returned null here too, which
 *          made the caller re-route on full adjacency — and full adjacency
 *          ignores `lockedDoors`, so the planner confidently marched the player
 *          through a golden door it knew was locked, with no key. Failing closed
 *          matches upstream, where an unreachable key simply means no search
 *          state ever reaches the trial (navigationdata.cpp:52-114).
 */
function routeWithGoldenDoor(
	state: NavState,
	section: LabSection,
	start: string,
	end: string,
	targets: string[],
): string[] | null {
	if (state.lockedDoors.length === 0) return null;

	// Find locked doors in this section. Include endRoom (trial) — a golden
	// door can connect directly to the trial, and endRoom is not in roomIds.
	const sectionSet = new Set([...section.roomIds, section.endRoom]);
	const sectionLocks = state.lockedDoors.filter(
		([a, b]) => sectionSet.has(a) && sectionSet.has(b),
	);
	if (sectionLocks.length === 0) return null;

	// Build adjacency without golden door edges
	const unlockedAdj = new Map<string, string[]>();
	for (const [roomId, neighbors] of state.adjacency) {
		unlockedAdj.set(roomId, neighbors.filter((n) => {
			const pair = [roomId, n].sort();
			return !sectionLocks.some(([a, b]) => a === pair[0] && b === pair[1]);
		}));
	}

	// Check if a route through all targets to the end exists WITHOUT crossing locked doors.
	// If so, use it directly — don't return null, because the caller would use full adjacency
	// which might find a "shorter" path through the locked door the player can't open.
	const bypassRoute = bestRouteThroughTargets(unlockedAdj, start, end, targets, state.roomById);
	if (bypassRoute.length > 0) {
		return bypassRoute;
	}

	// Route needs the locked door — find golden key room in this section
	let keyRoom: string | null = null;
	for (const roomId of section.roomIds) {
		const room = state.roomById.get(roomId);
		if (room?.contents.some((c) => c.toLowerCase().includes('golden-key'))) {
			keyRoom = roomId;
			break;
		}
	}

	// No key room in a section a locked door blocks. There is no legal route, so
	// FAIL CLOSED — see the return-contract note in this function's doc comment.
	if (!keyRoom) return [];

	// Phase 1: start → key room, using unlocked adjacency (can't cross door yet).
	// Visit any targets reachable before the door.
	const preKeyTargets = targets.filter((t) => {
		const path = cheapestPath(unlockedAdj, start, t, state.roomById);
		return path.length > 0;
	});
	const keyTargets = preKeyTargets.includes(keyRoom) ? preKeyTargets : [...preKeyTargets, keyRoom];
	const phase1 = bestRouteThroughTargets(unlockedAdj, start, keyRoom, keyTargets.filter(t => t !== keyRoom), state.roomById);

	// Phase 2: key room → end, using full adjacency (door now open).
	// Visit remaining targets. If key room != door room, the player must
	// backtrack through the door room to proceed — add it as a mandatory target.
	const visitedInPhase1 = new Set(phase1);
	const postKeyTargets = targets.filter((t) => !visitedInPhase1.has(t));

	// Find the door room — the room with golden-door content.
	// It appears in all locked pairs. When door and key are in the same room, no backtracking needed.
	let doorRoom: string | null = null;
	for (const roomId of section.roomIds) {
		const room = state.roomById.get(roomId);
		if (room?.contents.some((c) => c.toLowerCase().includes('golden-door'))) {
			doorRoom = roomId;
			break;
		}
	}
	if (doorRoom && doorRoom !== keyRoom && !postKeyTargets.includes(doorRoom)) {
		postKeyTargets.push(doorRoom);
	}

	const phase2 = bestRouteThroughTargets(state.adjacency, keyRoom, end, postKeyTargets, state.roomById);

	// The key is unreachable, or the trial is unreachable even with it. Same as
	// above: no legal route exists, so fail closed rather than let the caller
	// route on full adjacency and march the player through the locked door.
	if (phase1.length === 0 || phase2.length === 0) return [];

	// Merge phases (avoid duplicate key room at boundary)
	return [...phase1, ...phase2.slice(1)];
}

/** Room shape the router needs: name drives cost, contents drive the tiebreak. */
type RoutingRoom = { name: string; contents: string[] };

// --- Room traversal cost ---
//
// Ported from LabCompass (labyrinthdata.cpp:8-26, roomCost() at :172-178).
// "Shortest" means fastest to walk, NOT fewest rooms: a Domain Atrium is a long
// trek and a Sepulchre Path is a few steps, so counting hops equally understates
// the cost of big rooms. Cost is charged on ENTERING a room, so the room the
// player already stands in is free — matching upstream's `length += roomCost(dest)`.
const ROOM_PREFIX_COST: Record<string, number> = {
	Sepulchre: 3, Estate: 5, Basilica: 7, Sanitorium: 8, Mansion: 9, Domain: 10,
};
const ROOM_SUFFIX_COST: Record<string, number> = {
	Path: 6, Passage: 6, Walkways: 8, Halls: 8, Annex: 10, Enclosure: 10, Crossing: 12, Atrium: 12,
};
const TRIAL_ROOM_COST = 5;

// Fallback for a name absent from the tables above (new league room types, or a
// malformed import). Deliberately mid-range rather than 0: a free unknown room
// would ATTRACT every route through it, which is the worse failure. Upstream's
// QHash returns 0 here — we diverge on purpose.
const UNKNOWN_ROOM_COST = 16;

/** Traversal cost of entering a room. */
export function roomCost(room: RoutingRoom | undefined): number {
	if (!room) return UNKNOWN_ROOM_COST;
	const name = room.name.trim();
	if (name.toLowerCase() === "aspirant's trial") return TRIAL_ROOM_COST;
	const [prefix, suffix] = name.split(/\s+/);
	const prefixCost = ROOM_PREFIX_COST[prefix];
	const suffixCost = ROOM_SUFFIX_COST[suffix];
	if (prefixCost === undefined || suffixCost === undefined) return UNKNOWN_ROOM_COST;
	return prefixCost + suffixCost;
}

/**
 * Total cost of walking a route. The starting room is already occupied, so it is
 * free — matching upstream's `length += roomCost(destination)`.
 *
 * Exported for tests only. Every candidate route in a comparison shares the same
 * start room, so charging for it would add the same constant to all of them and
 * could never change which one wins — the invariant has no observable effect on
 * routing, and this is the only seam that can pin it.
 */
export function routeCost(route: string[], roomById: Map<string, RoutingRoom>): number {
	let total = 0;
	for (const roomId of route.slice(1)) total += roomCost(roomById.get(roomId));
	return total;
}

function bestRouteThroughTargets(
	adjacency: Map<string, string[]>,
	start: string,
	end: string,
	targets: string[],
	roomById: Map<string, RoutingRoom>,
): string[] {
	// Drop targets the player cannot actually walk to. Every permutation
	// containing an unreachable target is invalid, so without this filter a
	// single stranded target (e.g. a darkshrine whose only link is a one-way
	// secret passage into it) discards EVERY candidate and the section route
	// collapses to empty — the planner then shows no route at all rather than
	// the route minus the room it can't reach.
	const reachable = targets.filter((t) => cheapestPath(adjacency, start, t, roomById).length > 0);

	if (reachable.length === 0) {
		return cheapestPath(adjacency, start, end, roomById);
	}

	// For small target sets (typically 1-4 per section), try all permutations
	const perms = permutations(reachable);
	const candidates: string[][] = [];

	for (const perm of perms) {
		const waypoints = [start, ...perm, end];
		let route: string[] = [];
		let valid = true;

		for (let i = 0; i < waypoints.length - 1; i++) {
			const segment = cheapestPath(adjacency, waypoints[i], waypoints[i + 1], roomById);
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

		if (!valid) continue;
		candidates.push(route);
	}

	// Last resort: a reachable target that still can't be sequenced into a route
	// to `end` must not cost the player the route to the trial.
	if (candidates.length === 0) return cheapestPath(adjacency, start, end, roomById);

	return pickCheapestPreferringDarkshrines(candidates, roomById);
}

/**
 * THE tiebreaker chokepoint. Every content-based route preference goes through here.
 *
 * It filters candidates to the MINIMUM cost FIRST, then prefers the one passing
 * through the most darkshrines. Cost always wins; content only ever breaks an
 * exact tie. A caller that hands in mixed-cost candidates therefore cannot cause
 * a detour — the pricier ones are discarded before any score is read.
 *
 * This is deliberately defensive: "shortest strategy points around" has regressed
 * twice, both times because a caller leaked a longer path into a content
 * comparison. Keeping the cost filter here rather than in each caller means a new
 * caller cannot reintroduce it.
 */
function pickCheapestPreferringDarkshrines(
	candidates: string[][],
	roomById: Map<string, RoutingRoom>,
): string[] {
	if (candidates.length === 0) return [];

	const costs = candidates.map((c) => routeCost(c, roomById));
	const minCost = Math.min(...costs);
	const cheapest = candidates.filter((_, i) => costs[i] === minCost);
	if (cheapest.length === 1) return cheapest[0];

	let best = cheapest[0];
	let bestScore = routeContentScore(best, roomById);
	for (const candidate of cheapest.slice(1)) {
		const score = routeContentScore(candidate, roomById);
		if (score > bestScore) {
			best = candidate;
			bestScore = score;
		}
	}
	return best;
}

/** Score a route by darkshrines it passes through. Only darkshrines matter for tiebreaking. */
function routeContentScore(route: string[], roomById: Map<string, RoutingRoom>): number {
	let score = 0;
	for (const roomId of route) {
		const room = roomById.get(roomId);
		if (!room) continue;
		if (room.contents.some((c) => c.toLowerCase().includes('darkshrine'))) {
			score += 1;
		}
	}
	return score;
}

/**
 * BFS from `start`, returning shortest-HOP distance to every reachable room.
 *
 * Intentionally unweighted: its only caller measures topological depth to decide
 * which side of a golden door is "forward". That is a graph-structure question,
 * so room traversal cost is irrelevant and would distort it.
 */
function bfsDistances(adjacency: Map<string, string[]>, start: string): Map<string, number> {
	const dist = new Map<string, number>([[start, 0]]);
	const queue: string[] = [start];
	while (queue.length > 0) {
		const current = queue.shift()!;
		const currentDist = dist.get(current)!;
		for (const neighbor of adjacency.get(current) ?? []) {
			if (dist.has(neighbor)) continue;
			dist.set(neighbor, currentDist + 1);
			queue.push(neighbor);
		}
	}
	return dist;
}

/**
 * Cheapest path from `start` to `end` by summed room cost (uniform-cost search).
 *
 * Ties are broken toward darkshrines, and that preference is expressed IN the
 * search comparator rather than by post-filtering finished paths: pop order is
 * (cost ascending, darkshrine count descending), so the first arrival at a room
 * is already the best-scoring of its cheapest prefixes. A strictly cheaper path
 * always outranks a darkshrine-richer one, which is what makes a content-driven
 * detour structurally impossible here.
 *
 * Returns [] when `end` is unreachable.
 */
function cheapestPath(
	adjacency: Map<string, string[]>,
	start: string,
	end: string,
	roomById: Map<string, RoutingRoom>,
): string[] {
	if (start === end) return [start];

	interface Reached { cost: number; score: number; path: string[] }
	const best = new Map<string, Reached>([[start, { cost: 0, score: 0, path: [start] }]]);
	const settled = new Set<string>();
	const frontier: string[] = [start];

	while (frontier.length > 0) {
		// Lab sections are tiny (tens of rooms), so a linear scan for the minimum
		// beats the complexity of a real heap.
		let pickIdx = 0;
		for (let i = 1; i < frontier.length; i++) {
			const a = best.get(frontier[i])!;
			const b = best.get(frontier[pickIdx])!;
			if (a.cost < b.cost || (a.cost === b.cost && a.score > b.score)) pickIdx = i;
		}
		const current = frontier.splice(pickIdx, 1)[0];
		if (settled.has(current)) continue;
		settled.add(current);

		const reached = best.get(current)!;
		if (current === end) return reached.path;

		for (const neighbor of adjacency.get(current) ?? []) {
			if (settled.has(neighbor)) continue;
			const room = roomById.get(neighbor);
			const cost = reached.cost + roomCost(room);
			const score = reached.score
				+ (room?.contents.some((c) => c.toLowerCase().includes('darkshrine')) ? 1 : 0);
			const known = best.get(neighbor);
			// Cheaper wins outright; at equal cost, more darkshrines wins.
			if (known && !(cost < known.cost || (cost === known.cost && score > known.score))) continue;
			best.set(neighbor, { cost, score, path: [...reached.path, neighbor] });
			if (!known) frontier.push(neighbor);
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
