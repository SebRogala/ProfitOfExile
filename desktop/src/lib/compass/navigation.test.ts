import { describe, it, expect } from 'vitest';
import {
	createNavState,
	loadLayout,
	handleNavEvent,
	setStrategy,
	roomCost,
	toggleTargetRoom,
	isRoomNamePriced,
	routeCost,
	type LabLayout,
	type RouteStrategy,
	type NavState,
} from './navigation';

/** Build a minimal lab layout for testing. */
function makeLayout(rooms: { id: string; name: string; x: string; exits: Record<string, string>; contents?: string[] }[]): LabLayout {
	return {
		difficulty: 'Uber',
		date: '2026-04-10',
		weapon: 'Sword',
		phase1: '',
		phase2: '',
		trap1: '',
		trap2: '',
		rooms: rooms.map((r) => ({
			...r,
			y: '0',
			areacode: '',
			contents: r.contents ?? [],
		})),
	};
}

/** Helper: process a sequence of events starting from a loaded layout. */
function processEvents(
	layout: LabLayout,
	events: { type: string; name?: string }[],
	strategy?: RouteStrategy,
): NavState {
	let state = loadLayout(createNavState(), layout);
	if (strategy) state = setStrategy(state, strategy);
	for (const event of events) {
		state = handleNavEvent(state, event as any);
	}
	return state;
}

describe('RoomChanged disambiguation', () => {
	// Layout with two rooms named "Basilica Halls" — a common lab pattern.
	//
	//   Plaza → Room1(Basilica Halls, x=1) → Room2(Basilica Halls, x=2) → Trial
	//
	const duplicateNameLayout = makeLayout([
		{ id: 'plaza', name: "Aspirant's Trial", x: '0', exits: { E: 'room1' } },
		{ id: 'room1', name: 'Basilica Halls', x: '1', exits: { W: 'plaza', E: 'room2' } },
		{ id: 'room2', name: 'Basilica Halls', x: '2', exits: { W: 'room1', E: 'trial' } },
		{ id: 'trial', name: "Aspirant's Trial", x: '3', exits: { W: 'room2' } },
	]);

	it('should resolve first entry to the room connected to start', () => {
		const state = processEvents(duplicateNameLayout, [
			{ type: 'PlazaEntered' },
			{ type: 'RoomChanged', name: 'Basilica Halls' },
		]);
		// From plaza, only room1 is connected
		expect(state.currentRoom).toBe('room1');
	});

	it('should resolve consecutive same-named rooms by excluding current room', () => {
		const state = processEvents(duplicateNameLayout, [
			{ type: 'PlazaEntered' },
			{ type: 'RoomChanged', name: 'Basilica Halls' },  // → room1
			{ type: 'RoomChanged', name: 'Basilica Halls' },  // → room2 (not room1 again)
		]);
		expect(state.currentRoom).toBe('room2');
	});

	it('should not lose track when same name appears consecutively', () => {
		const state = processEvents(duplicateNameLayout, [
			{ type: 'PlazaEntered' },
			{ type: 'RoomChanged', name: 'Basilica Halls' },
			{ type: 'RoomChanged', name: 'Basilica Halls' },
		]);
		// currentRoom must not be null — that was the bug
		expect(state.currentRoom).not.toBeNull();
	});

	// Layout with duplicate names that are NOT adjacent (branching).
	//
	//   Plaza → A(Estate Walkways, x=1) → B(Domain Crossing, x=2) → C(Estate Walkways, x=3) → Trial
	//
	const nonAdjacentDuplicateLayout = makeLayout([
		{ id: 'plaza', name: "Aspirant's Trial", x: '0', exits: { E: 'a' } },
		{ id: 'a', name: 'Estate Walkways', x: '1', exits: { W: 'plaza', E: 'b' } },
		{ id: 'b', name: 'Domain Crossing', x: '2', exits: { W: 'a', E: 'c' } },
		{ id: 'c', name: 'Estate Walkways', x: '3', exits: { W: 'b', E: 'trial' } },
		{ id: 'trial', name: "Aspirant's Trial", x: '4', exits: { W: 'c' } },
	]);

	it('should resolve non-adjacent duplicates via adjacency', () => {
		const state = processEvents(nonAdjacentDuplicateLayout, [
			{ type: 'PlazaEntered' },
			{ type: 'RoomChanged', name: 'Estate Walkways' },  // → a (connected to plaza)
			{ type: 'RoomChanged', name: 'Domain Crossing' },  // → b
			{ type: 'RoomChanged', name: 'Estate Walkways' },  // → c (connected to b, not a)
		]);
		expect(state.currentRoom).toBe('c');
	});
});

describe('Golden door routing', () => {
	// Layout where the golden door connects directly to the trial room.
	// The key room is a dead-end branch from the door room.
	//
	//   r1 ─── door(golden-door) ─── trial
	//             │
	//           key(golden-key)
	//
	const goldenDoorToTrialLayout = makeLayout([
		{ id: 'r1', name: 'Estate Walkways', x: '0', exits: { E: 'door' } },
		{ id: 'door', name: 'Mansion Atrium', x: '1', exits: { N: 'key', E: 'trial' }, contents: ['golden-door'] },
		{ id: 'key', name: 'Basilica Annex', x: '1.5', exits: {}, contents: ['golden-key'] },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should route through golden key room when golden door connects to trial', () => {
		const state = loadLayout(createNavState(), goldenDoorToTrialLayout);
		// Route must visit the key room before reaching the trial
		expect(state.plannedRoute).toContain('key');
		const keyIdx = state.plannedRoute.indexOf('key');
		const trialIdx = state.plannedRoute.indexOf('trial');
		expect(keyIdx).toBeLessThan(trialIdx);
	});

	it('should backtrack through door room after picking up golden key', () => {
		const state = loadLayout(createNavState(), goldenDoorToTrialLayout);
		// Route: r1 → door → key → door → trial
		expect(state.plannedRoute).toEqual(['r1', 'door', 'key', 'door', 'trial']);
	});

	// Layout where a secret passage bypasses the golden door entirely.
	//
	//   r1 ─── door(golden-door) ─── trial
	//    │         │                   │
	//    └── shortcut ─────────────────┘
	//             key(golden-key)
	//
	const goldenDoorWithBypassLayout = makeLayout([
		{ id: 'r1', name: 'Estate Walkways', x: '0', exits: { E: 'door', SE: 'shortcut' } },
		{ id: 'door', name: 'Mansion Atrium', x: '1', exits: { N: 'key', E: 'trial' }, contents: ['golden-door'] },
		{ id: 'key', name: 'Basilica Annex', x: '1.5', exits: {}, contents: ['golden-key'] },
		{ id: 'shortcut', name: 'Secret Passage', x: '1', exits: { E: 'trial' } },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should skip golden key when a bypass route exists', () => {
		const state = loadLayout(createNavState(), goldenDoorWithBypassLayout);
		// Route should use the shortcut and skip the key room
		expect(state.plannedRoute).not.toContain('key');
		expect(state.plannedRoute).toContain('shortcut');
	});

	// Regression: uber-2026-04-17 "mansion halls" topology.
	//
	// Real-world zig-zag lab where the entry room to the door has a HIGHER
	// x-coordinate than the door room itself, because the lab path loops
	// north then comes back south. Under the old x-coordinate heuristic this
	// caused the entry edge to be mis-locked and the key room to be skipped
	// from the route, leaving the golden door impassable.
	//
	//         r2 (x=175, north)
	//        ╱    ╲
	//       ╱      ╲ SE
	//      ╱ NE     ╲
	//   r1(x=38) ─ r3[door, x=140] ─NE─ trial(x=244)
	//                │ C
	//                │
	//              r4[key, x=72]   (dead-end branch)
	//
	// Rule being enforced: when a golden door blocks the section and no
	// bypass exists, the golden-key room MUST appear in the planned route.
	const zigzagGoldenDoorLayout = makeLayout([
		{ id: 'r1', name: 'Estate Path', x: '38', exits: { NE: 'r2' } },
		{ id: 'r2', name: 'Basilica Passage', x: '175', exits: { SE: 'r3' } },
		{
			id: 'r3',
			name: 'Mansion Halls',
			x: '140',
			exits: { C: 'r4', NE: 'trial' },
			contents: ['golden-door'],
		},
		{ id: 'r4', name: 'Sepulchre Annex', x: '72', exits: { C: 'r3' }, contents: ['golden-key'] },
		{ id: 'trial', name: "Aspirant's Trial", x: '244', exits: {} },
	]);

	// The key room's only edge is a secret passage OUT of it, so nothing can
	// enter it and the golden door can never be opened. There is no legal route.
	//
	// The one answer that must never be produced is a route through the locked
	// door: the planner would be sending the player somewhere they cannot go,
	// while `lockedDoors` still records the door as shut.
	const unreachableKeyLayout = makeLayout([
		{ id: 'r1', name: 'Estate Path', x: '0', exits: { E: 'door' } },
		{ id: 'door', name: 'Mansion Atrium', x: '1', exits: { E: 'trial' }, contents: ['golden-door'] },
		{ id: 'key', name: 'Basilica Annex', x: '0.5', exits: { C: 'door' }, contents: ['golden-key'] },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should refuse to route through a locked door when the key is unreachable', () => {
		const state = loadLayout(createNavState(), unreachableKeyLayout);
		expect(state.lockedDoors).toEqual([['door', 'trial']]);
		expect(state.plannedRoute).toEqual([]);
	});

	// A golden door with no golden key anywhere in the section — malformed data.
	// Fails closed for the same reason: crossing it is not an option we may offer.
	const missingKeyLayout = makeLayout([
		{ id: 'r1', name: 'Estate Path', x: '0', exits: { E: 'door' } },
		{ id: 'door', name: 'Mansion Atrium', x: '1', exits: { E: 'trial' }, contents: ['golden-door'] },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should refuse to route through a locked door when the section has no key', () => {
		const state = loadLayout(createNavState(), missingKeyLayout);
		expect(state.plannedRoute).toEqual([]);
	});

	// A darkshrine target sits BEHIND the golden door, and a bypass to the trial
	// exists. The bypass is shorter, but taking it abandons the target — and the
	// key that would have opened the way to it. Wanting the vault means the door
	// route wins despite the bypass.
	//
	//   s ─── door[golden-door] ─── vault[darkshrine]
	//   ├── key[golden-key]
	//   └── byp ─── trial
	const targetBehindDoorLayout = makeLayout([
		{ id: 's', name: 'Estate Path', x: '0', exits: { E: 'door', NE: 'key', SE: 'byp' } },
		{ id: 'key', name: 'Sepulchre Path', x: '0.5', exits: {}, contents: ['golden-key'] },
		{ id: 'door', name: 'Estate Path', x: '1', exits: { E: 'vault' }, contents: ['golden-door'] },
		{ id: 'vault', name: 'Sepulchre Path', x: '2', exits: {}, contents: ['darkshrine'] },
		{ id: 'byp', name: 'Sepulchre Path', x: '1', exits: { NE: 'trial' } },
		{ id: 'trial', name: "Aspirant's Trial", x: '3', exits: {} },
	]);

	it('should not take a bypass that abandons a target behind the golden door', () => {
		let state = loadLayout(createNavState(), targetBehindDoorLayout);
		state = setStrategy(state, 'darkshrines');
		expect(state.plannedRoute).toContain('vault');
		expect(state.plannedRoute).toContain('key');
	});

	// `pocket` is a darkshrine you can walk into but never walk out of toward the
	// trial. It must be dropped ALONE — the reachable darkshrine `far` stays on
	// the route rather than being discarded along with it.
	const unsequenceableTargetLayout = makeLayout([
		{ id: 's', name: 'Estate Path', x: '0', exits: { E: 'd', SE: 'far', C: 'pocket' } },
		{ id: 'd', name: 'Sepulchre Path', x: '1', exits: { E: 'trial' } },
		{ id: 'far', name: 'Domain Atrium', x: '1', exits: { NE: 'trial' }, contents: ['darkshrine'] },
		{ id: 'pocket', name: 'Sepulchre Path', x: '0.5', exits: {}, contents: ['darkshrine'] },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should drop only the unusable target, keeping the reachable ones', () => {
		let state = loadLayout(createNavState(), unsequenceableTargetLayout);
		state = setStrategy(state, 'darkshrines');
		expect(state.plannedRoute).toContain('far');
		expect(state.plannedRoute).not.toContain('pocket');
	});

	// Walking through a golden door consumes the key and unlocks that door. The
	// pair crossed is (room we just left, room we entered) — tracked from
	// currentRoom, not previousRoom, which is already a room further back.
	it('should unlock the door the player actually walked through', () => {
		const state = processEvents(goldenDoorToTrialLayout, [
			{ type: 'PlazaEntered' },
			{ type: 'RoomChanged', name: 'Estate Walkways' },  // r1
			{ type: 'RoomChanged', name: 'Mansion Atrium' },   // door
			{ type: 'RoomChanged', name: 'Basilica Annex' },   // key — pick it up
			{ type: 'RoomChanged', name: 'Mansion Atrium' },   // back to door
			{ type: 'RoomChanged', name: "Aspirant's Trial" }, // cross the door
		]);
		expect(state.currentRoom).toBe('trial');
		expect(state.lockedDoors).toEqual([]);
		expect(state.goldenKeys).toBe(0);
	});

	// The golden KEY and golden DOOR can occupy the SAME room. Rare, but real:
	// it occurs twice in the vendored poelab layouts (2018-01-09_merciless room
	// 6, and 2018-01-10_merciless room 1, which is also the lab entrance).
	// No backtrack waypoint is needed — you pick the key up standing in the
	// doorway. That falls out of the structure, not a special case: phase 2
	// already starts at this room, so naming it a waypoint is the identity.
	const sameRoomKeyAndDoorLayout = makeLayout([
		{ id: 'r1', name: 'Estate Path', x: '0', exits: { E: 'kd' } },
		{
			id: 'kd',
			name: 'Mansion Atrium',
			x: '1',
			exits: { E: 'trial' },
			contents: ['golden-key', 'golden-door'],
		},
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should route straight through a room holding both the key and the door', () => {
		const state = loadLayout(createNavState(), sameRoomKeyAndDoorLayout);
		expect(state.plannedRoute).toEqual(['r1', 'kd', 'trial']);
	});

	// Same, but the entrance itself holds both — the 2018-01-10_merciless shape.
	const entranceKeyAndDoorLayout = makeLayout([
		{
			id: 'kd',
			name: 'Mansion Atrium',
			x: '0',
			exits: { E: 'mid' },
			contents: ['golden-key', 'golden-door'],
		},
		{ id: 'mid', name: 'Estate Path', x: '1', exits: { E: 'trial' } },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should route from an entrance that holds both the key and the door', () => {
		const state = loadLayout(createNavState(), entranceKeyAndDoorLayout);
		expect(state.plannedRoute).toEqual(['kd', 'mid', 'trial']);
	});

	it('should route through key room on zig-zag layouts where entry x > door x', () => {
		const state = loadLayout(createNavState(), zigzagGoldenDoorLayout);
		// Rule: key room MUST be visited when the door blocks the trial.
		expect(state.plannedRoute).toContain('r4');
		// Player must backtrack through the door room after grabbing the key.
		const keyIdx = state.plannedRoute.indexOf('r4');
		const trialIdx = state.plannedRoute.indexOf('trial');
		expect(keyIdx).toBeLessThan(trialIdx);
		// And trial is still reached at the end.
		expect(state.plannedRoute[state.plannedRoute.length - 1]).toBe('trial');
	});
});

describe('Route strategy', () => {
	const simpleLayout = makeLayout([
		{ id: 'r1', name: 'Estate Walkways', x: '0', exits: { E: 'r2', SE: 'r3' } },
		{ id: 'r2', name: 'Domain Crossing', x: '1', exits: { W: 'r1', E: 'trial' }, contents: ['darkshrine'] },
		{ id: 'r3', name: 'Basilica Halls', x: '1', exits: { NW: 'r1', E: 'trial' } },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: { W: 'r2', W2: 'r3' } },
	]);

	// Regression: "Shortest" strategy visibly detoured through content rooms.
	//
	// r1 has a direct edge to the trial (cost 5), plus a branch through a
	// darkshrine room (cost 27). The darkshrine tiebreaker may only choose
	// between routes of EQUAL cost — this branch is strictly more expensive, so
	// it must be discarded before content is even considered.
	//
	//   r1 ─────────────── trial
	//    └── r2[darkshrine] ──┘
	const darkshrineDetourLayout = makeLayout([
		{ id: 'r1', name: 'Estate Walkways', x: '0', exits: { E: 'trial', SE: 'r2' } },
		{ id: 'r2', name: 'Domain Crossing', x: '1', exits: { NE: 'trial' }, contents: ['darkshrine'] },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should take the direct edge under shortest even when a darkshrine detour exists', () => {
		const state = loadLayout(createNavState(), darkshrineDetourLayout);
		expect(state.plannedRoute).toEqual(['r1', 'trial']);
	});

	it('should still visit the darkshrine under the darkshrines strategy', () => {
		let state = loadLayout(createNavState(), darkshrineDetourLayout);
		state = setStrategy(state, 'darkshrines');
		expect(state.plannedRoute).toEqual(['r1', 'r2', 'trial']);
	});

	// Two branches of EQUAL cost (Estate Path and Estate Passage both cost 11),
	// only one holding a darkshrine. The non-darkshrine branch is listed first in
	// `exits` so the search reaches it first — picking the first arrival would
	// yield r3, so this fails if the tiebreaker is removed rather than reordered.
	//
	//   r1 ─── r3 ─────────── trial
	//    └── r2[darkshrine] ────┘
	const equalCostTieLayout = makeLayout([
		{ id: 'r1', name: 'Estate Walkways', x: '0', exits: { E: 'r3', SE: 'r2' } },
		{ id: 'r3', name: 'Estate Path', x: '1', exits: { E: 'trial' } },
		{ id: 'r2', name: 'Estate Passage', x: '1', exits: { NE: 'trial' }, contents: ['darkshrine'] },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should prefer the darkshrine branch when both branches cost the same', () => {
		const state = loadLayout(createNavState(), equalCostTieLayout);
		expect(state.plannedRoute).toEqual(['r1', 'r2', 'trial']);
	});

	// Room traversal cost, not hop count, decides the route (LabCompass parity).
	//
	//   r1 ─── big[Domain Atrium, 22] ──────────── trial     2 hops, cost 27
	//    └──── s1[Sepulchre Path, 9] ─ s2[9] ───── trial     3 hops, cost 23
	const weightedCostLayout = makeLayout([
		{ id: 'r1', name: 'Estate Walkways', x: '0', exits: { E: 'big', SE: 's1' } },
		{ id: 'big', name: 'Domain Atrium', x: '1', exits: { E: 'trial' } },
		{ id: 's1', name: 'Sepulchre Path', x: '1', exits: { E: 's2' } },
		{ id: 's2', name: 'Sepulchre Path', x: '2', exits: { NE: 'trial' } },
		{ id: 'trial', name: "Aspirant's Trial", x: '3', exits: {} },
	]);

	it('should take more small rooms over fewer expensive ones', () => {
		const state = loadLayout(createNavState(), weightedCostLayout);
		expect(state.plannedRoute).toEqual(['r1', 's1', 's2', 'trial']);
	});

	// A room name absent from the cost tables must not be treated as free —
	// a zero-cost unknown would attract every route through it.
	//
	//   r1 ─── mystery[unknown name] ───── trial
	//    └──── cheap[Sepulchre Path, 9] ────┘
	const unknownRoomNameLayout = makeLayout([
		{ id: 'r1', name: 'Estate Walkways', x: '0', exits: { E: 'mystery', SE: 'cheap' } },
		{ id: 'mystery', name: 'Voidborn Terrace', x: '1', exits: { E: 'trial' } },
		{ id: 'cheap', name: 'Sepulchre Path', x: '1', exits: { NE: 'trial' } },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should not route through an unknown room name as if it were free', () => {
		const state = loadLayout(createNavState(), unknownRoomNameLayout);
		expect(state.plannedRoute).toEqual(['r1', 'cheap', 'trial']);
	});

	// Secret passages ('C' exits) are one-way — openable from one side only.
	// `shrine`'s ONLY connection is a secret passage OUT of it into r2, so the
	// player can never enter it. Treating that edge as bidirectional would make
	// the router send them r1 → r2 → shrine → r2 → trial, through a door that
	// does not open from r2's side.
	const oneWaySecretPassageLayout = makeLayout([
		{ id: 'r1', name: 'Estate Path', x: '0', exits: { E: 'r2' } },
		{ id: 'r2', name: 'Estate Path', x: '1', exits: { E: 'trial' } },
		{ id: 'shrine', name: 'Sepulchre Path', x: '1.5', exits: { C: 'r2' }, contents: ['darkshrine'] },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should not enter a room reachable only backwards through a secret passage', () => {
		let state = loadLayout(createNavState(), oneWaySecretPassageLayout);
		state = setStrategy(state, 'darkshrines');
		expect(state.plannedRoute).toEqual(['r1', 'r2', 'trial']);
	});

	// Once Izaro is beaten you cannot walk back into the trial room. All three
	// Izaro rooms share the name "Aspirant's Trial", so RoomChanged for that name
	// is resolved by adjacency to where the player stands — and if the beaten
	// trial behind them counts as adjacent, there are two candidates.
	//
	// This layout defeats both fallbacks that would otherwise rescue it: the
	// player walks straight into trial2 while the planned route points at the
	// darkshrine first (so the on-route shortcut does not apply), and trial2 sits
	// at a LOWER x than r2 (zig-zag), so "prefer forward" finds no single
	// candidate either. With a bidirectional trial gate the position is dropped.
	//
	//   r1 → trial1 → r2 ─── trial2 (x=1.5, behind r2)
	//                  └── shrine[darkshrine] ──┘
	const zigzagTrialGateLayout = makeLayout([
		{ id: 'r1', name: 'Estate Path', x: '0', exits: { E: 'trial1' } },
		{ id: 'trial1', name: "Aspirant's Trial", x: '1', exits: { E: 'r2' } },
		{ id: 'r2', name: 'Estate Path', x: '2', exits: { NE: 'shrine', E: 'trial2' } },
		{ id: 'shrine', name: 'Sepulchre Path', x: '2.5', exits: { SE: 'trial2' }, contents: ['darkshrine'] },
		{ id: 'trial2', name: "Aspirant's Trial", x: '1.5', exits: {} },
	]);

	it('should resolve the trial ahead rather than the beaten one behind', () => {
		const state = processEvents(zigzagTrialGateLayout, [
			{ type: 'PlazaEntered' },
			{ type: 'RoomChanged', name: 'Estate Path' },        // → r1
			{ type: 'RoomChanged', name: "Aspirant's Trial" },   // → trial1
			{ type: 'RoomChanged', name: 'Estate Path' },        // → r2
			{ type: 'RoomChanged', name: "Aspirant's Trial" },   // → trial2, not trial1
		], 'darkshrines');
		expect(state.currentRoom).toBe('trial2');
	});

	// Two darkshrine targets, so the router compares whole candidate ROUTES (one
	// per target ordering) rather than single paths. Visiting b then a forces a
	// re-crossing of b, which scores HIGHER (b counted twice) while costing more.
	// Cost must win: this is what pins the minimum-cost filter in the tiebreaker
	// chokepoint, the one place a pricier-but-shrine-richer route could sneak in.
	//
	//   r1 → a[darkshrine] → b[darkshrine] → trial
	const twoTargetOrderingLayout = makeLayout([
		{ id: 'r1', name: 'Estate Path', x: '0', exits: { E: 'a', SE: 'b' } },
		{ id: 'a', name: 'Sepulchre Path', x: '1', exits: { E: 'b' }, contents: ['darkshrine'] },
		{ id: 'b', name: 'Sepulchre Path', x: '2', exits: { E: 'trial' }, contents: ['darkshrine'] },
		{ id: 'trial', name: "Aspirant's Trial", x: '3', exits: {} },
	]);

	it('should not pick a pricier route just because it re-crosses a darkshrine', () => {
		let state = loadLayout(createNavState(), twoTargetOrderingLayout);
		state = setStrategy(state, 'darkshrines');
		expect(state.plannedRoute).toEqual(['r1', 'a', 'b', 'trial']);
	});

	it('should route to the new layout targets when a layout is loaded', () => {
		// Reproduces importing a poelab file while a content strategy is active:
		// loadLayout must route against THIS layout's targets, not the last one's.
		// The two layouts name their darkshrines differently on purpose — with
		// matching ids the stale targets coincide and the bug hides.
		let state = loadLayout(createNavState(), twoTargetOrderingLayout);
		state = setStrategy(state, 'darkshrines');
		expect(state.targetRooms).toEqual(['a', 'b']);

		state = loadLayout(state, darkshrineDetourLayout);
		expect(state.plannedRoute).toEqual(['r1', 'r2', 'trial']);
	});

	// A truncated or hand-edited export can point an exit at an id that is not in
	// `rooms`. Section detection must skip it rather than compute rooms[NaN].
	const danglingExitLayout = makeLayout([
		{ id: 'a', name: 'Estate Path', x: '0', exits: { E: 't1' } },
		{ id: 't1', name: "Aspirant's Trial", x: '1', exits: { NE: 'ghost', N: 'b' } },
		{ id: 'b', name: 'Estate Path', x: '2', exits: { E: 't2' } },
		{ id: 't2', name: "Aspirant's Trial", x: '3', exits: {} },
	]);

	it('should ignore a trial exit pointing at a room that does not exist', () => {
		const state = loadLayout(createNavState(), danglingExitLayout);
		expect(state.plannedRoute).toEqual(['a', 't1', 'b', 't2']);
	});

	// A trial exit pointing BACK up the array cannot open a section — sections are
	// contiguous windows, so honouring it would carve a non-contiguous one and
	// swallow the following section.
	const backwardTrialExitLayout = makeLayout([
		{ id: 'a', name: 'Estate Path', x: '0', exits: { E: 't1' } },
		{ id: 't1', name: "Aspirant's Trial", x: '1', exits: { E: 'b' } },
		{ id: 'b', name: 'Estate Path', x: '2', exits: { E: 't2' } },
		{ id: 't2', name: "Aspirant's Trial", x: '3', exits: { W: 'b', NE: 'c' } },
		{ id: 'c', name: 'Estate Path', x: '4', exits: { E: 't3' } },
		{ id: 't3', name: "Aspirant's Trial", x: '5', exits: {} },
	]);

	it('should not open a section on a trial exit pointing backwards', () => {
		const state = loadLayout(createNavState(), backwardTrialExitLayout);
		expect(state.plannedRoute.at(-1)).toBe('t3');
	});

	// A trial with two forward exits has two doors into ONE section, not two
	// sections. Taking the later one misfiles the earlier room into the section
	// already beaten, where the trial sink puts it out of reach.
	const twoForwardExitsLayout = makeLayout([
		{ id: 'a', name: 'Estate Path', x: '0', exits: { E: 't1' } },
		{ id: 't1', name: "Aspirant's Trial", x: '1', exits: { NE: 'b', N: 'c' } },
		{ id: 'b', name: 'Estate Path', x: '2', exits: { E: 'c' }, contents: ['darkshrine'] },
		{ id: 'c', name: 'Estate Path', x: '3', exits: { E: 't2' } },
		{ id: 't2', name: "Aspirant's Trial", x: '4', exits: {} },
	]);

	it('should open one section from the earliest of two forward trial exits', () => {
		let state = loadLayout(createNavState(), twoForwardExitsLayout);
		state = setStrategy(state, 'darkshrines');
		expect(state.plannedRoute).toEqual(['a', 't1', 'b', 'c', 't2']);
	});

	// Rooms listed after the final trial belong to the last section. If they are
	// not absorbed they land in no section, so they can never become targets.
	// `tail` is listed after the final trial AND that trial has a forward exit to
	// it, so it opens a window of its own — one containing no trial. Absorbing it
	// into the previous section is what keeps it eligible as a target; otherwise
	// it belongs to no section and can never be routed to. It is also reachable
	// from `b`, as real tail rooms are — reachable only through the trial would
	// make it unreachable anyway, since the trial is a sink.
	const tailAfterFinalTrialLayout = makeLayout([
		{ id: 'a', name: 'Estate Path', x: '0', exits: { E: 't1' } },
		{ id: 't1', name: "Aspirant's Trial", x: '1', exits: { E: 'b' } },
		{ id: 'b', name: 'Estate Path', x: '2', exits: { E: 't2', NE: 'tail' } },
		{ id: 't2', name: "Aspirant's Trial", x: '3', exits: { NE: 'tail' } },
		{ id: 'tail', name: 'Sepulchre Path', x: '2.5', exits: {}, contents: ['darkshrine'] },
	]);

	it('should still target a darkshrine listed after the final trial', () => {
		let state = loadLayout(createNavState(), tailAfterFinalTrialLayout);
		state = setStrategy(state, 'darkshrines');
		expect(state.plannedRoute).toContain('tail');
		expect(state.plannedRoute.at(-1)).toBe('t2');
	});

	// Closes a gap I had written off as unconstructible: two candidate ROUTES of
	// equal cost but different darkshrine count, which is what the tiebreaker in
	// pickCheapestPreferringDarkshrines exists to decide. One-way secret passages
	// make it possible — the p→q leg and the q→p leg are different rooms, so the
	// ring is cost-symmetric but shrine-asymmetric.
	//
	// Targets are set manually: under the darkshrines strategy `x` would itself
	// become a target and collapse the case. permutations() yields ['p','q']
	// first, so the shrine-free route is the one a naive "take the first
	// cheapest" would return.
	const asymmetricRingLayout = makeLayout([
		{ id: 's', name: 'Estate Walkways', x: '0', exits: { E: 'p', SE: 'q' } },
		{ id: 'p', name: 'Sepulchre Path', x: '1', exits: { C: 'y', NE: 'trial' } },
		{ id: 'q', name: 'Sepulchre Path', x: '1', exits: { C: 'x', NE: 'trial' } },
		{ id: 'y', name: 'Sepulchre Path', x: '1.5', exits: { C: 'q' } },
		{ id: 'x', name: 'Sepulchre Path', x: '1.5', exits: { C: 'p' }, contents: ['darkshrine'] },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: {} },
	]);

	it('should prefer the target order whose connecting leg passes a darkshrine', () => {
		let state = loadLayout(createNavState(), asymmetricRingLayout);
		state = toggleTargetRoom(state, 'p');
		state = toggleTargetRoom(state, 'q');
		// p→y→q and q→x→p both cost 32; only the second passes the shrine.
		expect(state.plannedRoute).toEqual(['s', 'q', 'x', 'p', 'trial']);
	});

	it('should preserve strategy when reloading layout', () => {
		let state = loadLayout(createNavState(), simpleLayout);
		state = setStrategy(state, 'darkshrines');
		// Reload layout — strategy must survive
		state = loadLayout(state, simpleLayout);
		expect(state.strategy).toBe('darkshrines');
	});
});

describe('roomCost', () => {
	it.each([
		['Estate Path', 11],
		['Domain Atrium', 22],
		['Sepulchre Path', 9],
		["Aspirant's Trial", 5],
		['Voidborn Terrace', 16],  // neither affix known
		['Sepulchre', 16],         // one word — no suffix to price
		// poelab ships lowercase, Client.txt title-cases — both must price alike.
		['estate path', 11],
		['ESTATE PATH', 11],
		['Domain atrium', 22],
	])('should charge %s as %i', (name, expected) => {
		expect(roomCost({ name: name as string, contents: [] })).toBe(expected);
	});

	// The point of the unknown-room fallback is that it must never look like a
	// bargain. Pinning the exact constant alone would let 10 (cheaper than the
	// cheapest real room) pass; this states the property the fallback exists for.
	it('should charge an unknown room more than the cheapest known room', () => {
		expect(roomCost({ name: 'Voidborn Terrace', contents: [] }))
			.toBeGreaterThan(roomCost({ name: 'Sepulchre Path', contents: [] }));
	});

	it('should charge a missing room the unknown-room cost rather than nothing', () => {
		expect(roomCost(undefined)).toBe(16);
	});
});

describe('routeCost', () => {
	const rooms = new Map([
		['start', { name: 'Domain Atrium', contents: [] }],   // 22 — must NOT be charged
		['next', { name: 'Sepulchre Path', contents: [] }],   // 9
		['trial', { name: "Aspirant's Trial", contents: [] }], // 5
	]);

	it('should charge every room entered', () => {
		expect(routeCost(['start', 'next', 'trial'], rooms)).toBe(14);
	});

	it('should not charge the room the player already stands in', () => {
		expect(routeCost(['start'], rooms)).toBe(0);
	});
});

// Real poelab exports, vendored from LabCompass's test data (GPL-3.0). The
// planner feeds imported poelab JSON to loadLayout verbatim, so these are the
// only fixtures that exercise the shape production actually sees — including
// lowercase room names, which a hand-written TitleCase fixture cannot catch.
describe('real poelab layouts', () => {
	const STRATEGIES: RouteStrategy[] = ['shortest', 'darkshrines', 'darkshrines-argus', 'everything'];

	const layouts = import.meta.glob('./__fixtures__/poelab-*.json', {
		eager: true,
		import: 'default',
	}) as Record<string, LabLayout>;

	it('should load at least one vendored layout', () => {
		expect(Object.keys(layouts).length).toBeGreaterThan(0);
	});

	for (const [path, layout] of Object.entries(layouts)) {
		const name = path.split('/').pop();

		it(`should price every room in ${name} from the cost tables`, () => {
			// An unrecognised name falls back to UNKNOWN_ROOM_COST, which flattens
			// the cost model toward hop counting. Real layouts must never do that.
			// Asked via isRoomNamePriced, not `=== 16`: "sanitorium halls" really
			// does cost 16, so the number cannot distinguish the two.
			const unpriced = layout.rooms
				.filter((room) => !isRoomNamePriced(room.name))
				.map((room) => room.name);
			expect(unpriced).toEqual([]);
		});

		// A beaten Izaro cannot be re-entered. A route naming the same trial twice
		// is unwalkable — it was the visible symptom of sections being cut on
		// array order instead of on the trials' own exits.
		it(`should never re-enter a beaten trial in ${name}`, () => {
			const trialIds = new Set(
				layout.rooms.filter((r) => r.name.toLowerCase() === "aspirant's trial").map((r) => r.id),
			);
			for (const strategy of STRATEGIES) {
				let state = loadLayout(createNavState(), layout);
				state = setStrategy(state, strategy);
				const revisited = state.plannedRoute.filter(
					(id, i) => trialIds.has(id) && state.plannedRoute.indexOf(id) !== i,
				);
				expect(revisited, `${strategy}: ${state.plannedRoute.join('>')}`).toEqual([]);
			}
		});

		// Walk the route the way the player would and check the door rule holds:
		// crossing a locked pair requires a key picked up earlier on the route.
		it(`should never cross a golden door without a key in ${name}`, () => {
			for (const strategy of STRATEGIES) {
				let state = loadLayout(createNavState(), layout);
				state = setStrategy(state, strategy);

				const locked = state.lockedDoors.map(([a, b]) => `${a}|${b}`);
				let keys = 0;
				const crossings: string[] = [];

				const collected = new Set<string>();
				state.plannedRoute.forEach((id, i) => {
					const room = state.roomById.get(id);
					// Only the FIRST visit yields the key — a route that passes back
					// through the key room must not bank a phantom second one, which
					// would let a keyless crossing slip past this check.
					if (
						room?.contents.some((c) => c.toLowerCase().includes('golden-key')) &&
						!collected.has(id)
					) {
						collected.add(id);
						keys += 1;
					}
					if (i === 0) return;
					const pair = [state.plannedRoute[i - 1], id].sort().join('|');
					if (!locked.includes(pair)) return;
					if (keys < 1) crossings.push(pair);
					else keys -= 1;
				});

				expect(crossings, `${strategy}: ${state.plannedRoute.join('>')}`).toEqual([]);
			}
		});

		it.each(STRATEGIES)(`should reach the final trial in ${name} (%s)`, (strategy) => {
			let state = loadLayout(createNavState(), layout);
			state = setStrategy(state, strategy);
			const lastTrial = [...layout.rooms]
				.reverse()
				.find((r) => r.name.toLowerCase() === "aspirant's trial");
			// Also the guard that keeps the two invariants above honest: they
			// assert on an empty array, so an empty route would satisfy them.
			expect(state.plannedRoute.at(-1)).toBe(lastTrial!.id);
		});

		// The invariants above check the route as PLANNED. This walks it the way
		// the player does, re-planning after every room, because that is the state
		// the overlay is actually in — and it is where re-entry survived: standing
		// IN a trial matched no section (roomIds excludes trials), so the router
		// fell back to section 0 and pointed the player back through the Izaro
		// they had just beaten.
		it(`should never plan a route back into a trial already beaten in ${name}`, () => {
			let state = loadLayout(createNavState(), layout);
			state = handleNavEvent(state, { type: 'PlazaEntered' });

			const trialIds = new Set(
				layout.rooms.filter((r) => r.name.toLowerCase() === "aspirant's trial").map((r) => r.id),
			);
			const beaten = new Set<string>();
			const walked: string[] = [];

			for (const roomId of [...state.plannedRoute]) {
				const room = state.roomById.get(roomId)!;
				state = handleNavEvent(state, { type: 'RoomChanged', name: room.name });
				walked.push(state.currentRoom ?? '?');
				if (state.currentRoom && trialIds.has(state.currentRoom)) beaten.add(state.currentRoom);

				const replanned = state.plannedRoute.filter((id) => beaten.has(id) && id !== state.currentRoom);
				expect(replanned, `at ${walked.join('>')} | route ${state.plannedRoute.join('>')}`).toEqual([]);
			}
		});
	}
});
