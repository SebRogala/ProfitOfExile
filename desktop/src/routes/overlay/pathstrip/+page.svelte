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
	let currentRoomId = $state<string | null>(null);
	let visitedRoomIds = $state<string[]>([]);
	let hidden = $state(false);

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
			const diffs = diff ? [diff] : ['Normal', 'Cruel', 'Merciless', 'Uber'];
			for (const d of diffs) {
				const r = await fetch(`${serverUrl}/api/lab/layout/${d}`);
				if (r.ok) {
					const layout: LabLayout = await r.json();
					navState = loadLayout(createNavState(), layout);
					if (!lockedDifficulty) lockedDifficulty = layout.difficulty;
					layoutLoaded = true;
					return;
				}
			}
		} catch (e) {
			console.warn('[pathstrip] fetchLayout error:', e);
		}
	}

	// Fetch layout on init — only once
	$effect(() => {
		if (!layoutLoaded) fetchLayoutFromServer();
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
