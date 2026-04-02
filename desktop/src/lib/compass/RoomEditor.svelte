<script lang="ts">
	import { getPresetsByName, getDoorExitLocations, getContentLocations, type RoomPreset } from './room-presets';
	import Select from '$lib/components/Select.svelte';
	import { getRoomSvgUrl, getDisabledSvgUrl } from './svg-loader';
	import type { LabLayoutRoom } from './navigation';

	let {
		room,
		serverUrl = '',
		difficulty = '',
		onSave = (() => {}) as () => void,
		onClose = (() => {}) as () => void,
	}: {
		room: LabLayoutRoom;
		serverUrl: string;
		difficulty: string;
		onSave: () => void;
		onClose: () => void;
	} = $props();

	// Available variants for this room
	const hasGoldenDoor = room.contents.some(c => c.toLowerCase().includes('golden-door'));
	const variants = getPresetsByName(room.name, hasGoldenDoor);
	const variantsNoDoor = hasGoldenDoor ? [] : getPresetsByName(room.name, true);
	const allVariants = [...variants, ...variantsNoDoor];

	let selectedAreaCode = $state(room.areacode || (allVariants[0]?.areaCode ?? ''));

	// Content toggles
	const contentTypes = [
		{ key: 'argus', label: 'Argus', color: '#ea580c' },
		{ key: 'darkshrine', label: 'Darkshrine', color: '#ef4444' },
		{ key: 'golden-key', label: 'Golden Key', color: '#fbbf24' },
		{ key: 'golden-door', label: 'Golden Door', color: '#d97706' },
		{ key: 'silver-key', label: 'Silver Key', color: '#9ca3af' },
		{ key: 'silver-door', label: 'Silver Door', color: '#9ca3af' },
	];

	let contentState = $state<Record<string, boolean>>({});

	// Initialize from room contents
	for (const ct of contentTypes) {
		contentState[ct.key] = room.contents.some(c => c.toLowerCase().includes(ct.key));
	}

	const gauntletOptions = ['', 'Trap gauntlet', 'Escort gauntlet'];
	const puzzleOptions = ['', 'Switch puzzle', 'Lever puzzle'];
	const secretOptions = ['', 'Well', 'Loose Grating', 'Hidden Switch', 'Crumbling Wall'];

	let gauntletValue = $state(
		room.contents.find(c => c.toLowerCase().includes('gauntlet')) ?? ''
	);
	let puzzleValue = $state(
		room.contents.find(c => c.toLowerCase().includes('puzzle')) ?? ''
	);
	let secretPassage = $state(room.secret_passage ?? '');
	let saving = $state(false);
	let error = $state('');

	async function save() {
		saving = true;
		error = '';

		const newContents: string[] = [];
		for (const ct of contentTypes) {
			if (contentState[ct.key]) newContents.push(ct.key);
		}
		if (gauntletValue) newContents.push(gauntletValue);
		if (puzzleValue) newContents.push(puzzleValue);

		try {
			const res = await fetch(`${serverUrl}/api/lab/layout/${difficulty}/room/${room.id}`, {
				method: 'PATCH',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					areacode: selectedAreaCode,
					contents: newContents,
					secret_passage: secretPassage || null,
				}),
			});
			if (!res.ok) {
				const data = await res.json().catch(() => ({}));
				throw new Error(data.error || `Server returned ${res.status}`);
			}
			onSave();
		} catch (e: any) {
			error = e?.message || 'Save failed';
		} finally {
			saving = false;
		}
	}
</script>

<div class="editor">
	<div class="editor-header">
		<span class="editor-title">Edit: {room.name} (#{room.id})</span>
		<button class="close-btn" onclick={onClose}>&times;</button>
	</div>

	<div class="section">
		<div class="section-label">Room Layout Variant</div>
		<div class="variants">
			{#each allVariants as variant}
				{@const doors = getDoorExitLocations(variant)}
			{@const contents = getContentLocations(variant)}
				<button
					class="variant-thumb"
					class:selected={variant.areaCode === selectedAreaCode}
					onclick={() => { selectedAreaCode = variant.areaCode; }}
					title={variant.areaCode}
				>
					<div class="variant-preview" style="background-image: url({getRoomSvgUrl(variant.areaCode) ?? getDisabledSvgUrl()});">
						{#each doors as door}
							<div class="variant-dot" style="left: {door.tileRect.x * 100 + 5}%; top: {door.tileRect.y * 100 + 5}%;">
								<span class="variant-exit-label">{door.direction}</span>
							</div>
						{/each}
						{#each contents as content}
							<div class="variant-dot variant-content-dot" class:major={content.major}
								style="left: {content.tileRect.x * 100 + 5}%; top: {content.tileRect.y * 100 + 5}%;"></div>
						{/each}
					</div>
					<span class="variant-code">{variant.areaCode.replace(/^[a-z]+_/, '')}</span>
				</button>
			{/each}
			{#if allVariants.length === 0}
				<span class="no-variants">No variants available</span>
			{/if}
		</div>
	</div>

	<div class="section">
		<div class="section-label">Room Contents</div>
		<div class="content-toggles">
			{#each contentTypes as ct}
				<button
					class="content-toggle"
					class:active={contentState[ct.key]}
					style="--toggle-color: {ct.color}"
					onclick={() => { contentState[ct.key] = !contentState[ct.key]; }}
				>
					{ct.label}
				</button>
			{/each}
		</div>
		<div class="picker-row">
			<Select
				bind:value={gauntletValue}
				options={gauntletOptions.map(o => ({ value: o, label: o || 'No gauntlet' }))}
			/>
			<Select
				bind:value={puzzleValue}
				options={puzzleOptions.map(o => ({ value: o, label: o || 'No puzzle' }))}
			/>
		</div>
	</div>

	<div class="section">
		<div class="section-label">Secret Passage</div>
		<Select
			bind:value={secretPassage}
			options={secretOptions.map(o => ({ value: o, label: o || 'None' }))}
		/>
	</div>

	{#if error}
		<div class="error">{error}</div>
	{/if}

	<div class="actions">
		<button class="save-btn" onclick={save} disabled={saving}>
			{saving ? 'Saving...' : 'Save for Everyone'}
		</button>
		<button class="cancel-btn" onclick={onClose}>Cancel</button>
	</div>
</div>

<style>
	.editor {
		background: var(--color-lab-surface, #1a1d27);
		border: 1px solid var(--color-lab-border, #2a2d37);
		border-radius: 8px;
		padding: 12px;
	}

	.editor-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 10px;
	}

	.editor-title {
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--color-lab-text, #e4e4e7);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--color-lab-text-secondary, #9ca3af);
		font-size: 1.2rem;
		cursor: pointer;
		padding: 0 4px;
	}

	.section {
		margin-bottom: 10px;
	}

	.section-label {
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary, #9ca3af);
		margin-bottom: 6px;
		font-weight: 600;
	}

	.variants {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
		gap: 8px;
	}

	.variant-thumb {
		border: 2px solid #374151;
		border-radius: 6px;
		background: #111827;
		cursor: pointer;
		padding: 6px;
		display: flex;
		flex-direction: column;
		align-items: center;
	}

	.variant-thumb.selected {
		border-color: #10b981;
		background: #064e3b;
	}

	.variant-preview {
		position: relative;
		width: 100%;
		aspect-ratio: 1.3;
		background-size: 100% 100%;
		background-repeat: no-repeat;
	}

	.variant-dot {
		position: absolute;
		transform: translate(-50%, -50%);
	}

	.variant-exit-label {
		background: rgba(0, 0, 0, 0.75);
		color: #67e8f9;
		font-size: 9px;
		font-weight: 700;
		padding: 1px 4px;
		border-radius: 3px;
		border: 1px solid #0e7490;
	}

	.variant-content-dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: #fbbf24;
		opacity: 0.8;
	}

	.variant-content-dot.major {
		background: #a855f7;
	}

	.variant-code {
		font-size: 9px;
		color: #9ca3af;
		margin-top: 3px;
		font-weight: 500;
	}

	.no-variants {
		color: #6b7280;
		font-size: 0.75rem;
	}

	.content-toggles {
		display: flex;
		gap: 4px;
		flex-wrap: wrap;
		margin-bottom: 6px;
	}

	.content-toggle {
		padding: 4px 10px;
		border: 2px solid #374151;
		border-radius: 5px;
		background: #111827;
		color: #6b7280;
		font-size: 0.75rem;
		cursor: pointer;
		font-weight: 700;
	}

	.content-toggle.active {
		border-color: var(--toggle-color);
		background: var(--toggle-color);
		color: #000;
	}

	.picker-row {
		display: flex;
		gap: 6px;
	}

	.actions {
		display: flex;
		gap: 6px;
		margin-top: 8px;
	}

	.save-btn {
		background: #059669;
		border: none;
		color: white;
		padding: 5px 12px;
		border-radius: 4px;
		font-size: 0.75rem;
		font-weight: 600;
		cursor: pointer;
	}

	.save-btn:disabled {
		opacity: 0.5;
	}

	.cancel-btn {
		background: transparent;
		border: 1px solid #374151;
		color: #9ca3af;
		padding: 5px 12px;
		border-radius: 4px;
		font-size: 0.75rem;
		cursor: pointer;
	}

	.error {
		color: #ef4444;
		font-size: 0.75rem;
		margin-top: 4px;
	}
</style>
