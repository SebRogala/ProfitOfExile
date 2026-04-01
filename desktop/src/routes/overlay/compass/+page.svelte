<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import { invoke } from '@tauri-apps/api/core';
	import CompassOverlay from '$lib/compass/CompassOverlay.svelte';
	import { getPresetByAreaCode, getDoorExitLocations, getContentLocations } from '$lib/compass/room-presets';
	import {
		createNavState,
		loadLayout,
		handleNavEvent,
		getNextDirection,
		getNextExitText,
		getRoomContents,
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
	let preset = $derived(currentRoom?.areacode ? getPresetByAreaCode(currentRoom.areacode) : null);
	let doors = $derived<DoorExitLocation[]>(preset ? getDoorExitLocations(preset) : []);
	let contents = $derived<ContentLocation[]>(preset ? getContentLocations(preset) : []);
	let targetDirection = $derived(getNextDirection(navState));
	let exitText = $derived(getNextExitText(navState));
	let roomName = $derived(currentRoom?.name ?? '');
	let areaCode = $derived(currentRoom?.areacode ?? '');
	let contentNames = $derived(currentRoom ? getRoomContents(navState, currentRoom.id) : []);
	let showOverlay = $derived(navState.inLab || navState.currentRoom !== null);

	// --- Event handling ---

	function onNavEvent(event: NavEvent) {
		navState = handleNavEvent(navState, event);

		switch (event.type) {
			case 'PlazaEntered':
				resetTimer();
				break;
			case 'RoomChanged':
				if (navState.inLab && timerStart === null) {
					startTimer();
				}
				break;
			case 'LabFinished':
				stopTimer();
				break;
			case 'LabExited':
				resetTimer();
				break;
		}
	}

	// --- Initialization via $effect (onMount does not fire in overlay windows) ---

	// Load compass mode setting
	$effect(() => {
		invoke<any>('get_compass_settings')
			.then((settings) => {
				if (settings?.mode) mode = settings.mode;
			})
			.catch((e) => console.warn('[compass] get_compass_settings failed:', e));
	});

	// Fetch today's layout from server
	$effect(() => {
		invoke<any>('get_status')
			.then((status) => {
				const serverUrl = status?.server_url;
				if (!serverUrl) return;
				// Try all difficulties — Uber is most common for lab farming
				for (const diff of ['Uber', 'Merciless', 'Cruel', 'Normal']) {
					fetch(`${serverUrl}/api/lab/layout/${diff}`)
						.then((r) => {
							if (r.ok) return r.json();
							return null;
						})
						.then((layout: LabLayout | null) => {
							if (layout && !navState.layout) {
								navState = loadLayout(navState, layout);
							}
						})
						.catch(() => {});
				}
			})
			.catch((e) => console.warn('[compass] get_status failed:', e));
	});

	// Listen for lab-nav events
	$effect(() => {
		let cancelled = false;
		const unlistenPromise = listen<NavEvent>('lab-nav', (event) => {
			if (cancelled) return;
			onNavEvent(event.payload);
		});
		return () => {
			cancelled = true;
			unlistenPromise.then((unlisten) => unlisten());
		};
	});
</script>

<div class="compass-container">
	{#if showOverlay}
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
		{#if exitText && mode === 'minimap'}
			<div class="exit-text">{exitText}</div>
		{/if}
	{/if}
</div>

<style>
	.compass-container {
		position: fixed;
		top: 0;
		right: 0;
		pointer-events: none;
	}

	.exit-text {
		color: #9ca3af;
		font-size: 9px;
		font-family: system-ui, sans-serif;
		text-align: center;
		margin-top: 2px;
		padding: 2px 6px;
		background: rgba(13, 13, 21, 0.85);
		border-radius: 3px;
	}
</style>
