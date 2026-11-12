<script lang="ts">
	import { getContext, untrack } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import CompassOverlay from '$lib/compass/CompassOverlay.svelte';
	import { resolvePreset, getDoorExitLocations, getVisibleContentLocations, hasSecretPassage } from '$lib/compass/room-presets';
	import {
		resolveRouteDoors,
		getNextExitText,
		getRoomContents,
		type NavState,
	} from '$lib/compass/navigation';
	import {
		LAB_SESSION,
		type CompassSettings,
		type LabNavEvent,
		type LabSession,
	} from '$lib/compass/lab-session.svelte';
	import type { DoorExitLocation, ContentLocation } from '$lib/compass/room-presets';
	import {
		createTimerState,
		formatTimer,
		startTimer as timerStart_fn,
		stopTimer as timerStop_fn,
		resetTimer as timerReset_fn,
	} from '$lib/compass/timer';

	let { configMode, label }: { configMode: boolean; label: string } = $props();

	// --- State ---
	const session = getContext<LabSession>(LAB_SESSION);
	let mode = $state<'minimap' | 'direction' | 'minimal'>('minimap');

	// Timer state (shared module)
	let timer = $state(createTimerState());
	let elapsed = $state(0);
	let timerText = $derived(formatTimer(elapsed));

	function startTimer() { timer = timerStart_fn(timer, (e) => { elapsed = e; }); }
	function stopTimer() { timer = timerStop_fn(timer); }
	function resetTimer() { timer = timerReset_fn(timer); elapsed = 0; }

	// --- Derived from navigation state ---
	let navState = $derived(session.navState);
	let currentRoom = $derived(navState.currentRoom ? navState.roomById.get(navState.currentRoom) : null);
	// The same preset the room's exits were normalized with at layout load.
	let preset = $derived(currentRoom ? resolvePreset(currentRoom) : null);
	// Door markers use PRESET doorLocations for positioning (they know where
	// physical doors are in the room SVG). The target is the door the next
	// room's connection was normalized onto at layout load.
	let doors = $derived<DoorExitLocation[]>(preset ? getDoorExitLocations(preset) : []);
	// Entry and exit doors from the resolver the room editor uses too, against
	// the same preset, so the two views cannot disagree.
	let routeDoors = $derived(
		currentRoom && navState.layout
			? resolveRouteDoors(navState.layout.rooms, navState.plannedRoute, currentRoom.id, navState.previousRoom, preset)
			: { entry: null, exit: null },
	);
	let navDirection = $derived(routeDoors.exit);
	let entryDirection = $derived(routeDoors.entry);
	// Route guidance needs no preset: a room without one keeps its layout direction.
	let targetDirection = $derived(navDirection);
	// Content positions shown only for the loot the room holds; 'C' as the way
	// on is a secret passage, which turns non-major content into the target.
	let contents = $derived<ContentLocation[]>(
		preset && currentRoom
			? getVisibleContentLocations(
					preset,
					currentRoom.contents,
					hasSecretPassage(navState.exitDirections.get(currentRoom.id) ?? []),
					navDirection === 'C',
				)
			: [],
	);
	let exitText = $derived(getNextExitText(navState));
	let roomName = $derived(currentRoom?.name ?? '');
	let areaCode = $derived(preset?.areaCode ?? currentRoom?.areacode ?? '');
	let contentNames = $derived(currentRoom ? getRoomContents(navState, currentRoom.id) : []);
	let hidden = $state(false);
	const drawing = $derived(session.layoutLoaded && !hidden && navState.inLab);

	// Shrine warning state
	let shrineEnabled = $state(true);
	let shrineSize = $state('medium');
	let shrineCorner = $state('bottom-right');
	let shrineOnTake = $state('green');
	let shrineTakenInRoom = $state(false);
	let hasDarkshrine = $derived(contentNames.some(c => c.toLowerCase() === 'darkshrine'));

	// --- Event handling ---

	function logToApp(msg: string) {
		// console is the only fallback left once the log IPC itself is what
		// failed. It is unreachable in a release webview, but a swallowed failure
		// here hid the fact that nothing else in this window was being logged.
		invoke('app_log_from_frontend', { msg }).catch((e) => console.warn('[compass] logToApp IPC failed:', msg, e));
	}

	function logRoom(navAfter: NavState) {
		if (!navAfter.currentRoom) return;
		const room = navAfter.roomById.get(navAfter.currentRoom);
		const doorsAfter = room && navAfter.layout
			? resolveRouteDoors(navAfter.layout.rooms, navAfter.plannedRoute, room.id, navAfter.previousRoom, resolvePreset(room))
			: { entry: null, exit: null };
		logToApp(`[compass] room=${room?.name} id=${navAfter.currentRoom} route=${navAfter.plannedRoute.slice(0,4).join('>')} navDir=${doorsAfter.exit} targetDir=${doorsAfter.exit} entryDir=${doorsAfter.entry}`);
	}

	function onNavEvent(event: LabNavEvent, navAfter: NavState) {
		logRoom(navAfter);

		switch (event.type) {
			case 'PlazaEntered':
				resetTimer();
				hidden = false;
				break;
			case 'RoomChanged': {
				if (navAfter.inLab && timer.startTimestamp === null) {
					startTimer();
				}
				shrineTakenInRoom = false;
				// Hide during Izaro fights, show again on regular rooms
				const room = navAfter.currentRoom ? navAfter.roomById.get(navAfter.currentRoom) : null;
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

	function onSettings(settings: CompassSettings) {
		if (settings.mode && settings.mode !== mode) mode = settings.mode as typeof mode;
		if (settings.shrine_warn_enabled !== undefined) shrineEnabled = settings.shrine_warn_enabled;
		if (settings.shrine_warn_size) shrineSize = settings.shrine_warn_size;
		if (settings.shrine_warn_corner) shrineCorner = settings.shrine_warn_corner;
		if (settings.shrine_warn_on_take) shrineOnTake = settings.shrine_warn_on_take;
	}

	$effect(() => untrack(() => session.acquire({ onNavEvent, onSettings })));
</script>

<div class="compass-container">
	{#if drawing}
		<!-- The window no longer hides with the lab, so `drawing` is true only while
		     `navState.inLab` is true (set by PlazaEntered, cleared by LabExited, and
		     rebuilt by catch-up replay); `hidden` remains the in-lab rule for Izaro fights. -->
		<div class="compass-content">
			<CompassOverlay
				{mode}
				{areaCode}
				{doors}
				{contents}
				{targetDirection}
				{entryDirection}
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
	{:else if configMode}
		<p class="placeholder">{label}</p>
	{/if}
</div>

<style>
	.compass-container {
		position: relative;
		width: 100%;
		height: 100%;
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

	.placeholder {
		padding: 4px 8px;
		font-size: 11px;
		color: var(--color-lab-text-muted);
	}

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

