import { describe, it, expect } from 'vitest';
import {
	createNavState,
	loadLayout,
	handleNavEvent,
	setStrategy,
	type LabLayout,
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
function processEvents(layout: LabLayout, events: { type: string; name?: string }[]): NavState {
	let state = loadLayout(createNavState(), layout);
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
	// r1 has a DIRECT 1-hop edge to the trial, plus a 2-hop branch through a
	// darkshrine room. The darkshrine tiebreaker is only ever allowed to choose
	// between paths of EQUAL hop count — here the branch is strictly longer, so
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

	it('should preserve strategy when reloading layout', () => {
		let state = loadLayout(createNavState(), simpleLayout);
		state = setStrategy(state, 'darkshrines');
		// Reload layout — strategy must survive
		state = loadLayout(state, simpleLayout);
		expect(state.strategy).toBe('darkshrines');
	});
});
