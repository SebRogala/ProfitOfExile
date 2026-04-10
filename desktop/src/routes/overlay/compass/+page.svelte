<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import { invoke } from '@tauri-apps/api/core';
	import CompassOverlay from '$lib/compass/CompassOverlay.svelte';
	import { getPresetByAreaCode, getPresetsByName, getDoorExitLocations, getContentLocations, getTileRect, matchExitToPresetDoor } from '$lib/compass/room-presets';
	import {
		createNavState,
		loadLayout,
		handleNavEvent,
		getNextDirection,
		getNextExitText,
		getRoomContents,
		setStrategy,
		type NavEvent,
		type LabLayout,
	} from '$lib/compass/navigation';
	import type { DoorExitLocation, ContentLocation } from '$lib/compass/room-presets';
	import {
		createTimerState,
		formatTimer,
		startTimer as timerStart_fn,
		stopTimer as timerStop_fn,
		resetTimer as timerReset_fn,
	} from '$lib/compass/timer';

	// --- State ---
	let navState = $state(createNavState());
	let mode = $state<'minimap' | 'direction' | 'minimal'>('minimap');

	// Timer state (shared module)
	let timer = $state(createTimerState());
	let elapsed = $state(0);
	let timerText = $derived(formatTimer(elapsed));

	function startTimer() { timer = timerStart_fn(timer, (e) => { elapsed = e; }); }
	function stopTimer() { timer = timerStop_fn(timer); }
	function resetTimer() { timer = timerReset_fn(timer); elapsed = 0; }

	// --- Derived from navigation state ---
	let currentRoom = $derived(navState.currentRoom ? navState.roomById.get(navState.currentRoom) : null);
	let preset = $derived.by(() => {
		if (!currentRoom) return null;
		// Try area code first (exact match)
		if (currentRoom.areacode) {
			const p = getPresetByAreaCode(currentRoom.areacode);
			if (p) return p;
		}
		// Fallback: look up by room name (handles missing area codes in Normal/Cruel labs)
		const hasGoldenDoor = currentRoom.contents.some(c => c.toLowerCase().includes('golden-door'));
		const byName = getPresetsByName(currentRoom.name, hasGoldenDoor);
		if (byName.length <= 1) return byName[0] ?? null;
		// Multiple variants: prefer non-bottleneck (simpler room shape, more common)
		const nonBottleneck = byName.find(p => !p.areaCode.includes('bottleneck'));
		return nonBottleneck ?? byName[0];
	});
	// Door markers use PRESET doorLocations for positioning (they know where
	// physical doors are in the room SVG). The target is determined by matching
	// the navigation engine's next direction against the layout exits, then
	// finding which preset door corresponds to that connection.
	let doors = $derived<DoorExitLocation[]>(preset ? getDoorExitLocations(preset) : []);
	let contents = $derived<ContentLocation[]>(preset && currentRoom?.contents.length ? getContentLocations(preset) : []);
	// Nav engine returns layout exit direction (e.g. "NE") but preset doors use
	// physical positions (e.g. "N", "S"). Match by angular proximity.
	let navDirection = $derived(getNextDirection(navState));
	let targetDirection = $derived(
		navDirection && preset
			? matchExitToPresetDoor(navDirection, preset.doorLocations)
			: null
	);
	let exitText = $derived(getNextExitText(navState));
	let roomName = $derived(currentRoom?.name ?? '');
	let areaCode = $derived(preset?.areaCode ?? currentRoom?.areacode ?? '');
	let contentNames = $derived(currentRoom ? getRoomContents(navState, currentRoom.id) : []);
	let hidden = $state(false);
	let layoutLoaded = $state(false);
	let showOverlay = $derived(layoutLoaded && !hidden);

	// --- Event handling ---

	function logToApp(msg: string) {
		invoke('app_log_from_frontend', { msg }).catch(() => {});
	}

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
		if (navState.currentRoom) {
			const room = navState.roomById.get(navState.currentRoom);
			logToApp(`[compass] room=${room?.name} id=${navState.currentRoom} route=${navState.plannedRoute.slice(0,4).join('>')} navDir=${navDirection} targetDir=${targetDirection}`);
		}

		switch (event.type) {
			case 'PlazaEntered':
				resetTimer();
				hidden = false;
				break;
			case 'RoomChanged': {
				if (navState.inLab && timer.startTimestamp === null) {
					startTimer();
				}
				shrineTakenInRoom = false;
				// Hide during Izaro fights, show again on regular rooms
				const room = navState.currentRoom ? navState.roomById.get(navState.currentRoom) : null;
				hidden = room?.name.toLowerCase() === "aspirant's trial";
				break;
			}
			case 'DarkshrineActivated':
				shrineTakenInRoom = true;
				break;
			case 'LabFinished':
				stopTimer();
				break;
			case 'LabExited':
				resetTimer();
				hidden = true;
				break;
		}
	}

	// --- Initialization via $effect (onMount does not fire in overlay windows) ---

	// Shrine warning state
	let shrineEnabled = $state(true);
	let shrineSize = $state('medium');
	let shrineCorner = $state('bottom-right');
	let shrineOnTake = $state('green');
	let shrineTakenInRoom = $state(false);
	let hasDarkshrine = $derived(contentNames.some(c => c.toLowerCase() === 'darkshrine'));

	// Poll compass settings every 2s (mode/strategy/difficulty can change from planner tab)
	let settingsErrorLogged = false;
	$effect(() => {
		function loadSettings() {
			invoke<any>('get_compass_settings')
				.then((settings) => {
					settingsErrorLogged = false;
					if (settings?.mode && settings.mode !== mode) mode = settings.mode;
					if (settings?.strategy && settings.strategy !== navState.strategy) {
						navState = setStrategy(navState, settings.strategy);
					}
					if (settings?.difficulty && !lockedDifficulty) {
						lockedDifficulty = settings.difficulty;
					}
					if (settings?.shrine_warn_enabled !== undefined) shrineEnabled = settings.shrine_warn_enabled;
					if (settings?.shrine_warn_size) shrineSize = settings.shrine_warn_size;
					if (settings?.shrine_warn_corner) shrineCorner = settings.shrine_warn_corner;
					if (settings?.shrine_warn_on_take) shrineOnTake = settings.shrine_warn_on_take;
				})
				.catch((e: any) => {
					if (!settingsErrorLogged) {
						logToApp(`[compass] settings poll failed: ${e}`);
						settingsErrorLogged = true;
					}
				});
		}
		loadSettings();
		const interval = setInterval(loadSettings, 2000);
		return () => clearInterval(interval);
	});

	let lockedDifficulty = $state<string | null>(null);

	async function fetchLayoutFromServer(preferredDiff?: string) {
		try {
			const status = await invoke<any>('get_status');
			const serverUrl = status?.server_url;
			if (!serverUrl) {
				logToApp('[compass] no server_url yet, retrying in 2s');
				setTimeout(() => fetchLayoutFromServer(preferredDiff), 2000);
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
					logToApp(`[compass] layout loaded: ${layout.difficulty} (${layout.rooms.length} rooms)`);
					return;
				}
			}
			logToApp(`[compass] no layout found for: ${diffs.join(', ')}`);
		} catch (e) {
			logToApp(`[compass] fetchLayout error: ${e}`);
		}
	}

	// Fetch layout and catch up to current lab state on init
	$effect(() => {
		if (layoutLoaded) return;
		(async () => {
			// Read difficulty from settings before fetching layout
			const settings = await invoke<any>('get_compass_settings').catch(() => null);
			if (settings?.difficulty) lockedDifficulty = settings.difficulty;
			if (settings?.strategy) navState = setStrategy(navState, settings.strategy);
			if (settings?.mode) mode = settings.mode;
			await fetchLayoutFromServer();
			// Replay recent Client.txt events to reconstruct current room
			const catchup = await invoke<any>('get_lab_catchup').catch((e: any) => {
				logToApp(`[compass] catchup failed: ${e}`);
				return null;
			});
			if (catchup?.events?.length) {
				logToApp(`[compass] replaying ${catchup.events.length} catchup events`);
				for (const event of catchup.events) {
					onNavEvent(event);
				}
			}
		})();
	});

	// Listen for lab-nav events + layout updates from server
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

<div class="compass-container">
	{#if showOverlay}
		<div class="compass-content">
			<CompassOverlay
				{mode}
				{areaCode}
				{doors}
				{contents}
				{targetDirection}
				{roomName}
				{contentNames}
				{timerText}
			/>
			{#if shrineEnabled && hasDarkshrine && !(shrineTakenInRoom && shrineOnTake === 'hide')}
				<div class="shrine-warn shrine-{shrineCorner} shrine-{shrineSize}" class:shrine-taken={shrineTakenInRoom}>D</div>
			{/if}
		</div>
		{#if exitText && mode === 'minimap'}
			<div class="exit-text">{exitText}</div>
		{/if}
	{/if}
</div>

<style>
	.compass-container {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		pointer-events: none;
		display: flex;
		flex-direction: column;
	}

	.compass-content {
		flex: 1;
		min-height: 0;
		position: relative;
	}

	.shrine-warn {
		position: absolute;
		z-index: 10;
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 900;
		color: #dc2626;
		text-shadow: 0 0 6px rgba(220, 38, 38, 0.8), 0 1px 2px rgba(0, 0, 0, 0.9);
		line-height: 1;
	}

	/* Corner positions */
	.shrine-top-left { top: 4px; left: 4px; }
	.shrine-top-right { top: 4px; right: 4px; }
	.shrine-bottom-left { bottom: 4px; left: 4px; }
	.shrine-bottom-right { bottom: 4px; right: 4px; }

	.shrine-taken {
		color: #22c55e;
		text-shadow: 0 0 6px rgba(34, 197, 94, 0.8), 0 1px 2px rgba(0, 0, 0, 0.9);
	}

	/* Sizes */
	.shrine-small { font-size: 18px; }
	.shrine-medium { font-size: 40px; }
	.shrine-large { font-size: 56px; }

	.exit-text {
		color: #e5e7eb;
		font-size: 13px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		font-family: system-ui, sans-serif;
		text-align: center;
		margin-top: 4px;
		text-shadow: 0 1px 2px rgba(0, 0, 0, 0.9);
		padding: 2px 6px;
		background: rgba(13, 13, 21, 0.85);
		border-radius: 3px;
	}
</style>
