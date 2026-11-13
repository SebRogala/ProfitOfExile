/**
 * The server-base carrier (POE-383, D2/D3): who hears a url, and how a wait for
 * the first one settles.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	applyServerSlice,
	getServerUrl,
	onServerUrlChange,
	resetServerBaseForTests,
	SERVER_URL_WAIT_MS,
	whenServerUrl
} from './server-base.svelte';

const DEFAULT = 'https://default.example';

function deliver(url: string): void {
	applyServerSlice({ url, defaultUrl: DEFAULT });
}

beforeEach(() => {
	resetServerBaseForTests();
});

afterEach(() => {
	vi.useRealTimers();
});

describe('onServerUrlChange', () => {
	it('a listener hears the first url with no previous', () => {
		const heard = vi.fn();
		onServerUrlChange(heard);

		deliver('https://a.example');

		expect(heard.mock.calls).toEqual([['https://a.example', null]]);
	});

	it('a changed url reaches the listener with the previous one', () => {
		deliver('https://a.example');
		const heard = vi.fn();
		onServerUrlChange(heard);
		heard.mockClear();

		deliver('https://b.example');

		expect(heard.mock.calls).toEqual([['https://b.example', 'https://a.example']]);
	});

	it('a repeated url reaches no listener', () => {
		deliver('https://a.example');
		const heard = vi.fn();
		onServerUrlChange(heard);
		heard.mockClear();

		deliver('https://a.example');

		expect(heard).not.toHaveBeenCalled();
	});

	it('a listener added after the url is known hears it once', () => {
		deliver('https://a.example');
		const heard = vi.fn();

		onServerUrlChange(heard);

		expect(heard.mock.calls).toEqual([['https://a.example', null]]);
	});

	it('an unsubscribed listener hears no later change', () => {
		const heard = vi.fn();
		const stop = onServerUrlChange(heard);
		stop();

		deliver('https://a.example');

		expect(heard).not.toHaveBeenCalled();
	});
});

describe('applyServerSlice', () => {
	it('an empty url leaves the server unknown', () => {
		const heard = vi.fn();
		onServerUrlChange(heard);

		deliver('');

		expect(getServerUrl()).toBeNull();
		expect(heard).not.toHaveBeenCalled();
	});

	it('an absent slice keeps the known url', () => {
		deliver('https://a.example');

		applyServerSlice(undefined);

		expect(getServerUrl()).toBe('https://a.example');
	});
});

describe('whenServerUrl', () => {
	it('resolves when the first url lands', async () => {
		const wait = whenServerUrl();

		deliver('https://a.example');

		await expect(wait).resolves.toBe('https://a.example');
	});

	it('rejects at once for an already-aborted signal while the url is known', async () => {
		deliver('https://a.example');
		const controller = new AbortController();
		controller.abort('gone');

		await expect(whenServerUrl(controller.signal)).rejects.toBe('gone');
	});

	it('rejects at once for an already-aborted signal while the url is unknown', async () => {
		vi.useFakeTimers();
		const controller = new AbortController();
		controller.abort('gone');
		const settled = vi.fn();

		whenServerUrl(controller.signal).catch(settled);
		await vi.advanceTimersByTimeAsync(0);

		expect(settled).toHaveBeenCalledWith('gone');
	});

	it('rejects when its signal aborts during the wait', async () => {
		const controller = new AbortController();
		const wait = whenServerUrl(controller.signal);

		controller.abort('closed');

		await expect(wait).rejects.toBe('closed');
	});

	it('rejects after SERVER_URL_WAIT_MS with no url', async () => {
		vi.useFakeTimers();
		const settled = vi.fn();
		whenServerUrl().catch(settled);

		await vi.advanceTimersByTimeAsync(SERVER_URL_WAIT_MS - 1);
		expect(settled).not.toHaveBeenCalled();
		await vi.advanceTimersByTimeAsync(1);

		expect(settled).toHaveBeenCalledWith(new Error('server url not known'));
	});

	it('a settled wait leaves no waiter or abort listener behind', async () => {
		vi.useFakeTimers();
		const controller = new AbortController();
		const removed = vi.spyOn(controller.signal, 'removeEventListener');
		const resolved = vi.fn();
		const rejected = vi.fn();
		whenServerUrl(controller.signal).then(resolved, rejected);
		controller.abort('closed');
		await vi.advanceTimersByTimeAsync(0);

		deliver('https://a.example');
		await vi.advanceTimersByTimeAsync(SERVER_URL_WAIT_MS);

		expect(removed).toHaveBeenCalledWith('abort', expect.any(Function));
		expect(resolved).not.toHaveBeenCalled();
		expect(rejected).toHaveBeenCalledTimes(1);
		expect(vi.getTimerCount()).toBe(0);
	});
});
