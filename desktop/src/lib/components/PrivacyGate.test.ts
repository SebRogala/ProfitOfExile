/**
 * PrivacyGate, rendered with `svelte/server`: which notice the blocking modal
 * shows for a given accepted revision. Interaction (Escape, focus trap,
 * Continue/Quit) is checked by the WI-6b Playwright READBACK; desktop has no DOM
 * client-render environment.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => new Promise(() => {})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import PrivacyGate from './PrivacyGate.svelte';
import PrivacyNoticeView from './PrivacyNoticeView.svelte';
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

	it('first run renders the ruled summary', () => {
		const html = body({ acceptedRevision: null });
		expect(html).toContain('aria-modal="true"');
		expect(html).toContain('ProfitOfExile reads your game screen on demand and sends game data to its server. No screenshot leaves your device.');
	});

	it('a bumped revision renders the current revision summary', () => {
		const html = body({ notice: threeRevisions, acceptedRevision: 1 });
		expect(html).toContain('Data &amp; privacy updated (revision 3).');
		expect(html).not.toContain('change-three');
		expect(html).not.toContain('change-two');
		expect(html).not.toContain('change-one');
	});

	it('the changes modal keeps the full notice collapsed', () => {
		expect(body({ notice: threeRevisions, acceptedRevision: 1 })).not.toContain('section-body');
	});

	it('an unknown accepted revision renders a blocking dialog without the notice or Continue', () => {
		const html = body({ notice: threeRevisions });
		expect(html).toContain('aria-modal="true"');
		expect(html).toContain('aria-busy="true"');
		expect(html).toContain('>Quit<');
		expect(html).not.toContain('>Continue<');
		expect(html).not.toContain('section-body');
	});

	it('a known revision renders a dialog that is not busy', () => {
		expect(body({ notice: threeRevisions, acceptedRevision: null })).toContain('aria-busy="false"');
	});

	it('accepted current revision renders no modal', () => {
		expect(body({ notice: threeRevisions, acceptedRevision: 3 })).not.toContain('role="dialog"');
	});

	it('an accepted gate renders no modal even on first run', async () => {
		const { acceptNoticeForTests } = await import('$lib/privacy/gate.svelte');
		acceptNoticeForTests();
		expect(body({ acceptedRevision: null })).not.toContain('role="dialog"');
	});

	it('the modal offers the in-app notice link', () => {
		expect(body({ acceptedRevision: null })).toContain('href="#privacy-notice"');
	});

	it('first run keeps the full notice behind the link', () => {
		for (const s of bundled.sections) expect(body({ acceptedRevision: null })).not.toContain(`data-section="${s.id}"`);
	});

	it('an accepted future revision renders no modal', () => {
		expect(body({ notice: threeRevisions, acceptedRevision: 4 })).not.toContain('role="dialog"');
	});

	it('the opened notice renderer includes every bundled section', () => {
		const html = render(PrivacyNoticeView).body;
		const escaped = (text: string) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
		for (const section of bundled.sections) {
			expect(html).toContain(`data-section="${section.id}"`);
			expect(html).toContain(escaped(section.heading));
			for (const paragraph of section.paragraphs) expect(html).toContain(escaped(paragraph));
			for (const item of section.items) expect(html).toContain(escaped(item));
		}
	});

	it('the notice renderer includes section list items', () => {
		const notice = { ...threeRevisions, sections: [{ id: 'list', heading: 'List', paragraphs: [], items: ['notice-item'] }] };
		expect(render(PrivacyNoticeView, { props: { notice } }).body).toContain('<li>notice-item</li>');
	});
});
