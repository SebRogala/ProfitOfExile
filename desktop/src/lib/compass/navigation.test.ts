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
});

describe('Route strategy', () => {
	const simpleLayout = makeLayout([
		{ id: 'r1', name: 'Estate Walkways', x: '0', exits: { E: 'r2', SE: 'r3' } },
		{ id: 'r2', name: 'Domain Crossing', x: '1', exits: { W: 'r1', E: 'trial' }, contents: ['darkshrine'] },
		{ id: 'r3', name: 'Basilica Halls', x: '1', exits: { NW: 'r1', E: 'trial' } },
		{ id: 'trial', name: "Aspirant's Trial", x: '2', exits: { W: 'r2', W2: 'r3' } },
	]);

	it('should preserve strategy when reloading layout', () => {
		let state = loadLayout(createNavState(), simpleLayout);
		state = setStrategy(state, 'darkshrines');
		// Reload layout — strategy must survive
		state = loadLayout(state, simpleLayout);
		expect(state.strategy).toBe('darkshrines');
	});
});
