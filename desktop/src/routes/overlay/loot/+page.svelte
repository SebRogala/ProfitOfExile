<script lang="ts">
	/**
	 * Loot capture overlay — PROOF OF CONCEPT (2026-10-27), display-only.
	 *
	 * The `loot-capture` window is built, moved and sized by Rust
	 * (`src-tauri/src/loot_capture.rs`) to the capture rectangle itself, so this
	 * page only draws a frame filling the window plus the readout. What to draw
	 * comes from polling `loot_capture_view` (cross-window events are unreliable
	 * in WebView2, see docs/OVERLAY-GUIDE.md). No hot rects: every click goes to
	 * the game.
	 */
	import { invoke } from '@tauri-apps/api/core';

	type LootCaptureView = { visible: boolean; preview: boolean; lines: string[] };

	const POLL_MS = 100;

	let view = $state<LootCaptureView>({ visible: false, preview: false, lines: [] });

	$effect(() => {
		let failures = 0;
		const timer = setInterval(async () => {
			try {
				view = await invoke<LootCaptureView>('loot_capture_view');
				failures = 0;
			} catch (error) {
				// Logged once per run of failures, not once per 100 ms poll.
				if (failures++ === 0) console.error('[loot] loot_capture_view failed:', error);
			}
		}, POLL_MS);
		return () => clearInterval(timer);
	});
</script>

{#if view.visible}
	<div class="frame" class:preview={view.preview && view.lines.length === 0}>
		{#if view.lines.length > 0}
			<div class="readout">
				{#each view.lines as line, index (index)}
					<div class:unknown={line.startsWith('unknown:')}>{line}</div>
				{/each}
			</div>
		{/if}
	</div>
{/if}

<style>
	:global(html), :global(body) {
		margin: 0;
		padding: 0;
		background: transparent !important;
		overflow: hidden;
	}

	.frame {
		position: fixed;
		inset: 0;
		box-sizing: border-box;
		border: 3px solid #e94560;
		pointer-events: none;
		user-select: none;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
	}

	.frame.preview {
		border-style: dashed;
		border-color: rgba(233, 69, 96, 0.7);
	}

	.readout {
		position: absolute;
		top: 8px;
		left: 8px;
		max-width: calc(100% - 16px);
		padding: 6px 10px;
		border-radius: 3px;
		background: rgba(17, 17, 17, 0.85);
		color: #fff;
		font-size: 14px;
		font-weight: 600;
		line-height: 1.4;
	}

	.readout .unknown {
		color: #f0b35a;
		font-weight: 400;
	}
</style>
