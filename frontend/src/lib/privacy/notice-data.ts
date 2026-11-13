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

export const notice: Notice = raw;
