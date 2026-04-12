<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import { invoke } from '@tauri-apps/api/core';
	import LabGraph from '$lib/compass/LabGraph.svelte';
	import {
		createNavState,
		loadLayout,
		handleNavEvent,
		setStrategy,
		type NavEvent,
		type LabLayout,
	} from '$lib/compass/navigation';

	let navState = $state(createNavState());
	let pendingLayoutReset = $state(false);
	let currentRoomId = $state<string | null>(null);
	let visitedRoomIds = $state<string[]>([]);
	let hidden = $state(false);

	function applyLayoutReset() {
		navState = createNavState();
		layoutLoaded = false;
		pendingLayoutReset = false;
		currentRoomId = null;
		visitedRoomIds = [];
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
		switch (event.type) {
			case 'PlazaEntered':
				currentRoomId = null;
				visitedRoomIds = [];
				hidden = false;
				break;
			case 'RoomChanged': {
				// Track visited: previous room becomes visited
				if (currentRoomId && !visitedRoomIds.includes(currentRoomId)) {
					visitedRoomIds = [...visitedRoomIds, currentRoomId];
				}
				currentRoomId = navState.currentRoom;
				// Hide during Izaro fights, show again on regular rooms
				const room = navState.currentRoom ? navState.roomById.get(navState.currentRoom) : null;
				hidden = room?.name.toLowerCase() === "aspirant's trial";
				break;
			}
			case 'LabFinished':
				break;
			case 'LabExited':
				currentRoomId = null;
				hidden = true;
				if (pendingLayoutReset) {
					applyLayoutReset();
				}
				break;
		}
	}

	let lockedDifficulty = $state<string | null>(null);
	let layoutLoaded = $state(false);

	async function fetchLayoutFromServer(preferredDiff?: string) {
		try {
			const status = await invoke<any>('get_status');
			const serverUrl = status?.server_url;
			if (!serverUrl) {
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
					return;
				}
			}
		} catch (e) {
			console.warn('[pathstrip] fetchLayout error:', e);
		}
	}

	// Fetch layout and catch up to current lab state on init
	$effect(() => {
		if (layoutLoaded) return;
		(async () => {
			// Read difficulty/strategy from compass settings before fetching layout
			const settings = await invoke<any>('get_compass_settings').catch(() => null);
			if (settings?.difficulty) lockedDifficulty = settings.difficulty;
			if (settings?.strategy) navState = setStrategy(navState, settings.strategy);
			await fetchLayoutFromServer();
			// Replay recent Client.txt events to reconstruct current room
			const catchup = await invoke<any>('get_lab_catchup').catch((e: any) => {
				console.warn('[pathstrip] catchup failed:', e);
				return null;
			});
			if (catchup?.events?.length) {
				for (const event of catchup.events) {
					onNavEvent(event);
				}
			}
		})();
	});

	// Poll strategy from compass settings every 2s — pathstrip shares strategy with compass (set from planner tab)
	let settingsErrorLogged = false;
	$effect(() => {
		function loadSettings() {
			invoke<any>('get_compass_settings')
				.then((settings) => {
					settingsErrorLogged = false;
					if (settings?.strategy && settings.strategy !== navState.strategy) {
						navState = setStrategy(navState, settings.strategy);
					}
				})
				.catch((e: any) => {
					if (!settingsErrorLogged) {
						console.warn('[pathstrip] settings poll failed:', e);
						settingsErrorLogged = true;
					}
				});
		}
		loadSettings();
		const interval = setInterval(loadSettings, 2000);
		return () => clearInterval(interval);
	});

	// Listen for lab-nav events + layout updates from server polling
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
					pendingLayoutReset = true;
				} else {
					applyLayoutReset();
				}
				return;
			}
			fetchLayoutFromServer(event.payload?.difficulty);
		});
		return () => {
			cancelled = true;
			navPromise.then((unlisten) => unlisten());
			layoutPromise.then((unlisten) => unlisten());
		};
	});
</script>

<div class="strip-container">
	{#if navState.layout && !hidden}
		<LabGraph
			{navState}
			width={900}
			height={350}
			padding={40}
			nodeRadius={20}
			showLabels={false}
			showContentDots={true}
			compact={false}
			{currentRoomId}
			{visitedRoomIds}
		/>
	{/if}
</div>

<style>
	.strip-container {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		pointer-events: none;
		background: transparent;
	}
</style>
