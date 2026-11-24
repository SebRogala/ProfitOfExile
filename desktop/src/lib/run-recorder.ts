/**
 * Headless lab-run recorder — measures every run and submits it to the server.
 *
 * Lives in the main window, which is open for the whole session. Measurement
 * used to run inside the timer overlay's webview, so runs were only recorded
 * while that overlay was enabled — with it toggled off, the Runs tab silently
 * collected nothing. The overlay is display-only now; this module is the one
 * place a run is measured and submitted.
 *
 * Elapsed time is computed from timestamps at event time, not from a ticking
 * interval: the main window can be minimized mid-run, and WebView2 throttles
 * background timers, but the timestamps are taken when the Tauri events arrive.
 */

import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import {
	createNavState,
	loadLayout,
	handleNavEvent,
	setStrategy,
	type NavEvent,
	type NavState,
} from '$lib/compass/navigation';
import { fetchLabLayout } from '$lib/compass/layout-loader';
import { getServerUrl } from '$lib/stores/server-base.svelte';

interface RoomLogEntry {
	room_name: string;
	entered_at: string;
	room_number: number;
}

let navState: NavState = createNavState();
let lockedDifficulty: string | null = null;
let pendingLayoutReset = false;
let layoutGeneration = 0;

let roomLog: RoomLogEntry[] = [];
let roomCounter = 0;
let runStartTimestamp: number | null = null;
let killSeconds = 0; // Izaro death time — snapshotted on LabFinished

let started = false;

function logToApp(msg: string) {
	invoke('app_log_from_frontend', { msg }).catch((e) => console.warn('[recorder] logToApp IPC failed:', msg, e));
}

function elapsedSeconds(timestamp = Date.now()): number {
	return runStartTimestamp === null ? 0 : Math.floor((timestamp - runStartTimestamp) / 1000);
}

function resetRun() {
	roomLog = [];
	roomCounter = 0;
	runStartTimestamp = null;
	killSeconds = 0;
}

function applyLayoutReset() {
	navState = createNavState();
	lockedDifficulty = null;
	pendingLayoutReset = false;
	logToApp('[recorder] layout reset applied (midnight UTC)');
	// Refetch immediately: in the overlays a `layoutLoaded` guard effect does
	// this, but a module has no effects, so without this call the recorder
	// stays layout-less until the next layout upload — and if the new day's
	// layout was uploaded before midnight, that event never comes. Runs
	// submitted with an empty navState all read has_golden_door=false.
	void fetchLayoutFromServer();
}

async function fetchLayoutFromServer(preferredDiff?: string) {
	const generation = ++layoutGeneration;
	if (preferredDiff) lockedDifficulty = preferredDiff;
	const selected = lockedDifficulty;
	const layout = await fetchLabLayout({
		preferredDifficulty: preferredDiff,
		lockedDifficulty,
		log: (msg) => logToApp(`[recorder] ${msg}`),
	});
	if (!layout || generation !== layoutGeneration) return;
	if ('missing' in layout) {
		if (selected) {
			navState = {
				...createNavState(),
				strategy: navState.strategy,
				inLab: navState.inLab,
				finished: navState.finished
			};
		}
		return;
	}
	navState = loadLayout(navState, layout);
	if (!lockedDifficulty) lockedDifficulty = layout.difficulty;
	logToApp(`[recorder] layout loaded: ${layout.difficulty} (${layout.rooms.length} rooms)`);
}

// Called on LabExited with the run complete. Kill time was snapshotted on
// LabFinished; total elapsed runs from the first room to lab exit.
async function submitRun(timestamp = Date.now()) {
	const totalElapsed = elapsedSeconds(timestamp);
	const snapshotRooms = [...roomLog];
	const snapshotKillTime = killSeconds;
	const snapshotStartedAt = runStartTimestamp;
	const snapshotDifficulty = lockedDifficulty;
	const snapshotNav = navState;
	if (snapshotKillTime <= 0 || snapshotRooms.length === 0) return;
	try {
		const serverUrl = getServerUrl();
		if (!serverUrl) { logToApp('[recorder] no server_url, cannot submit run'); return; }
		const status = await invoke<any>('get_status');

		const settings = await invoke<any>('get_compass_settings').catch(() => null);
		const visitedRoomNames = new Set(snapshotRooms.map(r => r.room_name.toLowerCase()));
		const hasGoldenDoor = snapshotNav.lockedDoors.length > 0 ||
			Array.from(snapshotNav.roomById.values())
				.filter(r => visitedRoomNames.has(r.name.toLowerCase()))
				.some(r => r.contents.some(c => c.toLowerCase().includes('golden-door')));

		const body = {
			difficulty: snapshotDifficulty ?? settings?.difficulty,
			strategy: settings?.strategy ?? snapshotNav.strategy,
			elapsed_seconds: totalElapsed,
			kill_seconds: snapshotKillTime,
			room_count: snapshotRooms.length,
			has_golden_door: hasGoldenDoor,
			started_at: snapshotStartedAt ? new Date(snapshotStartedAt).toISOString() : new Date().toISOString(),
			rooms: snapshotRooms,
		};

		const res = await fetch(`${serverUrl}/api/lab/runs`, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
				'X-Device-ID': status?.device_id ?? '',
				'X-App-Version': status?.app_version ?? '',
			},
			body: JSON.stringify(body),
		});

		if (res.ok) {
			const data = await res.json();
			logToApp(`[recorder] run submitted: id=${data.run_id}, kill=${snapshotKillTime}s, total=${totalElapsed}s`);
		} else {
			const errBody = await res.text().catch(() => '');
			logToApp(`[recorder] submit failed: ${res.status} ${errBody}`);
		}
	} catch (e) {
		logToApp(`[recorder] submit error: ${e}`);
	}
}

function onNavEvent(event: any, timestamp = Date.now()) {
	if (event.type === 'LayoutChanged') {
		fetchLayoutFromServer(event.difficulty);
		return;
	}
	if (event.type === 'StrategyChanged') {
		navState = setStrategy(navState, event.strategy);
		return;
	}
	navState = handleNavEvent(navState, event as NavEvent);

	switch (event.type) {
		case 'PlazaEntered':
			resetRun();
			break;
		case 'RoomChanged':
			if (navState.inLab && runStartTimestamp === null) {
				runStartTimestamp = timestamp;
			}
			roomCounter++;
			roomLog = [...roomLog, {
				room_name: event.name,
				entered_at: new Date(timestamp).toISOString(),
				room_number: roomCounter,
			}];
			break;
		case 'LabFinished':
			killSeconds = elapsedSeconds(timestamp);
			break;
		case 'LabExited':
			submitRun(timestamp);
			resetRun();
			if (pendingLayoutReset) {
				applyLayoutReset();
			}
			break;
	}
}

/**
 * Start the recorder: init from settings, replay catchup, subscribe to events.
 * Call once from the main-window layout — it never unmounts, so no cleanup.
 */
export function startRunRecorder(): void {
	if (started) return;
	started = true;
	let liveEvents: { event: NavEvent; timestamp: number }[] | null = [];

	(async () => {
		const generation = layoutGeneration;
		const settings = await invoke<any>('get_compass_settings').catch((e: any) => {
			logToApp(`[recorder] init settings failed: ${e}`);
			return null;
		});
		if (generation === layoutGeneration) {
			if (settings?.difficulty && !lockedDifficulty) lockedDifficulty = settings.difficulty;
			if (settings?.strategy) navState = setStrategy(navState, settings.strategy);
			await fetchLayoutFromServer();
		}
		// Catch up to current lab state (app may start mid-run)
		const catchup = await invoke<any>('get_lab_catchup').catch((e: any) => {
			logToApp(`[recorder] catchup failed: ${e}`);
			return null;
		});
		const events: NavEvent[] = catchup?.events ?? [];
		const buffered = liveEvents!;
		liveEvents = null;
		// Client.txt replay can include events already delivered by the listener.
		// Recover the prefix before the longest matching live sequence, then use
		// arrival timestamps for the live suffix so delayed IPC cannot zero a run.
		let prefixLength = events.length;
		let overlap = 0;
		for (let i = 0; i < events.length; i++) {
			let matched = 0;
			while (i + matched < events.length && matched < buffered.length) {
				const replay = events[i + matched];
				const live = buffered[matched].event;
				if (replay.type !== live.type ||
					(replay.type === 'RoomChanged' && live.type === 'RoomChanged' && replay.name !== live.name)) break;
				matched++;
			}
			if (matched > overlap) {
				overlap = matched;
				prefixLength = i;
			}
		}
		if (events.length) logToApp(`[recorder] replaying ${prefixLength} catchup events`);
		const recoveredAt = buffered[0]?.timestamp ?? Date.now();
		for (const event of events.slice(0, prefixLength)) onNavEvent(event, recoveredAt);
		for (const { event, timestamp } of buffered) onNavEvent(event, timestamp);
	})();

	listen<NavEvent>('lab-nav', (event) => {
		if (liveEvents && !['LayoutChanged', 'StrategyChanged'].includes(event.payload.type)) {
			liveEvents.push({ event: event.payload, timestamp: Date.now() });
		} else {
			onNavEvent(event.payload);
		}
	});
	listen<any>('lab-layout-updated', (event) => {
		if (event.payload?.action === 'reset') {
			if (navState.inLab) {
				// Mid-run at midnight — defer reset until lab exit
				pendingLayoutReset = true;
				logToApp('[recorder] layout reset deferred (in lab)');
			} else {
				applyLayoutReset();
			}
			return;
		}
		fetchLayoutFromServer(event.payload?.difficulty);
	});
}
