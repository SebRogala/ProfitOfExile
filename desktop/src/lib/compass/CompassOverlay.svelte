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
		roomName = '',
		contentNames = [] as string[],
		timerText = '00:00',
	} = $props();

	const directions = $derived(doors.map(d => d.direction));
	const hasContent = $derived(contents.length > 0);
</script>

{#if mode === 'minimap'}
	<RoomMinimap {areaCode} {doors} {contents} {targetDirection} {roomName} />
{:else if mode === 'direction'}
	<DirectionCompass {directions} {targetDirection} {roomName} {hasContent} />
{:else if mode === 'minimal'}
	<MinimalBar {targetDirection} contents={contentNames} {timerText} />
{/if}
