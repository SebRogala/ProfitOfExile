<script lang="ts">
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { PhysicalSize } from '@tauri-apps/api/dpi';
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';

	// If ?sync=<label>, move that window to match this one in real-time
	const syncTarget = new URLSearchParams(window.location.search).get('sync');

	const EDGE = 10;

	type Dir = 'East' | 'North' | 'NorthEast' | 'NorthWest' | 'South' | 'SouthEast' | 'SouthWest' | 'West';

	const CURSORS: Record<Dir, string> = {
		North: 'ns-resize', South: 'ns-resize',
		East: 'ew-resize', West: 'ew-resize',
		NorthEast: 'nesw-resize', SouthWest: 'nesw-resize',
		NorthWest: 'nwse-resize', SouthEast: 'nwse-resize',
	};

	function getEdge(e: MouseEvent): Dir | null {
		const w = window.innerWidth;
		const h = window.innerHeight;
		const t = e.clientY < EDGE;
		const b = e.clientY > h - EDGE;
		const l = e.clientX < EDGE;
		const r = e.clientX > w - EDGE;

		if (t && l) return 'NorthWest';
		if (t && r) return 'NorthEast';
		if (b && l) return 'SouthWest';
		if (b && r) return 'SouthEast';
		if (t) return 'North';
		if (b) return 'South';
		if (l) return 'West';
		if (r) return 'East';
		return null;
	}

	async function handleMouseDown(e: MouseEvent) {
		// Don't start drag/resize when clicking buttons.
		if ((e.target as HTMLElement).closest('.ctrl-btn')) return;
		const win = getCurrentWebviewWindow();
		const dir = getEdge(e);
		if (dir) {
			await win.startResizeDragging(dir);
		} else {
			await win.startDragging();
		}
	}

	function handleMouseMove(e: MouseEvent) {
		const dir = getEdge(e);
		document.body.style.cursor = dir ? CURSORS[dir] : 'move';
	}

	function handleSave() {
		getCurrentWebviewWindow().emit('overlay-save', {})
			.catch(err => console.error('[overlay] emit overlay-save failed:', err));
	}

	function handleCancel() {
		getCurrentWebviewWindow().emit('overlay-cancel', {})
			.catch(err => console.error('[overlay] emit overlay-cancel failed:', err));
	}

	let syncInterval: ReturnType<typeof setInterval> | undefined;

	onMount(() => {
		const win = getCurrentWebviewWindow();
		(async () => {
			try {
				const size = await win.outerSize();
				await win.setSize(new PhysicalSize(size.width + 1, size.height + 1));
				await win.setSize(new PhysicalSize(size.width, size.height));
			} catch (e) {
				console.error('Overlay transparency workaround failed:', e);
			}

			// Sync target window position in real-time
			if (syncTarget) {
				let lastX = 0, lastY = 0, lastW = 0, lastH = 0;
				syncInterval = setInterval(async () => {
					try {
						const pos = await win.outerPosition();
						const size = await win.outerSize();
						if (pos.x !== lastX || pos.y !== lastY || size.width !== lastW || size.height !== lastH) {
							lastX = pos.x; lastY = pos.y; lastW = size.width; lastH = size.height;
							await invoke('move_overlay', { label: syncTarget, x: pos.x, y: pos.y, w: size.width, h: size.height });
						}
					} catch (e) { console.warn(`[overlay] position sync failed for '${syncTarget}':`, e); }
				}, 100);
			}
		})().catch(e => console.error('[overlay] unexpected async error:', e));

		return () => {
			if (syncInterval) clearInterval(syncInterval);
		};
	});
</script>

<div class="overlay" role="presentation" onmousedown={handleMouseDown} onmousemove={handleMouseMove}>
	<div class="border-top"></div>
	<div class="border-bottom"></div>
	<div class="border-left"></div>
	<div class="border-right"></div>
	<div class="label">
		<button class="ctrl-btn save" onpointerup={handleSave}>Save</button>
		<button class="ctrl-btn cancel" onpointerup={handleCancel}>Cancel</button>
	</div>
</div>

<style>
	:global(html), :global(body) {
		margin: 0;
		padding: 0;
		background: transparent !important;
		overflow: hidden;
	}

	.overlay {
		width: 100vw;
		height: 100vh;
		background: transparent;
		cursor: move;
		position: relative;
		box-sizing: border-box;
	}

	.border-top, .border-bottom, .border-left, .border-right {
		position: absolute;
		background: #e94560;
		pointer-events: none;
	}
	.border-top { top: 0; left: 0; right: 0; height: 3px; }
	.border-bottom { bottom: 0; left: 0; right: 0; height: 3px; }
	.border-left { top: 0; bottom: 0; left: 0; width: 3px; }
	.border-right { top: 0; bottom: 0; right: 0; width: 3px; }

	.label {
		position: absolute;
		bottom: 8px;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		gap: 6px;
		align-items: center;
		font-family: -apple-system, sans-serif;
		white-space: nowrap;
		z-index: 10;
	}

	.ctrl-btn {
		padding: 4px 14px;
		border: none;
		border-radius: 3px;
		font-size: 11px;
		font-weight: 600;
		font-family: inherit;
		cursor: pointer;
		pointer-events: auto;
	}

	.ctrl-btn.save {
		background: #22c55e;
		color: #111;
	}

	.ctrl-btn.save:hover {
		background: #16a34a;
	}

	.ctrl-btn.cancel {
		background: #e94560;
		color: white;
	}

	.ctrl-btn.cancel:hover {
		background: #c53050;
	}
</style>
