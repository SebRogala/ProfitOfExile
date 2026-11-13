import raw from './notice.json';

export interface NoticeSection {
	id: string;
	heading: string;
	paragraphs: string[];
	items: string[];
}

export interface NoticeChange {
	revision: number;
	date: string;
	items: string[];
}

export interface Notice {
	revision: number;
	date: string;
	draft: boolean;
	changes: NoticeChange[];
	sections: NoticeSection[];
}

/** The bundled Data & Privacy notice (ADR-033); `notice.json` mirrors the website's copy. */
export const notice: Notice = raw;

/** The change entries newer than `acceptedRevision`, newest first. */
export function changesSince(source: Notice, acceptedRevision: number): NoticeChange[] {
	return source.changes
		.filter((c) => c.revision > acceptedRevision)
		.sort((a, b) => b.revision - a.revision);
}
