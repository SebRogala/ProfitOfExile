<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import { invoke } from '@tauri-apps/api/core';
	import {
		createNavState,
		loadLayout,
		handleNavEvent,
		setStrategy,
		type NavEvent,
	} from '$lib/compass/navigation';
	import { fetchLabLayout } from '$lib/compass/layout-loader';
	import {
		createTimerState,
		formatTimer,
		startTimer as timerStart_fn,
		stopTimer as timerStop_fn,
		resetTimer as timerReset_fn,
	} from '$lib/compass/timer';

	// --- Navigation state (needed to know when we're in the lab and which room) ---
	// Display only: run measurement and submission live in $lib/run-recorder.ts,
	// which runs in the main window regardless of this overlay's toggle.
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

	function startTimer() { timer = timerStart_fn(timer, (e) => { elapsed = e; }); }
	function stopTimer() { timer = timerStop_fn(timer); }
	function resetTimer() { timer = timerReset_fn(timer); elapsed = 0; }

	function logToApp(msg: string) {
		invoke('app_log_from_frontend', { msg }).catch((e) => console.warn('[timer] logToApp IPC failed:', msg, e));
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
				// Hide during Izaro fights
				const room = navState.currentRoom ? navState.roomById.get(navState.currentRoom) : null;
				hidden = room?.name.toLowerCase() === "aspirant's trial";
				break;
			}
			case 'LabFinished':
				// Timer keeps running through looting
				hidden = false;
				break;
			case 'LabExited':
				stopTimer();
				resetTimer();
				hidden = true;
				if (pendingLayoutReset) {
					applyLayoutReset();
				}
				break;
		}
	}

	// --- Layout fetching (needed for navigation, room_count, and golden door detection) ---
	async function fetchLayoutFromServer(preferredDiff?: string) {
		if (preferredDiff) lockedDifficulty = preferredDiff;
		const layout = await fetchLabLayout({
			preferredDifficulty: preferredDiff,
			lockedDifficulty,
			log: (msg) => logToApp(`[timer] ${msg}`),
		});
		if (!layout) return;
		navState = loadLayout(navState, layout);
		if (!lockedDifficulty) lockedDifficulty = layout.difficulty;
		layoutLoaded = true;
		logToApp(`[timer] layout loaded: ${layout.difficulty} (${layout.rooms.length} rooms)`);
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
