<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { store } from '$lib/stores/status.svelte';
	import Select from '$lib/components/Select.svelte';

	let runs = $state<any[]>([]);
	let stats = $state<{ avg_seconds: number; best_seconds: number; avg_kill_seconds: number; best_kill_seconds: number; total_runs: number }>({ avg_seconds: 0, best_seconds: 0, avg_kill_seconds: 0, best_kill_seconds: 0, total_runs: 0 });
	let difficulty = $state('');
	let loading = $state(false);
	let error = $state('');

	const difficultyOptions = [
		{ value: '', label: 'All Difficulties' },
		{ value: 'Uber', label: 'Uber Lab' },
		{ value: 'Merciless', label: 'Merciless' },
		{ value: 'Cruel', label: 'Cruel' },
		{ value: 'Normal', label: 'Normal' },
	];

	function formatTime(seconds: number): string {
		const m = Math.floor(seconds / 60);
		const s = seconds % 60;
		return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
	}

	function formatDate(iso: string): string {
		const d = new Date(iso);
		return d.toLocaleDateString('en-GB', { day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit' });
	}

	async function fetchRuns() {
		const serverUrl = store.status?.server_url;
		const deviceId = store.status?.device_id;
		if (!serverUrl || !deviceId) {
			error = 'Waiting for server connection...';
			return;
		}

		loading = true;
		error = '';
		try {
			const params = new URLSearchParams({ limit: '50' });
			if (difficulty) params.set('difficulty', difficulty);
			const res = await fetch(`${serverUrl}/api/lab/runs?${params}`, {
				headers: {
					'X-Device-ID': deviceId,
					'X-App-Version': store.status?.app_version ?? '',
				},
			});
			if (!res.ok) throw new Error(`Server returned ${res.status}`);
			const data = await res.json();
			runs = data.runs ?? [];
			stats = data.stats ?? { avg_seconds: 0, best_seconds: 0, avg_kill_seconds: 0, best_kill_seconds: 0, total_runs: 0 };
		} catch (e: any) {
			error = e?.message || 'Failed to fetch runs';
			runs = [];
			stats = { avg_seconds: 0, best_seconds: 0, avg_kill_seconds: 0, best_kill_seconds: 0, total_runs: 0 };
		} finally {
			loading = false;
		}
	}

	// Fetch on mount and when filter changes
	$effect(() => {
		const _ = difficulty;
		const url = store.status?.server_url;
		if (url) fetchRuns();
	});
</script>

<div class="runs-page">
	<div class="runs-header">
		<h1>Run History</h1>
		<Select bind:value={difficulty} options={difficultyOptions} onchange={fetchRuns} />
	</div>

	{#if stats.total_runs > 0}
		<div class="stats-row">
			{#if stats.best_kill_seconds > 0}
				<div class="stat-card best">
					<div class="stat-value">{formatTime(stats.best_kill_seconds)}</div>
					<div class="stat-label">Best Kill</div>
				</div>
				<div class="stat-card">
					<div class="stat-value">{formatTime(Math.round(stats.avg_kill_seconds))}</div>
					<div class="stat-label">Avg Kill</div>
				</div>
			{:else}
				<div class="stat-card best">
					<div class="stat-value">{formatTime(stats.best_seconds)}</div>
					<div class="stat-label">Best Time</div>
				</div>
			{/if}
			<div class="stat-card">
				<div class="stat-value">{formatTime(Math.round(stats.avg_seconds))}</div>
				<div class="stat-label">Avg Total</div>
			</div>
			<div class="stat-card">
				<div class="stat-value">{stats.total_runs}</div>
				<div class="stat-label">Total Runs</div>
			</div>
		</div>
	{/if}

	{#if loading}
		<div class="loading">Loading...</div>
	{:else if error}
		<div class="error">{error}</div>
	{:else if runs.length === 0}
		<div class="empty">
			<p>No runs recorded yet.</p>
			<p class="hint">Complete a lab run with the timer overlay enabled to start tracking.</p>
		</div>
	{:else}
		<div class="table-wrap">
			<table>
				<thead>
					<tr>
						<th>Date</th>
						<th>Difficulty</th>
						<th>Strategy</th>
						<th>Kill</th>
						<th>Total</th>
						<th>Rooms</th>
						<th>Golden Door</th>
					</tr>
				</thead>
				<tbody>
					{#each runs as run}
						<tr>
							<td>{formatDate(run.started_at)}</td>
							<td>{run.difficulty}</td>
							<td>{run.strategy}</td>
							<td class="mono">{run.kill_seconds ? formatTime(run.kill_seconds) : '-'}</td>
							<td class="mono">{formatTime(run.elapsed_seconds)}</td>
							<td>{run.room_count}</td>
							<td>{run.has_golden_door ? 'Yes' : '-'}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>

<style>
	.runs-page {
		display: flex;
		flex-direction: column;
		gap: 12px;
		height: 100%;
	}

	.runs-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 10px 16px;
	}

	.runs-header h1 {
		font-size: 1rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}

	.stats-row {
		display: flex;
		gap: 10px;
	}

	.stat-card {
		flex: 1;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 12px;
		text-align: center;
	}

	.stat-card.best {
		border-color: #4ade80;
	}

	.stat-value {
		font-size: 1.25rem;
		font-weight: 700;
		color: var(--color-lab-text);
		font-family: 'Consolas', 'Monaco', monospace;
	}

	.stat-card.best .stat-value {
		color: #4ade80;
	}

	.stat-label {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
		margin-top: 4px;
	}

	.table-wrap {
		flex: 1;
		overflow-y: auto;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8125rem;
	}

	thead {
		position: sticky;
		top: 0;
		background: var(--color-lab-surface);
	}

	th {
		text-align: left;
		padding: 8px 10px;
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
		border-bottom: 1px solid var(--color-lab-border);
		font-weight: 600;
	}

	td {
		padding: 6px 10px;
		color: var(--color-lab-text);
		border-bottom: 1px solid var(--color-lab-border);
	}

	td.mono {
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 600;
	}

	tr:hover {
		background: rgba(255, 255, 255, 0.02);
	}

	.loading, .error, .empty {
		text-align: center;
		padding: 40px;
		color: var(--color-lab-text-secondary);
	}

	.error {
		color: var(--color-lab-text);
	}

	.empty .hint {
		font-size: 0.8125rem;
		margin-top: 8px;
		opacity: 0.6;
	}
</style>
