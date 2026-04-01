<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import { invoke } from '@tauri-apps/api/core';
	import CompassOverlay from '$lib/compass/CompassOverlay.svelte';
	import {
		getPresetsByName,
		getDoorExitLocations,
		getContentLocations,
		type RoomPreset,
		type DoorExitLocation,
		type ContentLocation,
	} from '$lib/compass/room-presets';

	// --- Navigation event payload (tagged enum from Rust) ---
	type NavEvent =
		| { type: 'PlazaEntered' }
		| { type: 'RoomChanged'; name: string }
		| { type: 'SectionFinished' }
		| { type: 'LabFinished' }
		| { type: 'LabExited' };

	// --- State ---
	let inLab = $state(false);
	let roomName = $state('');
	let currentPreset = $state<RoomPreset | null>(null);
	let doors = $state<DoorExitLocation[]>([]);
	let contents = $state<ContentLocation[]>([]);
	let mode = $state<'minimap' | 'direction' | 'minimal'>('minimap');

	// Timer state
	let timerStart = $state<number | null>(null);
	let timerInterval = $state<ReturnType<typeof setInterval> | null>(null);
	let elapsed = $state(0);

	let timerText = $derived(formatTimer(elapsed));

	function formatTimer(seconds: number): string {
		const m = Math.floor(seconds / 60);
		const s = seconds % 60;
		return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
	}

	function startTimer() {
		if (timerStart !== null) return; // already running
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
		// Keep timerStart and elapsed so the display persists
	}

	function clearAll() {
		inLab = false;
		roomName = '';
		currentPreset = null;
		doors = [];
		contents = [];
		stopTimer();
		timerStart = null;
		elapsed = 0;
	}

	function handleNavEvent(event: NavEvent) {
		switch (event.type) {
			case 'PlazaEntered':
				inLab = true;
				roomName = '';
				currentPreset = null;
				doors = [];
				contents = [];
				break;

			case 'RoomChanged': {
				const presets = getPresetsByName(event.name, false);
				roomName = event.name;

				if (presets.length === 0) {
					currentPreset = null;
					doors = [];
					contents = [];
				} else {
					const variant = presets[0];
					currentPreset = variant;
					doors = getDoorExitLocations(variant);
					contents = getContentLocations(variant);
				}

				// Start timer on first room change inside the lab
				if (inLab && timerStart === null) {
					startTimer();
				}
				break;
			}

			case 'SectionFinished':
				// No special handling — room data stays
				break;

			case 'LabFinished':
				stopTimer();
				// Keep display so the player can see final state
				break;

			case 'LabExited':
				clearAll();
				break;
		}
	}

	// --- Initialization via $effect (onMount does not fire in overlay windows) ---

	// Load compass mode setting from Rust
	$effect(() => {
		invoke<string>('get_compass_settings')
			.then((settings: any) => {
				if (settings?.mode) {
					mode = settings.mode;
				}
			})
			.catch((e) => {
				console.warn('[compass] get_compass_settings failed, using default:', e);
			});
	});

	// Listen for lab-nav events from the Rust backend
	$effect(() => {
		let cancelled = false;

		const unlistenPromise = listen<NavEvent>('lab-nav', (event) => {
			if (cancelled) return;
			handleNavEvent(event.payload);
		});

		return () => {
			cancelled = true;
			unlistenPromise.then((unlisten) => unlisten());
		};
	});
</script>

<div class="compass-container">
	{#if inLab || currentPreset}
		<CompassOverlay
			{mode}
			areaCode={currentPreset?.areaCode ?? ''}
			{doors}
			{contents}
			roomName={roomName}
			{timerText}
		/>
	{/if}
</div>

<style>
	.compass-container {
		position: fixed;
		top: 0;
		right: 0;
		pointer-events: none;
	}
</style>
