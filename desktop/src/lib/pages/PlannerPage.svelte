<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { store } from '$lib/stores/status.svelte';
	import Select from '$lib/components/Select.svelte';
	import RoomEditor from '$lib/compass/RoomEditor.svelte';
	import {
		createNavState,
		loadLayout,
		setStrategy,
		toggleTargetRoom,
		type NavState,
		type LabLayout,
		type RouteStrategy,
	} from '$lib/compass/navigation';
	import LabGraph from '$lib/compass/LabGraph.svelte';

	// --- State ---
	let navState = $state<NavState>(createNavState());
	let difficulty = $state('Uber');
	let strategyValue = $state<string>('shortest');
	let compassMode = $state('minimap');
	let shrineEnabled = $state(true);
	let shrineSize = $state('medium');
	let shrineCorner = $state('bottom-right');
	let shrineOnTake = $state('green');
	let serverUrl = $state('');
	let loading = $state(false);
	let error = $state('');
	let fileInput = $state<HTMLInputElement | null>(null);

	const difficultyOptions = [
		{ value: 'Uber', label: 'Uber Lab' },
		{ value: 'Merciless', label: 'Merciless' },
		{ value: 'Cruel', label: 'Cruel' },
		{ value: 'Normal', label: 'Normal' },
	];

	const strategyOptions = [
		{ value: 'shortest', label: 'Shortest' },
		{ value: 'darkshrines', label: 'All Darkshrines' },
		{ value: 'darkshrines-argus', label: 'Darkshrines + Argus' },
		{ value: 'everything', label: 'Everything' },
	];

	const modeOptions = [
		{ value: 'minimap', label: 'Minimap' },
		{ value: 'direction', label: 'Direction' },
		{ value: 'minimal', label: 'Minimal' },
	];

	const shrineSizeOptions = [
		{ value: 'small', label: 'Small' },
		{ value: 'medium', label: 'Medium' },
		{ value: 'large', label: 'Large' },
	];

	const shrineCornerOptions = [
		{ value: 'top-left', label: 'Top Left' },
		{ value: 'top-right', label: 'Top Right' },
		{ value: 'bottom-left', label: 'Bottom Left' },
		{ value: 'bottom-right', label: 'Bottom Right' },
	];

	const shrineOnTakeOptions = [
		{ value: 'green', label: 'Turn Green' },
		{ value: 'hide', label: 'Hide' },
	];

	const SVG_WIDTH = 900;
	const SVG_HEIGHT = 350;
	const PADDING = 40;

	// --- Plan summary ---
	let routeRoomCount = $derived(navState.plannedRoute.length);

	let routeDarkshrineCount = $derived.by(() => {
		const routeSet = new Set(navState.plannedRoute);
		let count = 0;
		for (const roomId of routeSet) {
			const room = navState.roomById.get(roomId);
			if (room?.contents.some((c) => c.toLowerCase() === 'darkshrine')) {
				count++;
			}
		}
		return count;
	});

	let routeHasArgus = $derived.by(() => {
		const routeSet = new Set(navState.plannedRoute);
		for (const roomId of routeSet) {
			const room = navState.roomById.get(roomId);
			if (room?.contents.some((c) => c.toLowerCase() === 'argus')) {
				return true;
			}
		}
		return false;
	});

	// --- Lifecycle ---
	$effect(() => {
		// Keep serverUrl in sync with store (reacts to debug toggle)
		if (store.status?.server_url) {
			serverUrl = store.status.server_url;
		}
		invoke<any>('get_status')
			.then((status) => {
				if (status?.server_url) {
					serverUrl = status.server_url;
				}
			})
			.catch((e) => console.error('[planner] get_status failed:', e));
		// Load saved preferences
		invoke<any>('get_compass_settings')
			.then((s) => {
				if (s?.strategy) strategyValue = s.strategy;
				if (s?.difficulty) difficulty = s.difficulty;
				if (s?.mode) compassMode = s.mode;
				if (s?.shrine_warn_enabled !== undefined) shrineEnabled = s.shrine_warn_enabled;
				if (s?.shrine_warn_size) shrineSize = s.shrine_warn_size;
				if (s?.shrine_warn_corner) shrineCorner = s.shrine_warn_corner;
				if (s?.shrine_warn_on_take) shrineOnTake = s.shrine_warn_on_take;
			})
			.catch(() => {});
	});

	// Fetch layout when serverUrl or difficulty changes
	$effect(() => {
		if (!serverUrl) return;
		const url = serverUrl;
		const diff = difficulty;
		fetchLayout(url, diff);
	});

	async function fetchLayout(url: string, diff: string) {
		loading = true;
		error = '';
		try {
			const res = await fetch(`${url}/api/lab/layout/${diff}`);
			if (!res.ok) {
				if (res.status === 404) {
					navState = createNavState();
					return;
				}
				throw new Error(`Server returned ${res.status}`);
			}
			const layout: LabLayout = await res.json();
			navState = loadLayout(navState, layout);
			// Notify overlays that layout changed
			invoke('emit_lab_nav', { eventJson: { type: 'LayoutChanged', difficulty: layout.difficulty } }).catch(() => {});
		} catch (e: any) {
			error = e?.message || 'Failed to fetch layout';
			navState = createNavState();
		} finally {
			loading = false;
		}
	}

	// --- Handlers ---
	function handleStrategyChange() {
		navState = setStrategy(navState, strategyValue as RouteStrategy);
		invoke('set_compass_strategy', { strategy: strategyValue }).catch(() => {});
		invoke('emit_lab_nav', { eventJson: { type: 'StrategyChanged', strategy: strategyValue } }).catch(() => {});
	}

	// --- Room editor ---
	let editingRoom = $state<import('$lib/compass/navigation').LabLayoutRoom | null>(null);

	function handleRoomRightClick(roomId: string) {
		const room = navState.roomById.get(roomId);
		if (room && room.name.toLowerCase() !== "aspirant's trial") {
			editingRoom = room;
		}
	}

	function handleEditorSave() {
		editingRoom = null;
		// Refetch layout to pick up changes
		if (serverUrl) fetchLayout(serverUrl, difficulty);
	}

	function handleDifficultyChange() {
		invoke('set_compass_difficulty', { difficulty }).catch(() => {});
		// Effect will refetch and emit LayoutChanged
	}

	function handleShrineChange() {
		invoke('set_shrine_warn', { enabled: shrineEnabled, size: shrineSize, corner: shrineCorner, onTake: shrineOnTake }).catch(() => {});
	}

	function handleModeChange() {
		invoke('set_compass_mode', { mode: compassMode }).catch((e) =>
			console.error('[planner] set_compass_mode failed:', e),
		);
	}


	function handleRoomClick(roomId: string) {
		const room = navState.roomById.get(roomId);
		if (room?.name.toLowerCase() === "aspirant's trial") return;
		navState = toggleTargetRoom(navState, roomId);
	}

	function triggerImport() {
		fileInput?.click();
	}

	async function handleImport(event: Event) {
		const file = (event.target as HTMLInputElement).files?.[0];
		if (!file) return;
		try {
			const text = await file.text();
			const json = JSON.parse(text);
			// Re-read server URL (may have changed via debug toggle)
			const status = await invoke<any>('get_status');
			const uploadUrl = status?.server_url || serverUrl;
			// POST to server
			const res = await fetch(`${uploadUrl}/api/lab/layout/${difficulty}`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: text,
			});
			if (!res.ok) {
				const body = await res.json().catch(() => ({}));
				error = body?.error || `Upload failed (${res.status})`;
			}
			// Reload layout from parsed JSON regardless of upload result
			navState = loadLayout(navState, json);
		} catch (e: any) {
			error = e?.message || 'Failed to import layout';
		}
		// Reset file input so re-importing the same file triggers change
		if (fileInput) fileInput.value = '';
	}

</script>

<div class="planner-page">
	<div class="planner-header">
		<h1>Lab Planner</h1>
		<div class="header-controls">
			<Select
				bind:value={difficulty}
				options={difficultyOptions}
				onchange={handleDifficultyChange}
			/>
			<button class="btn-import" onclick={triggerImport}>Import</button>
			<input
				bind:this={fileInput}
				type="file"
				accept=".json"
				class="file-input-hidden"
				onchange={handleImport}
			/>
		</div>
	</div>

	{#if loading}
		<div class="loading-state">
			<div class="loading-spinner"></div>
			<p>Loading layout...</p>
		</div>
	{:else if !navState.layout}
		<div class="empty-state">
			<div class="empty-icon">&#x1F5FA;</div>
			<p class="empty-message">
				No layout available for today's {difficultyOptions.find((d) => d.value === difficulty)?.label ?? difficulty} lab.
			</p>
			<button class="btn-import-large" onclick={triggerImport}>Import Layout</button>
			<p class="empty-hint">Download from poelab.com and import.</p>
		</div>
	{:else}
		<div class="planner-body">
			<div class="graph-column">
				<div class="graph-area">
					<LabGraph
						{navState}
						width={SVG_WIDTH}
						height={SVG_HEIGHT}
						padding={PADDING}
						nodeRadius={20}
						interactive={true}
						editingRoomId={editingRoom?.id ?? null}
						onRoomClick={handleRoomClick}
						onRoomRightClick={handleRoomRightClick}
					/>
				</div>

				{#if editingRoom}
					{#key editingRoom.id}
						<RoomEditor
							room={editingRoom}
							{serverUrl}
							{difficulty}
							onSave={handleEditorSave}
							onClose={() => { editingRoom = null; }}
						/>
					{/key}
				{/if}
			</div>

			<div class="config-panel">
				<div class="config-section">
					<label class="config-label" for="planner-strategy">Strategy</label>
					<Select
						id="planner-strategy"
						bind:value={strategyValue}
						options={strategyOptions}
						onchange={handleStrategyChange}
					/>
				</div>

				<div class="config-section">
					<label class="config-label" for="planner-compass-mode">Compass Mode</label>
					<Select
						id="planner-compass-mode"
						bind:value={compassMode}
						options={modeOptions}
						onchange={handleModeChange}
					/>
				</div>

				<div class="config-section">
					<button class="config-toggle" class:active={shrineEnabled} onclick={() => { shrineEnabled = !shrineEnabled; handleShrineChange(); }}>
						<span class="toggle-dot" class:on={shrineEnabled}></span>
						Darkshrine Remind
					</button>
					{#if shrineEnabled}
						<Select
							bind:value={shrineSize}
							options={shrineSizeOptions}
							onchange={handleShrineChange}
						/>
						<Select
							bind:value={shrineCorner}
							options={shrineCornerOptions}
							onchange={handleShrineChange}
						/>
						<Select
							bind:value={shrineOnTake}
							options={shrineOnTakeOptions}
							onchange={handleShrineChange}
						/>
					{/if}
				</div>

				<div class="config-section">
					<div class="summary-title">Plan Summary</div>
					<div class="summary-grid">
						<span class="summary-label">Rooms</span>
						<span class="summary-value">{routeRoomCount}</span>
						<span class="summary-label">Darkshrines</span>
						<span class="summary-value">{routeDarkshrineCount}</span>
						<span class="summary-label">Argus</span>
						<span class="summary-value" class:summary-yes={routeHasArgus} class:summary-no={!routeHasArgus}>
							{routeHasArgus ? 'Yes' : 'No'}
						</span>
					</div>
				</div>
			</div>
		</div>
	{/if}

	{#if error}
		<div class="error-bar">
			<span>{error}</span>
			<button class="error-dismiss" onclick={() => { error = ''; }}>Dismiss</button>
		</div>
	{/if}
</div>

<style>
	.planner-page {
		display: flex;
		flex-direction: column;
		gap: 0;
		height: 100%;
	}

	/* --- Header --- */
	.planner-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 10px 16px;
		margin-bottom: 12px;
	}

	.planner-header h1 {
		font-size: 1rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}

	.header-controls {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.btn-import {
		background: var(--color-lab-blue);
		border: none;
		color: #fff;
		padding: 6px 14px;
		font-size: 0.8125rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
	}

	.btn-import:hover {
		opacity: 0.9;
	}

	.file-input-hidden {
		display: none;
	}

	/* --- Body (graph + config) --- */
	.planner-body {
		display: flex;
		gap: 12px;
		flex: 1;
		min-height: 0;
	}

	.graph-column {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 8px;
		min-height: 0;
	}

	/* --- Graph area --- */
	.graph-area {
		flex: 1;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 12px;
		display: flex;
		align-items: center;
		justify-content: center;
		min-width: 0;
	}

	/* --- Config panel --- */
	.config-panel {
		width: 200px;
		flex-shrink: 0;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.config-section {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.config-label {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
		font-weight: 600;
	}

	.config-toggle {
		all: unset;
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
		font-weight: 600;
		cursor: pointer;
	}

	.config-toggle.active {
		color: var(--color-lab-text);
	}

	.toggle-dot {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--color-lab-border);
		flex-shrink: 0;
	}

	.toggle-dot.on {
		background: #4ade80;
		box-shadow: 0 0 4px rgba(74, 222, 128, 0.5);
	}


	.summary-title {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
		font-weight: 600;
		margin-bottom: 4px;
		padding-bottom: 4px;
		border-bottom: 1px solid var(--color-lab-border);
	}

	.summary-grid {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 4px 8px;
	}

	.summary-label {
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
	}

	.summary-value {
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--color-lab-text);
		text-align: right;
	}

	.summary-yes {
		color: #4ade80;
	}

	.summary-no {
		color: var(--color-lab-text-secondary);
	}

	/* --- Empty state --- */
	.empty-state {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 40px 20px;
		text-align: center;
	}

	.empty-icon {
		font-size: 2.5rem;
		margin-bottom: 12px;
		opacity: 0.4;
	}

	.empty-message {
		color: var(--color-lab-text);
		font-size: 0.9375rem;
		margin-bottom: 16px;
	}

	.btn-import-large {
		background: var(--color-lab-blue);
		border: none;
		color: #fff;
		padding: 10px 24px;
		font-size: 0.9375rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
		margin-bottom: 8px;
	}

	.btn-import-large:hover {
		opacity: 0.9;
	}

	.empty-hint {
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
	}

	/* --- Loading state --- */
	.loading-state {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 40px 16px;
		color: var(--color-lab-text-secondary);
		font-size: 1rem;
	}

	.loading-spinner {
		width: 32px;
		height: 32px;
		border: 3px solid var(--color-lab-border);
		border-top-color: var(--color-lab-blue);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
		margin-bottom: 16px;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}

	/* --- Error bar --- */
	.error-bar {
		background: rgba(239, 68, 68, 0.1);
		border: 1px solid rgba(239, 68, 68, 0.3);
		padding: 8px 16px;
		margin-top: 12px;
		display: flex;
		align-items: center;
		justify-content: space-between;
		font-size: 0.8125rem;
		color: var(--color-lab-red);
	}

	.error-dismiss {
		background: transparent;
		border: 1px solid rgba(239, 68, 68, 0.4);
		color: var(--color-lab-red);
		padding: 4px 10px;
		font-size: 0.75rem;
		cursor: pointer;
		font-family: inherit;
	}

	.error-dismiss:hover {
		background: rgba(239, 68, 68, 0.15);
	}
</style>
