import { describe, expect, it } from 'vitest';
import { changesSince, type Notice } from './notice';

const fixture: Notice = {
	revision: 3,
	date: '2026-12-01',
	draft: false,
	changes: [
		{ revision: 1, date: '2026-11-13', items: ['first'] },
		{ revision: 3, date: '2026-12-01', items: ['third'] },
		{ revision: 2, date: '2026-11-20', items: ['second'] }
	],
	sections: []
};

describe('changesSince', () => {
	it('lists only entries newer than the accepted revision, newest first', () => {
		expect(changesSince(fixture, 1).map((c) => c.revision)).toEqual([3, 2]);
	});

	it('excludes the entry at the accepted revision itself', () => {
		expect(changesSince(fixture, 2).map((c) => c.revision)).toEqual([3]);
	});

	it('lists every entry when the accepted revision is 0', () => {
		expect(changesSince(fixture, 0).map((c) => c.revision)).toEqual([3, 2, 1]);
	});

	it('lists nothing when the current revision is accepted', () => {
		expect(changesSince(fixture, 3)).toEqual([]);
	});
});
