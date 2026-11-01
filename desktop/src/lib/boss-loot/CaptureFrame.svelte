<script lang="ts">
	let {
		mode,
		rect,
		boss,
		run,
		area,
		level,
		trackedBosses,
		previewShortcut
	}: {
		mode: 'preview' | 'capture' | 'refused'
		/** CSS coordinates converted from the native crop by the host's scale factor. */
		rect: { x: number; y: number; width: number; height: number }
		boss: string
		run: number
		area: string
		level: number | string
		trackedBosses: string[]
		previewShortcut: string
	} = $props()
</script>

<div class="capture-frame" class:refused={mode === 'refused'} class:reading={mode === 'capture'}
	style:left="{rect.x}px" style:top="{rect.y}px" style:width="{rect.width}px" style:height="{rect.height}px">
	{#if mode !== 'capture'}
		{#each ['top-left', 'top-right', 'bottom-left', 'bottom-right'] as corner (corner)}
			<i class="corner {corner}" aria-hidden="true"></i>
		{/each}
	{/if}
	<div class="chip" role="status">
		{#if mode === 'preview'}
			<b>Boss loot</b><span class="secondary">preview</span>
			{#each previewShortcut.split(' + ') as key, index (index)}
				{#if index > 0}<span>+</span>{/if}<kbd>{key}</kbd>
			{/each}
			<span class="secondary">hides</span>
		{:else if mode === 'capture'}
			<i class="spinner" aria-hidden="true"></i><span>Reading loot · {boss} · run {run}</span>
		{:else}
			<div><b class="warning">Not logged</b> · no boss profile for this zone</div>
			<div class="detail">Last zone <span class="mono">{area}</span>, level {level}. Boss loot tracks: {trackedBosses.join(', ')}.</div>
		{/if}
	</div>
</div>

<style>
	.capture-frame { position: absolute; box-sizing: border-box; pointer-events: none; user-select: none; color: var(--color-lab-cyan); font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; }
	.capture-frame.refused { color: var(--color-lab-yellow); }
	.reading { box-shadow: inset 0 0 0 3px var(--color-lab-cyan); border-radius: 2px; }
	.corner { position: absolute; width: 48px; height: 48px; border: 0 solid currentColor; }
	.top-left { top: 0; left: 0; border-top-width: 3px; border-left-width: 3px; }
	.top-right { top: 0; right: 0; border-top-width: 3px; border-right-width: 3px; }
	.bottom-left { bottom: 0; left: 0; border-bottom-width: 3px; border-left-width: 3px; }
	.bottom-right { bottom: 0; right: 0; border-bottom-width: 3px; border-right-width: 3px; }
	.chip { position: absolute; top: 12px; left: 12px; display: flex; align-items: center; gap: 8px; padding: 5px 10px; border: 1px solid var(--color-lab-border); border-radius: 4px; background: var(--color-lab-bg); color: var(--color-lab-text); font-size: 13px; }
	.refused .chip { display: block; }
	.secondary, .detail { color: var(--color-lab-text-secondary); }
	.detail { margin-top: 2px; font-size: 12px; }
	.warning { color: var(--color-lab-yellow); }
	.mono, kbd { font-family: 'Consolas', 'Monaco', monospace; }
	kbd { padding: 1px 6px; border: 1px solid var(--color-lab-text-muted); border-bottom-width: 2px; border-radius: 3px; background: var(--color-lab-surface); font-size: 11px; }
	.spinner { width: 12px; height: 12px; border: 2px solid var(--color-lab-border); border-top-color: var(--color-lab-blue); border-radius: 50%; animation: spin 1s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
