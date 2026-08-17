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
	} from '$lib/compass/navigation';
	import { fetchLabLayout } from '$lib/compass/layout-loader';

	// Everything this window can report goes through the app log. console is
	// unreachable in a release webview with no devtools, so a console.warn here
	// is a message that is never read by anyone — and a path strip that has
	// given up renders nothing at all, which makes the log the only trace.
	function logToApp(msg: string) {
		invoke('app_log_from_frontend', { msg }).catch((e) => console.warn('[pathstrip] logToApp IPC failed:', msg, e));
	}

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
				// Track visited: the room being left becomes visited — but only when
				// the tracker actually MOVED. A RoomChanged that resolves to the room
				// already tracked (a duplicate log line re-announcing it, a name no
				// room in the layout carries, a name too ambiguous to place) must not
				// retire the room the player is standing in.
				//
				// This list is append-only on purpose. Badges mean UNCOLLECTED content,
				// so a room must stay retired once left — walking back through it (a
				// golden-door route does exactly that) must not re-light the darkshrine
				// marker of a shrine already taken. The "current room always renders in
				// full" half of the invariant is LabGraph's job, enforced there for
				// both of its visited sources.
				const nextRoomId = navState.currentRoom;
				if (currentRoomId && currentRoomId !== nextRoomId && !visitedRoomIds.includes(currentRoomId)) {
					visitedRoomIds = [...visitedRoomIds, currentRoomId];
				}
				currentRoomId = nextRoomId;
				// Hide during Izaro fights, show again on regular rooms
				const room = navState.currentRoom ? navState.roomById.get(navState.currentRoom) : null;
				hidden = room?.name.toLowerCase() === "aspirant's trial";
				break;
			}
			case 'LabFinished':
				break;
			case 'LabExited':
				currentRoomId = null;
				// Symmetric with PlazaEntered. Without it, a run whose PlazaEntered
				// falls outside the replay window starts wearing the previous run's
				// visited set, and rooms it never entered render dimmed.
				visitedRoomIds = [];
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
		if (preferredDiff) lockedDifficulty = preferredDiff;
		const layout = await fetchLabLayout({
			preferredDifficulty: preferredDiff,
			lockedDifficulty,
			log: (msg) => logToApp(`[pathstrip] ${msg}`),
		});
		if (!layout) return;
		navState = loadLayout(navState, layout);
		if (!lockedDifficulty) lockedDifficulty = layout.difficulty;
		layoutLoaded = true;
		logToApp(`[pathstrip] layout loaded: ${layout.difficulty} (${layout.rooms.length} rooms)`);
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
				logToApp(`[pathstrip] catchup failed: ${e}`);
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
						logToApp(`[pathstrip] settings poll failed: ${e}`);
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
