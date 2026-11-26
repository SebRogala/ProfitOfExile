<script module lang="ts">
	export interface OverlayNoticeProps {
		kind: 'info' | 'warning' | 'error';
		header: string;
		body: string;
		keys?: string;
		progress?: boolean;
		reason?: string;
	}
</script>

<script lang="ts">
	let { kind, header, body, keys, progress = false, reason }: OverlayNoticeProps = $props();
	const keyOffset = $derived(keys ? body.indexOf(keys, body.startsWith('Press ') ? 'Press '.length : 0) : -1);
	const bodyParts = $derived(keys && keyOffset >= 0
		? [body.slice(0, keyOffset), body.slice(keyOffset + keys.length)]
		: [body]);
</script>

<div class="notice {kind}" data-kind={kind} role="status">
	<div class="head">
		{#if progress}
			<i class="spinner" aria-hidden="true"></i>
		{:else}
			<span class="glyph" aria-hidden="true">{kind === 'warning' ? '⚠' : kind === 'error' ? '✗' : '•'}</span>
		{/if}
		<span class="header" data-part="header">{header}</span>
	</div>
	<div class="body" data-part="body">{#each bodyParts as part, index}{#if index > 0 && keys}{#each keys.split(' + ') as key, keyIndex}{#if keyIndex > 0}{' + '}{/if}<kbd>{key}</kbd>{/each}{/if}{part}{/each}</div>
	{#if reason}<div class="reason">{reason}</div>{/if}
</div>

<style>
	.notice {
		box-sizing: border-box;
		width: max-content;
		max-width: 360px;
		min-height: 46px;
		padding: 5px 10px;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
		color: var(--color-lab-text);
		background: rgb(15 17 23 / 88%);
		border: 1px solid var(--color-lab-text-muted);
		border-radius: 6px;
		overflow: hidden;
		pointer-events: none;
	}
	.warning { border-color: rgb(234 179 8 / 45%); }
	.error { border-color: rgb(239 68 68 / 45%); }
	.head { display: flex; align-items: center; gap: 6px; font-size: 14px; font-weight: 600; line-height: 18px; }
	.header { min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
	.glyph { flex: none; font-weight: 700; }
	.info .glyph { color: var(--color-lab-text-secondary); }
	.warning .glyph { color: var(--color-lab-yellow); }
	.error .glyph { color: var(--color-lab-red); }
	.body {
		font-size: 13px;
		line-height: 16px;
		color: var(--color-lab-text-secondary);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		-webkit-box-orient: vertical;
		max-height: 32px;
		overflow: hidden;
	}
	.body:has(kbd) { line-height: 18px; max-height: 36px; padding-bottom: 2px; }
	kbd { font-family: 'Consolas', 'Monaco', monospace; font-size: 11px; line-height: 12px; padding: 0 5px; border: 1px solid var(--color-lab-text-muted); border-bottom-width: 2px; border-radius: 3px; background: var(--color-lab-surface); color: var(--color-lab-text); }
	.reason { margin-top: 2px; font: 11px/16px 'Consolas', 'Monaco', monospace; color: var(--color-lab-text-muted); }
	.spinner { box-sizing: border-box; flex: none; width: 12px; height: 12px; border: 2px solid var(--color-lab-border); border-top-color: var(--color-lab-blue); border-radius: 50%; animation: spin 1s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
