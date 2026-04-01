<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import Select from '$lib/components/Select.svelte';
	import {
		createNavState,
		loadLayout,
		setStrategy,
		toggleTargetRoom,
		type NavState,
		type LabLayout,
		type LabLayoutRoom,
		type RouteStrategy,
	} from '$lib/compass/navigation';

	// --- State ---
	let navState = $state<NavState>(createNavState());
	let difficulty = $state('uber');
	let strategyValue = $state<string>('darkshrines');
	let compassMode = $state('minimap');
	let serverUrl = $state('');
	let loading = $state(false);
	let error = $state('');
	let fileInput = $state<HTMLInputElement | null>(null);

	const difficultyOptions = [
		{ value: 'uber', label: 'Uber Lab' },
		{ value: 'merciless', label: 'Merciless' },
		{ value: 'cruel', label: 'Cruel' },
		{ value: 'normal', label: 'Normal' },
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

	// --- SVG graph computed values ---
	const SVG_WIDTH = 800;
	const SVG_HEIGHT = 300;
	const PADDING = 30;

	interface RoomNode {
		room: LabLayoutRoom;
		cx: number;
		cy: number;
		onRoute: boolean;
		isTrial: boolean;
		contentColor: string | null;
	}

	interface Edge {
		x1: number;
		y1: number;
		x2: number;
		y2: number;
		onRoute: boolean;
		key: string;
	}

	let roomNodes = $derived.by(() => {
		if (!navState.layout) return [];
		const rooms = navState.layout.rooms;
		if (rooms.length === 0) return [];

		const xs = rooms.map((r) => parseFloat(r.x));
		const ys = rooms.map((r) => parseFloat(r.y));
		const minX = Math.min(...xs);
		const maxX = Math.max(...xs);
		const minY = Math.min(...ys);
		const maxY = Math.max(...ys);
		const rangeX = maxX - minX || 1;
		const rangeY = maxY - minY || 1;

		const routeSet = new Set(navState.plannedRoute);

		return rooms.map((room): RoomNode => {
			const rawX = parseFloat(room.x);
			const rawY = parseFloat(room.y);
			const cx = PADDING + ((rawX - minX) / rangeX) * (SVG_WIDTH - 2 * PADDING);
			const cy = PADDING + ((rawY - minY) / rangeY) * (SVG_HEIGHT - 2 * PADDING);
			const isTrial = room.name.toLowerCase() === "aspirant's trial";
			const contentsLower = room.contents.map((c) => c.toLowerCase());

			let contentColor: string | null = null;
			if (contentsLower.includes('darkshrine')) contentColor = '#a855f7';
			else if (contentsLower.includes('argus')) contentColor = '#4ade80';
			else if (contentsLower.some((c) => c.includes('golden-key'))) contentColor = '#fbbf24';
			else if (contentsLower.some((c) => c.includes('puzzle'))) contentColor = '#60a5fa';
			else if (contentsLower.some((c) => c.includes('silver'))) contentColor = '#9ca3af';

			return {
				room,
				cx,
				cy,
				onRoute: routeSet.has(room.id),
				isTrial,
				contentColor,
			};
		});
	});

	let edges = $derived.by(() => {
		if (!navState.layout || roomNodes.length === 0) return [];
		const nodeMap = new Map<string, RoomNode>();
		for (const node of roomNodes) {
			nodeMap.set(node.room.id, node);
		}

		const seen = new Set<string>();
		const result: Edge[] = [];
		const routeEdges = new Set<string>();

		// Build route edge set
		for (let i = 0; i < navState.plannedRoute.length - 1; i++) {
			const pair = [navState.plannedRoute[i], navState.plannedRoute[i + 1]].sort().join('|');
			routeEdges.add(pair);
		}

		for (const room of navState.layout.rooms) {
			for (const targetId of Object.values(room.exits)) {
				const pair = [room.id, targetId].sort();
				const key = pair.join('|');
				if (seen.has(key)) continue;
				seen.add(key);

				const from = nodeMap.get(room.id);
				const to = nodeMap.get(targetId);
				if (!from || !to) continue;

				result.push({
					x1: from.cx,
					y1: from.cy,
					x2: to.cx,
					y2: to.cy,
					onRoute: routeEdges.has(key),
					key,
				});
			}
		}

		return result;
	});

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
		invoke<any>('get_status')
			.then((status) => {
				if (status?.server_url) {
					serverUrl = status.server_url;
				}
			})
			.catch((e) => console.error('[planner] get_status failed:', e));
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
			navState = loadLayout(createNavState(), layout);
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
	}

	function handleDifficultyChange() {
		// Effect will refetch
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
			// POST to server
			await fetch(`${serverUrl}/api/lab/layout/${difficulty}`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: text,
			});
			// Reload layout from parsed JSON
			navState = loadLayout(createNavState(), json);
		} catch (e: any) {
			error = e?.message || 'Failed to import layout';
		}
		// Reset file input so re-importing the same file triggers change
		if (fileInput) fileInput.value = '';
	}

	function roomTooltip(room: LabLayoutRoom): string {
		const parts = [room.name];
		if (room.contents.length > 0) {
			parts.push(room.contents.join(', '));
		}
		return parts.join('\n');
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
			<div class="graph-area">
				<svg
					viewBox="0 0 {SVG_WIDTH} {SVG_HEIGHT}"
					class="lab-graph"
					xmlns="http://www.w3.org/2000/svg"
				>
					<!-- Connection lines (background) -->
					{#each edges as edge (edge.key)}
						<line
							x1={edge.x1}
							y1={edge.y1}
							x2={edge.x2}
							y2={edge.y2}
							class="edge"
							class:edge-route={edge.onRoute}
						/>
					{/each}

					<!-- Route highlight overlay -->
					{#each edges.filter((e) => e.onRoute) as edge (edge.key + '-route')}
						<line
							x1={edge.x1}
							y1={edge.y1}
							x2={edge.x2}
							y2={edge.y2}
							class="edge-highlight"
						/>
					{/each}

					<!-- Room nodes -->
					{#each roomNodes as node (node.room.id)}
						<g
							class="room-group"
							class:off-route={!node.onRoute}
							class:is-trial={node.isTrial}
							onclick={() => handleRoomClick(node.room.id)}
						>
							<!-- Outer circle -->
							<circle
								cx={node.cx}
								cy={node.cy}
								r="14"
								class="room-circle"
								class:room-trial={node.isTrial}
								class:room-target={navState.targetRooms.includes(node.room.id)}
							/>

							<!-- Content indicator -->
							{#if node.contentColor}
								<circle
									cx={node.cx}
									cy={node.cy}
									r="5"
									fill={node.contentColor}
								/>
							{/if}

							<!-- Trial label -->
							{#if node.isTrial}
								<text
									x={node.cx}
									y={node.cy + 4}
									class="trial-label"
								>I</text>
							{/if}

							<!-- Room name label -->
							<text
								x={node.cx}
								y={node.cy + 24}
								class="room-name"
							>{node.room.name}</text>

							<title>{roomTooltip(node.room)}</title>
						</g>
					{/each}
				</svg>
			</div>

			<div class="config-panel">
				<div class="config-section">
					<label class="config-label">Strategy</label>
					<Select
						bind:value={strategyValue}
						options={strategyOptions}
						onchange={handleStrategyChange}
					/>
				</div>

				<div class="config-section">
					<label class="config-label">Compass Mode</label>
					<Select
						bind:value={compassMode}
						options={modeOptions}
						onchange={handleModeChange}
					/>
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

	.lab-graph {
		width: 100%;
		height: auto;
		max-height: 100%;
	}

	/* Edges */
	.edge {
		stroke: #374151;
		stroke-width: 1.5;
	}

	.edge-route {
		stroke: #374151;
		stroke-width: 1.5;
	}

	.edge-highlight {
		stroke: #10b981;
		stroke-width: 4;
		opacity: 0.5;
		stroke-linecap: round;
	}

	/* Room nodes */
	.room-group {
		cursor: pointer;
	}

	.room-group.off-route {
		opacity: 0.35;
	}

	.room-group.is-trial {
		cursor: default;
	}

	.room-circle {
		fill: #1f2937;
		stroke: #4b5563;
		stroke-width: 2;
	}

	.room-circle.room-trial {
		stroke: #f59e0b;
		stroke-width: 2.5;
	}

	.room-circle.room-target {
		stroke: #10b981;
		stroke-width: 2.5;
	}

	.trial-label {
		fill: #f59e0b;
		font-size: 12px;
		font-weight: 700;
		text-anchor: middle;
		pointer-events: none;
	}

	.room-name {
		fill: var(--color-lab-text-secondary);
		font-size: 7px;
		text-anchor: middle;
		pointer-events: none;
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
