<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import { invoke } from '@tauri-apps/api/core';
	import {
		createNavState,
		loadLayout,
		handleNavEvent,
		setStrategy,
		type NavEvent,
		type LabLayout,
	} from '$lib/compass/navigation';
	import {
		createTimerState,
		formatTimer,
		startTimer as timerStart_fn,
		stopTimer as timerStop_fn,
		resetTimer as timerReset_fn,
	} from '$lib/compass/timer';

	// --- Navigation state (needed for room tracking, golden door detection, and route info) ---
	let navState = $state(createNavState());
	let lockedDifficulty = $state<string | null>(null);
	let layoutLoaded = $state(false);

	// --- Timer state ---
	let timer = $state(createTimerState());
	let elapsed = $state(0);
	let timerText = $derived(formatTimer(elapsed));
	let hidden = $state(true);

	// --- Room tracking for run submission ---
	let roomLog = $state<{ room_name: string; entered_at: string; room_number: number }[]>([]);
	let roomCounter = 0;

	function startTimer() { timer = timerStart_fn(timer, (e) => { elapsed = e; }); }
	function stopTimer() { timer = timerStop_fn(timer); }
	function resetTimer() { timer = timerReset_fn(timer); elapsed = 0; roomLog = []; roomCounter = 0; }

	function logToApp(msg: string) {
		invoke('app_log_from_frontend', { msg }).catch((e) => console.warn('[timer] logToApp IPC failed:', msg, e));
	}

	// --- Submit completed run to server ---
	async function submitRun() {
		if (elapsed <= 0) return;
		try {
			const status = await invoke<any>('get_status');
			const serverUrl = status?.server_url;
			if (!serverUrl) { logToApp('[timer] no server_url, cannot submit run'); return; }

			const settings = await invoke<any>('get_compass_settings').catch(() => null);
			// Check only visited rooms (from roomLog), not the entire layout
			const visitedRoomNames = new Set(roomLog.map(r => r.room_name.toLowerCase()));
			const hasGoldenDoor = navState.lockedDoors.length > 0 ||
				Array.from(navState.roomById.values())
					.filter(r => visitedRoomNames.has(r.name.toLowerCase()))
					.some(r => r.contents.some(c => c.toLowerCase().includes('golden-door')));

			const body = {
				difficulty: settings?.difficulty ?? lockedDifficulty ?? 'Uber',
				strategy: settings?.strategy ?? navState.strategy,
				elapsed_seconds: elapsed,
				room_count: navState.plannedRoute.length,
				has_golden_door: hasGoldenDoor,
				started_at: timer.startTimestamp ? new Date(timer.startTimestamp).toISOString() : new Date().toISOString(),
				rooms: roomLog,
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
				logToApp(`[timer] run submitted: id=${data.run_id}, elapsed=${elapsed}s`);
			} else {
				const errBody = await res.text().catch(() => '');
				logToApp(`[timer] submit failed: ${res.status} ${errBody}`);
			}
		} catch (e) {
			logToApp(`[timer] submit error: ${e}`);
		}
	}

	// --- Event handling ---
	function onNavEvent(event: any) {
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
				resetTimer();
				hidden = false;
				break;
			case 'RoomChanged': {
				if (navState.inLab && timer.startTimestamp === null) {
					startTimer();
				}
				// Log room for submission
				roomCounter++;
				roomLog = [...roomLog, {
					room_name: event.name,
					entered_at: new Date().toISOString(),
					room_number: roomCounter,
				}];
				// Hide during Izaro fights
				const room = navState.currentRoom ? navState.roomById.get(navState.currentRoom) : null;
				hidden = room?.name.toLowerCase() === "aspirant's trial";
				break;
			}
			case 'LabFinished':
				stopTimer();
				submitRun();
				break;
			case 'LabExited':
				resetTimer();
				hidden = true;
				break;
		}
	}

	// --- Layout fetching (needed for navigation, room_count, and golden door detection) ---
	let layoutRetries = 0;
	async function fetchLayoutFromServer(preferredDiff?: string) {
		try {
			const status = await invoke<any>('get_status');
			const serverUrl = status?.server_url;
			if (!serverUrl) {
				if (layoutRetries++ < 15) {
					setTimeout(() => fetchLayoutFromServer(preferredDiff), 2000);
				} else {
					logToApp('[timer] giving up layout fetch after 15 retries');
				}
				return;
			}
			if (preferredDiff) lockedDifficulty = preferredDiff;
			const diff = preferredDiff ?? lockedDifficulty;
			const diffs = diff ? [diff] : ['Uber', 'Merciless', 'Cruel', 'Normal'];
			for (const d of diffs) {
				const r = await fetch(`${serverUrl}/api/lab/layout/${d}`);
				if (r.ok) {
					const layout: LabLayout = await r.json();
					navState = loadLayout(navState, layout);
					if (!lockedDifficulty) lockedDifficulty = layout.difficulty;
					layoutLoaded = true;
					return;
				}
			}
		} catch (e) {
			logToApp(`[timer] fetchLayout error: ${e}`);
		}
	}

	// --- Init: read settings, fetch layout, catch up ---
	$effect(() => {
		if (layoutLoaded) return;
		(async () => {
			const settings = await invoke<any>('get_compass_settings').catch((e: any) => {
				logToApp(`[timer] init settings failed: ${e}`);
				return null;
			});
			if (settings?.difficulty) lockedDifficulty = settings.difficulty;
			if (settings?.strategy) navState = setStrategy(navState, settings.strategy);
			await fetchLayoutFromServer();
			// Catch up to current lab state
			const catchup = await invoke<any>('get_lab_catchup').catch((e: any) => {
				logToApp(`[timer] catchup failed: ${e}`);
				return null;
			});
			if (catchup?.events?.length) {
				logToApp(`[timer] replaying ${catchup.events.length} catchup events`);
				for (const event of catchup.events) {
					onNavEvent(event);
				}
			}
		})();
	});

	// --- Listen for lab-nav events ---
	$effect(() => {
		let cancelled = false;
		const navPromise = listen<NavEvent>('lab-nav', (event) => {
			if (cancelled) return;
			onNavEvent(event.payload);
		});
		const layoutPromise = listen<any>('lab-layout-updated', (event) => {
			if (cancelled) return;
			fetchLayoutFromServer(event.payload?.difficulty);
		});
		return () => {
			cancelled = true;
			navPromise.then((unlisten) => unlisten());
			layoutPromise.then((unlisten) => unlisten());
		};
	});
</script>

<div class="timer-container">
	{#if !hidden}
		<div class="timer-display">{timerText}</div>
	{/if}
</div>

<style>
	:global(html), :global(body) {
		margin: 0;
		padding: 0;
		background: transparent !important;
		overflow: hidden;
	}

	.timer-container {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		pointer-events: none;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.timer-display {
		font-family: 'Consolas', 'Monaco', monospace;
		font-size: min(70vh, 28vw);
		font-weight: 700;
		color: #e5e7eb;
		text-shadow: 0 1px 3px rgba(0, 0, 0, 0.9);
		background: rgba(13, 13, 21, 0.75);
		padding: 0 0.15em;
		border-radius: 4px;
		white-space: nowrap;
		line-height: 1;
		letter-spacing: 0.05em;
	}
</style>
