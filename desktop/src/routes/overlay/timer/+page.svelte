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
	let pendingLayoutReset = $state(false);

	function applyLayoutReset() {
		navState = createNavState();
		layoutLoaded = false;
		lockedDifficulty = null;
		pendingLayoutReset = false;
		logToApp('[timer] layout reset applied (midnight UTC)');
	}

	// --- Timer state ---
	let timer = $state(createTimerState());
	let elapsed = $state(0);
	let timerText = $derived(formatTimer(elapsed));
	let hidden = $state(true);

	// --- Appearance (configurable from settings) ---
	let bgOpacity = $state(0.75);
	let textStroke = $state(true);
	let bgStyle = $derived(`rgba(13, 13, 21, ${bgOpacity})`);

	// --- Room tracking for run submission ---
	let roomLog = $state<{ room_name: string; entered_at: string; room_number: number }[]>([]);
	let roomCounter = 0;
	let killTime = $state(0); // Izaro death time — snapshotted on LabFinished

	function startTimer() { timer = timerStart_fn(timer, (e) => { elapsed = e; }); }
	function stopTimer() { timer = timerStop_fn(timer); }
	function resetTimer() { timer = timerReset_fn(timer); elapsed = 0; roomLog = []; roomCounter = 0; killTime = 0; }

	function logToApp(msg: string) {
		invoke('app_log_from_frontend', { msg }).catch((e) => console.warn('[timer] logToApp IPC failed:', msg, e));
	}

	// --- Submit completed run to server ---
	// Called on LabExited with the full run data. Kill time was snapshotted
	// on LabFinished; total elapsed is the time from first room to lab exit.
	async function submitRun() {
		const totalElapsed = elapsed;
		const snapshotRooms = [...roomLog];
		const snapshotKillTime = killTime;
		const snapshotStartedAt = timer.startTimestamp;
		if (snapshotKillTime <= 0 || snapshotRooms.length === 0) return;
		try {
			const status = await invoke<any>('get_status');
			const serverUrl = status?.server_url;
			if (!serverUrl) { logToApp('[timer] no server_url, cannot submit run'); return; }

			const settings = await invoke<any>('get_compass_settings').catch(() => null);
			const visitedRoomNames = new Set(snapshotRooms.map(r => r.room_name.toLowerCase()));
			const hasGoldenDoor = navState.lockedDoors.length > 0 ||
				Array.from(navState.roomById.values())
					.filter(r => visitedRoomNames.has(r.name.toLowerCase()))
					.some(r => r.contents.some(c => c.toLowerCase().includes('golden-door')));

			const body = {
				difficulty: settings?.difficulty ?? lockedDifficulty ?? 'Uber',
				strategy: settings?.strategy ?? navState.strategy,
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
				logToApp(`[timer] run submitted: id=${data.run_id}, kill=${snapshotKillTime}s, total=${totalElapsed}s`);
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
				// Snapshot Izaro death time — timer keeps running through looting
				killTime = elapsed;
				hidden = false;
				break;
			case 'LabExited':
				stopTimer();
				submitRun();
				resetTimer();
				hidden = true;
				if (pendingLayoutReset) {
					applyLayoutReset();
				}
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

	// --- Load appearance settings ---
	$effect(() => {
		invoke<any>('get_timer_appearance').then((a) => {
			if (a) {
				bgOpacity = a.bg_opacity;
				textStroke = a.text_stroke;
			}
		}).catch(() => {});
	});

	// --- Listen for lab-nav events + appearance changes ---
	$effect(() => {
		let cancelled = false;
		const navPromise = listen<NavEvent>('lab-nav', (event) => {
			if (cancelled) return;
			onNavEvent(event.payload);
		});
		const layoutPromise = listen<any>('lab-layout-updated', (event) => {
			if (cancelled) return;
			if (event.payload?.action === 'reset') {
				if (navState.inLab) {
					// Mid-run at midnight — defer reset until lab exit
					pendingLayoutReset = true;
					logToApp('[timer] layout reset deferred (in lab)');
				} else {
					applyLayoutReset();
				}
				return;
			}
			fetchLayoutFromServer(event.payload?.difficulty);
		});
		const appearancePromise = listen<any>('timer-appearance-changed', (event) => {
			if (cancelled) return;
			const a = event.payload;
			if (a) {
				bgOpacity = a.bg_opacity;
				textStroke = a.text_stroke;
			}
		});
		return () => {
			cancelled = true;
			navPromise.then((unlisten) => unlisten());
			layoutPromise.then((unlisten) => unlisten());
			appearancePromise.then((unlisten) => unlisten());
		};
	});
</script>

<div class="timer-container" style:background={!hidden ? bgStyle : 'transparent'}>
	{#if !hidden}
		<svg viewBox="0 0 236 60" class="timer-svg" preserveAspectRatio="xMidYMid meet">
			<text x="118" y="52" text-anchor="middle" class="timer-text" class:stroked={textStroke}>{timerText}</text>
		</svg>
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
		border-radius: 4px;
	}

	.timer-svg {
		width: 100%;
		height: 100%;
	}

	.timer-text {
		font-family: 'Consolas', 'Monaco', monospace;
		font-size: 72px;
		font-weight: 700;
		fill: #e5e7eb;
		letter-spacing: 0.05em;
	}

	.timer-text.stroked {
		stroke: rgba(0, 0, 0, 0.85);
		stroke-width: 2px;
		paint-order: stroke fill;
	}
</style>
