<script lang="ts">
	import type { DoorExitLocation, ContentLocation } from './room-presets';
	import RoomMinimap from './RoomMinimap.svelte';
	import DirectionCompass from './DirectionCompass.svelte';
	import MinimalBar from './MinimalBar.svelte';

	let {
		mode = 'minimap' as 'minimap' | 'direction' | 'minimal',
		areaCode = '',
		doors = [] as DoorExitLocation[],
		contents = [] as ContentLocation[],
		targetDirection = null as string | null,
		entryDirection = null as string | null,
		roomName = '',
		contentNames = [] as string[],
		timerText = '00:00',
	} = $props();

	const directions = $derived(doors.map(d => d.direction));
</script>

{#if mode === 'minimap'}
	<RoomMinimap {areaCode} {doors} {contents} {targetDirection} {entryDirection} {roomName} />
{:else if mode === 'direction'}
	<DirectionCompass {directions} {targetDirection} {roomName} {contents} />
{:else if mode === 'minimal'}
	<MinimalBar {targetDirection} contents={contentNames} {timerText} />
{/if}
