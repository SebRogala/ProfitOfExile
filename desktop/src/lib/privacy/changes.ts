import type { Notice, NoticeChange } from './notice';

/** The change entries newer than `acceptedRevision`, newest first. */
export function changesSince(source: Notice, acceptedRevision: number): NoticeChange[] {
	return source.changes
		.filter((c) => c.revision > acceptedRevision)
		.sort((a, b) => b.revision - a.revision);
}
