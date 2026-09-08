<script lang="ts">
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { PhysicalSize } from '@tauri-apps/api/dpi';
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';

	// If ?sync=<label>, move that window to match this one in real-time
	const query = new URLSearchParams(window.location.search);
	const syncTarget = query.get('sync');
	const previewKey = query.get('preview');
	const previewLabel = query.get('label') || previewKey || 'OCR region';

	function parsePreviewRect(raw: string | null): [number, number, number, number] | null {
		const values = raw?.split(',').map(Number);
		if (!values || values.length !== 4 || values.some((value) => !Number.isFinite(value))) return null;
		return values as [number, number, number, number];
	}

	type PreviewRow = [number, number, number, number];

	function parsePreviewRows(raw: string | null): PreviewRow[] {
		if (!raw) return [];
		try {
			const parsed: unknown = JSON.parse(raw);
			if (!Array.isArray(parsed)) return [];
			return parsed.flatMap((row): PreviewRow[] => {
				if (!Array.isArray(row) || row.length !== 4) return [];
				if (row.some((value) => typeof value !== 'number' || !Number.isFinite(value))) return [];
				return [[row[0] as number, row[1] as number, row[2] as number, row[3] as number]];
			});
		} catch (_) {
			return [];
		}
	}

	function parsePreviewDpr(raw: string | null): number {
		if (raw === null) return 1;
		const value = Number(raw);
		return Number.isFinite(value) && value > 0 ? value : 1;
	}

	const previewRect = parsePreviewRect(query.get('rect'));
	const previewRows = parsePreviewRows(query.get('rows'));
	const previewDpr = parsePreviewDpr(query.get('dpr'));
	const previewNumbers = previewRect ? `[${previewRect.join(', ')}]` : 'unlocated';

	function previewRowStyle(row: PreviewRow): string {
		const originX = previewRect?.[0] ?? 0;
		const originY = previewRect?.[1] ?? 0;
		return [
			`left: ${(row[0] - originX) / previewDpr}px`,
			`top: ${(row[1] - originY) / previewDpr}px`,
			`width: ${row[2] / previewDpr}px`,
			`height: ${row[3] / previewDpr}px`,
		].join('; ');
	}

	const BASE_EDGE = 10;

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
		// Scale edge zone for small windows — ensures enough drag area
		const edge = Math.min(BASE_EDGE, Math.floor(h / 8), Math.floor(w / 8));
		const t = e.clientY < edge;
		const b = e.clientY > h - edge;
		const l = e.clientX < edge;
		const r = e.clientX > w - edge;

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
		if (previewKey) return;
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


{#if previewKey}
	<div class="preview" role="img" aria-label={`${previewLabel} OCR region preview`}>
		<div class="border-top"></div>
		<div class="border-bottom"></div>
		<div class="border-left"></div>
		<div class="border-right"></div>
		{#each previewRows as row, index}
			<div class="preview-row" style={previewRowStyle(row)}>
				<span>{index}</span>
			</div>
		{/each}
		<div class="preview-label">
			<span>{previewLabel}</span>
			<span>{previewNumbers}</span>
		</div>
	</div>
{:else}
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
{/if}

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

	.preview {
		width: 100vw;
		height: 100vh;
		background: transparent;
		position: relative;
		box-sizing: border-box;
		pointer-events: none;
		user-select: none;
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

	.preview-label {
		position: absolute;
		top: 8px;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		gap: 8px;
		align-items: center;
		padding: 4px 8px;
		border-radius: 3px;
		background: rgba(17, 17, 17, 0.82);
		color: #fff;
		font-family: -apple-system, sans-serif;
		font-size: 11px;
		font-weight: 600;
		white-space: nowrap;
		z-index: 10;
	}

	.preview-row {
		position: absolute;
		border: 1px solid rgba(83, 211, 255, 0.9);
		box-sizing: border-box;
		pointer-events: none;
	}

	.preview-row span {
		position: absolute;
		top: 0;
		left: 0;
		padding: 1px 3px;
		background: rgba(17, 17, 17, 0.82);
		color: #53d3ff;
		font-family: -apple-system, sans-serif;
		font-size: 9px;
		font-weight: 600;
		line-height: 11px;
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
