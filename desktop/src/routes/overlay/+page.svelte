<script lang="ts">
	// Render the read-only OCR region preview opened by Settings.
	const query = new URLSearchParams(window.location.search);
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
{/if}

<style>
	:global(html), :global(body) {
		margin: 0;
		padding: 0;
		background: transparent !important;
		overflow: hidden;
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
</style>
