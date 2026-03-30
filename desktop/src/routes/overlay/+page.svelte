<script lang="ts">
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
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

	onMount(async () => {
		const win = getCurrentWebviewWindow();
		try {
			const size = await win.outerSize();
			await win.setSize({ type: 'Physical', width: size.width + 1, height: size.height + 1 });
			await win.setSize({ type: 'Physical', width: size.width, height: size.height });
		} catch (e) {
			console.error('Overlay transparency workaround failed:', e);
		}

		// Sync target window position in real-time
		let syncInterval: ReturnType<typeof setInterval> | undefined;
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
	<div class="label">Drag to move · Resize from edges</div>
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
		bottom: 5px;
		left: 50%;
		transform: translateX(-50%);
		background: #e94560;
		color: white;
		font-size: 11px;
		padding: 2px 8px;
		font-family: -apple-system, sans-serif;
		pointer-events: none;
		white-space: nowrap;
		border-radius: 3px;
	}
</style>
