<script lang="ts">
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

	async function startDrag(e: MouseEvent) {
		const win = getCurrentWebviewWindow();
		await win.startDragging();
	}
</script>

<div class="overlay" role="presentation" onmousedown={startDrag}>
	<div class="label">Gem Name Region — drag to move, resize edges, then Save in main window</div>
</div>

<style>
	:global(body) {
		margin: 0;
		padding: 0;
		background: transparent;
		overflow: hidden;
	}

	.overlay {
		width: 100vw;
		height: 100vh;
		border: 3px solid #e94560;
		background: rgba(233, 69, 96, 0.15);
		cursor: move;
		display: flex;
		align-items: flex-end;
		justify-content: center;
		box-sizing: border-box;
	}

	.label {
		background: rgba(0, 0, 0, 0.8);
		color: #e94560;
		font-size: 11px;
		padding: 2px 8px;
		font-family: -apple-system, sans-serif;
		pointer-events: none;
		white-space: nowrap;
	}
</style>
