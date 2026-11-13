/**
 * PrivacyGate, rendered with `svelte/server`: which notice the blocking modal
 * shows for a given accepted revision. Interaction (Escape, focus trap,
 * Continue/Quit) is checked by the WI-5 manual READBACK; desktop has no DOM
 * client-render environment.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => new Promise(() => {})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import PrivacyGate from './PrivacyGate.svelte';
import { resetNoticeGateForTests } from '$lib/privacy/gate.svelte';
import { notice as bundled, type Notice } from '$lib/privacy/notice';

const threeRevisions: Notice = {
	revision: 3,
	date: '2026-12-01',
	draft: false,
	changes: [
		{ revision: 1, date: '2026-11-13', items: ['change-one'] },
		{ revision: 2, date: '2026-11-20', items: ['change-two'] },
		{ revision: 3, date: '2026-12-01', items: ['change-three'] }
	],
	sections: [{ id: 'controller', heading: 'Who is responsible', paragraphs: ['section-body'], items: [] }]
};

function body(props: { notice?: Notice; acceptedRevision?: number | null }): string {
	return render(PrivacyGate, { props }).body;
}

describe('PrivacyGate', () => {
	beforeEach(() => {
		resetNoticeGateForTests();
	});

	it('first run renders the full notice sections', () => {
		const html = body({ acceptedRevision: null });
		expect(html).toContain('aria-modal="true"');
		for (const s of bundled.sections) expect(html).toContain(`data-section="${s.id}"`);
		expect(html).not.toContain('Data &amp; privacy changes');
	});

	it('a bumped revision renders the changes modal with exactly the newer entries', () => {
		const html = body({ notice: threeRevisions, acceptedRevision: 1 });
		expect(html).toContain('Data &amp; privacy changes');
		expect(html).toContain('change-three');
		expect(html).toContain('change-two');
		expect(html).not.toContain('change-one');
		expect(html.indexOf('data-revision="3"')).toBeLessThan(html.indexOf('data-revision="2"'));
	});

	it('the changes modal keeps the full notice collapsed', () => {
		expect(body({ notice: threeRevisions, acceptedRevision: 1 })).not.toContain('section-body');
	});

	it('accepted current revision renders no modal', () => {
		expect(body({ notice: threeRevisions, acceptedRevision: 3 })).not.toContain('role="dialog"');
	});

	it('an accepted gate renders no modal even on first run', async () => {
		const { acceptNoticeForTests } = await import('$lib/privacy/gate.svelte');
		acceptNoticeForTests();
		expect(body({ acceptedRevision: null })).not.toContain('role="dialog"');
	});

	it('the Draft badge shows only on a draft notice', () => {
		expect(body({ notice: { ...threeRevisions, draft: true }, acceptedRevision: null })).toContain('class="draft');
		expect(body({ notice: threeRevisions, acceptedRevision: null })).not.toContain('class="draft');
	});
});
