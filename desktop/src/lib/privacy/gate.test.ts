/**
 * The webview notice gate (POE-395, ADR-033): what Rust's answer and the
 * `privacy-accepted` event do to `noticeGate` and its waiters.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import { initNoticeGate, noticeGate, resetNoticeGateForTests, whenNoticeAccepted } from './gate.svelte';

const tauri = vi.hoisted(() => ({
	accepted: false,
	fail: false,
	onAccepted: null as null | (() => void),
	invokes: 0
}));

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async () => {
		tauri.invokes += 1;
		if (tauri.fail) throw new Error('no backend');
		return { currentRevision: 1, acceptedRevision: null, accepted: tauri.accepted, persisted: tauri.accepted };
	})
}));
vi.mock('@tauri-apps/api/event', () => ({
	listen: vi.fn(async (_event: string, handler: () => void) => {
		tauri.onAccepted = handler;
		return () => {};
	})
}));

beforeEach(() => {
	resetNoticeGateForTests();
	tauri.accepted = false;
	tauri.fail = false;
	tauri.onAccepted = null;
	tauri.invokes = 0;
});

describe('initNoticeGate', () => {
	it('an unaccepted answer leaves the gate known and closed', async () => {
		initNoticeGate();

		await vi.waitFor(() => expect(noticeGate.known).toBe(true));
		expect(noticeGate.accepted).toBe(false);
	});

	it('an accepted answer opens the gate', async () => {
		tauri.accepted = true;

		initNoticeGate();

		await vi.waitFor(() => expect(noticeGate.accepted).toBe(true));
	});

	it('a failed answer keeps the gate closed', async () => {
		tauri.fail = true;
		const logged = vi.spyOn(console, 'error').mockImplementation(() => {});

		initNoticeGate();

		await vi.waitFor(() =>
			expect(logged).toHaveBeenCalledWith('[privacy] get_privacy_state failed:', new Error('no backend'))
		);
		logged.mockRestore();
		expect(noticeGate.accepted).toBe(false);
	});

	it('an acceptance made while the listener registers is read from the state query', async () => {
		let registered!: () => void;
		vi.mocked(listen).mockImplementationOnce(
			(_event, handler) =>
				new Promise((resolve) => {
					registered = () => {
						tauri.onAccepted = handler as () => void;
						resolve(() => {});
					};
				})
		);

		initNoticeGate();
		await Promise.resolve();
		expect(tauri.invokes).toBe(0);
		tauri.accepted = true; // another window accepted; its event fired before registration
		registered();

		await vi.waitFor(() => expect(noticeGate.accepted).toBe(true));
	});

	it('a second call asks Rust no second time', async () => {
		initNoticeGate();
		initNoticeGate();

		await vi.waitFor(() => expect(noticeGate.known).toBe(true));
		expect(tauri.invokes).toBe(1);
	});
});

describe('whenNoticeAccepted', () => {
	it('resolves on privacy-accepted', async () => {
		const resolved = vi.fn();
		initNoticeGate();
		whenNoticeAccepted().then(resolved);
		await vi.waitFor(() => expect(noticeGate.known).toBe(true));
		await Promise.resolve();
		expect(resolved).not.toHaveBeenCalled();

		tauri.onAccepted!();

		await vi.waitFor(() => expect(resolved).toHaveBeenCalledTimes(1));
		expect(noticeGate.accepted).toBe(true);
	});
});
