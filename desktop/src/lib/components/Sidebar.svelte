<script lang="ts">
	let {
		open = true,
		currentPath = '/lab',
	}: {
		open: boolean;
		currentPath: string;
	} = $props();
</script>

{#if open}
<nav class="sidebar">
	<div class="top">
		<div class="section">
			<div class="label">Strategies</div>
			<a href="/lab" class="nav-item" class:active={currentPath === '/lab'}>
				<span class="icon">&#x2697;&#xFE0F;</span>
				<span>Lab Farming</span>
			</a>
			<div class="nav-item disabled">
				<span class="icon">&#x1F5FA;&#xFE0F;</span>
				<span>Mapping</span>
				<span class="badge">soon</span>
			</div>
			<div class="nav-item disabled">
				<span class="icon">&#x1F479;</span>
				<span>Bosses</span>
				<span class="badge">soon</span>
			</div>
		</div>

		<div class="section">
			<div class="label">Tools</div>
			<div class="nav-item disabled">
				<span class="icon">&#x1F50D;</span>
				<span>Trade Lookup</span>
			</div>
			<div class="nav-item disabled">
				<span class="icon">&#x1F4CA;</span>
				<span>Price Compare</span>
			</div>
		</div>
	</div>

	<div class="bottom">
		<div class="label">Overlays</div>
		<div class="overlay-row">
			<span>&#x1F9ED; Compass</span>
			<span class="mode off">off</span>
		</div>
		<div class="overlay-row">
			<span>&#x1F441; OCR</span>
			<span class="mode off">off</span>
		</div>
		<div class="overlay-row">
			<span>&#x2696;&#xFE0F; Compare</span>
			<span class="mode off">off</span>
		</div>
		<div class="overlay-row">
			<span>&#x1F4CB; Session</span>
			<span class="mode off">off</span>
		</div>
	</div>
</nav>
{/if}

<style>
	.sidebar {
		width: 180px;
		flex-shrink: 0;
		background: var(--surface);
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		justify-content: space-between;
		overflow-y: auto;
	}

	.top {
		flex: 1;
		overflow-y: auto;
		padding: 10px 8px;
	}

	.section {
		margin-bottom: 16px;
	}

	.label {
		font-size: 9px;
		text-transform: uppercase;
		letter-spacing: 1px;
		color: var(--text-muted);
		padding: 0 10px;
		margin-bottom: 4px;
	}

	.nav-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		font-size: 12px;
		border-radius: 5px;
		color: var(--text);
		text-decoration: none;
		cursor: pointer;
	}

	.nav-item:hover:not(.disabled) {
		background: var(--border);
	}

	.nav-item.active {
		background: var(--border);
		color: var(--accent);
	}

	.nav-item.disabled {
		color: var(--border);
		cursor: default;
	}

	.badge {
		font-size: 9px;
		color: var(--text-muted);
		margin-left: auto;
		opacity: 0.6;
	}

	.icon {
		flex-shrink: 0;
		width: 16px;
		text-align: center;
	}

	.bottom {
		border-top: 1px solid var(--border);
		padding: 8px;
	}

	.bottom .label {
		margin-bottom: 6px;
	}

	.overlay-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		background: var(--bg);
		padding: 4px 8px;
		margin-bottom: 3px;
		border-radius: 4px;
		font-size: 11px;
	}

	.mode {
		font-size: 10px;
	}

	.mode.off {
		color: var(--text-muted);
	}

	.mode.always {
		color: var(--success);
	}

	.mode.auto {
		color: var(--warning);
	}
</style>
