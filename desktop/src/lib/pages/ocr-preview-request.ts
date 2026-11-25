import type { OcrPreviewRequest } from '$lib/overlay/ocr-preview';

export type RegionKey = 'lab_gem' | 'lab_font' | 'lab_gem_slots' | 'temple_panel' | 'merc_panel';
export type RegionReadState = 'read' | 'clipped' | 'past-edge' | 'unset';

export type OcrRectView = {
	key: string;
	label: string;
	rect: [number, number, number, number] | null;
	rows: [number, number, number, number][];
	source: string;
	regionStates?: Partial<Record<RegionKey, RegionReadState>>;
};

type OcrPreviewOwner = {
	destroy: () => Promise<void>;
	preview: (request: OcrPreviewRequest) => Promise<void>;
};

export type OcrPreviewRequestDependencies = {
	owner: OcrPreviewOwner;
	fetchRects: () => Promise<OcrRectView[]>;
	setRects: (rows: OcrRectView[]) => void;
	warn: (...args: unknown[]) => void;
	error: (...args: unknown[]) => void;
};

export async function requestOcrPreview(
	key: string,
	dependencies: OcrPreviewRequestDependencies
): Promise<void> {
	await dependencies.owner.destroy();

	try {
		const rows = await dependencies.fetchRects();
		dependencies.setRects(rows);
		const row = rows.find((item) => item.key === key);
		if (!row?.rect) {
			dependencies.warn(`[settings] OCR preview '${key}' is unlocated`);
			return;
		}
		await dependencies.owner.preview({
			key,
			label: row.label,
			rect: row.rect,
			rows: row.rows
		});
	} catch (error) {
		dependencies.error('[settings] OCR preview failed:', error);
	}
}
