<script lang="ts">
	import { getContext, untrack } from 'svelte';
	import LabGraph from '$lib/compass/LabGraph.svelte';
	import type { NavState } from '$lib/compass/navigation';
	import { LAB_SESSION, type LabNavEvent, type LabSession } from '$lib/compass/lab-session.svelte';

	let { configMode, label }: { configMode: boolean; label: string } = $props();

	const session = getContext<LabSession>(LAB_SESSION);

	let currentRoomId = $state<string | null>(null);
	let visitedRoomIds = $state<string[]>([]);
	let hidden = $state(false);
	const drawing = $derived(!!session.navState.layout && !hidden && session.navState.inLab);

	function onReset() {
		currentRoomId = null;
		visitedRoomIds = [];
	}

	function onNavEvent(event: LabNavEvent, navAfter: NavState) {
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
				const nextRoomId = navAfter.currentRoom;
				if (currentRoomId && currentRoomId !== nextRoomId && !visitedRoomIds.includes(currentRoomId)) {
					visitedRoomIds = [...visitedRoomIds, currentRoomId];
				}
				currentRoomId = nextRoomId;
				// Hide during Izaro fights, show again on regular rooms
				const room = navAfter.currentRoom ? navAfter.roomById.get(navAfter.currentRoom) : null;
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
				break;
		}
	}

	$effect(() => untrack(() => session.acquire({ onNavEvent, onReset })));
</script>

<div class="strip-container">
	{#if drawing}
		<!-- The window no longer hides with the lab, so `drawing` is true only while
		     `navState.inLab` is true (set by PlazaEntered, cleared by LabExited, and
		     rebuilt by catch-up replay); `hidden` remains the in-lab rule for Izaro fights. -->
		<LabGraph
			navState={session.navState}
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
	{:else if configMode}
		<p class="placeholder">{label}</p>
	{/if}
</div>

<style>
	.strip-container {
		position: relative;
		width: 100%;
		height: 100%;
		pointer-events: none;
	}

	.placeholder {
		padding: 4px 8px;
		font-size: 11px;
		color: var(--color-lab-text-muted);
	}
</style>
