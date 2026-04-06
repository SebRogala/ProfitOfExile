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

	// --- State ---
	let navState = $state(createNavState());
	let mode = $state<'minimap' | 'direction' | 'minimal'>('minimap');

	// Timer state
	let timerStart = $state<number | null>(null);
	let timerInterval: ReturnType<typeof setInterval> | null = null;
	let elapsed = $state(0);

	let timerText = $derived(formatTimer(elapsed));

	function formatTimer(seconds: number): string {
		const m = Math.floor(seconds / 60);
		const s = seconds % 60;
		return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
	}

	function startTimer() {
		if (timerStart !== null) return;
		timerStart = Date.now();
		elapsed = 0;
		timerInterval = setInterval(() => {
			if (timerStart !== null) {
				elapsed = Math.floor((Date.now() - timerStart) / 1000);
			}
		}, 1000);
	}

	function stopTimer() {
		if (timerInterval !== null) {
			clearInterval(timerInterval);
			timerInterval = null;
		}
	}

	function resetTimer() {
		stopTimer();
		timerStart = null;
		elapsed = 0;
	}

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
				if (navState.inLab && timerStart === null) {
					startTimer();
				}
				// Hide during Izaro fights, show again on regular rooms
				const room = navState.currentRoom ? navState.roomById.get(navState.currentRoom) : null;
				hidden = room?.name.toLowerCase() === "aspirant's trial";
				break;
			}
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

	// Poll compass mode setting every 2s (mode can change from planner tab)
	$effect(() => {
		function loadMode() {
			invoke<any>('get_compass_settings')
				.then((settings) => {
					if (settings?.mode && settings.mode !== mode) mode = settings.mode;
				})
				.catch(() => {});
		}
		loadMode();
		const interval = setInterval(loadMode, 2000);
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
			const diffs = diff ? [diff] : ['Normal', 'Cruel', 'Merciless', 'Uber'];
			for (const d of diffs) {
				const r = await fetch(`${serverUrl}/api/lab/layout/${d}`);
				if (r.ok) {
					const layout: LabLayout = await r.json();
					navState = loadLayout(createNavState(), layout);
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

	// Fetch layout on init — only once
	$effect(() => {
		if (!layoutLoaded) fetchLayoutFromServer();
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
	}

	.exit-text {
		color: #e5e7eb;
		font-size: 10px;
		font-family: system-ui, sans-serif;
		text-align: center;
		margin-top: 4px;
		background: rgba(0, 0, 0, 0.7);
		padding: 3px 8px;
		border-radius: 4px;
		text-shadow: 0 1px 2px rgba(0, 0, 0, 0.9);
		padding: 2px 6px;
		background: rgba(13, 13, 21, 0.85);
		border-radius: 3px;
	}
</style>
