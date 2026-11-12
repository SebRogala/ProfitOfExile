import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount, flushSync } from 'svelte';
import Page from './+page.svelte';

/** The fake stream the page opens; the test fires `onopen` itself. */
let streams: FakeEventSource[];
class FakeEventSource {
	onopen: (() => void) | null = null;
	onmessage: ((e: MessageEvent) => void) | null = null;
	onerror: (() => void) | null = null;
	constructor(public url: string) {
		streams.push(this);
	}
	close() {}
}

/** Every GET /api/analysis/status takes the next queued answer; the default serves `served`. */
let served: string;
let statusAnswers: Array<Promise<Response>>;
let statusReads: number;
function serve() {
	vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request) => {
		const url = new URL(String(input), window.location.origin);
		switch (url.pathname) {
			case '/api/analysis/status':
				statusReads++;
				return statusAnswers.shift() ?? Response.json({ league: served });
			case '/api/mercure/token':
				return Response.json({ token: 'tok', url: 'https://hub.test/.well-known/mercure' });
			case '/api/analysis/font': return Response.json({ safe: [], premium: [], jackpot: [] });
			case '/api/analysis/gems/names': return Response.json({ names: [] });
			default:
				return Response.json({ data: [] });
		}
	}));
}

let mounted: ReturnType<typeof mount> | null = null;
async function attach() {
	const target = document.createElement('div');
	document.body.append(target);
	flushSync(() => { mounted = mount(Page, { target }); });
	await vi.advanceTimersByTimeAsync(0);
	expect(streams).toHaveLength(1);
}
/** Past the page's whole reload window (2 s debounce, jitter pinned to 0 by Math.random). */
const pastReloadWindow = () => vi.advanceTimersByTimeAsync(7000);

beforeEach(() => {
	streams = [];
	statusAnswers = [];
	statusReads = 0;
	served = 'Mercenaries';
	vi.stubGlobal('EventSource', FakeEventSource);
	vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
	vi.spyOn(Math, 'random').mockReturnValue(0);
	vi.spyOn(console, 'log').mockImplementation(() => {});
	serve();
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	document.body.innerHTML = '';
	vi.useRealTimers();
	vi.unstubAllGlobals();
	vi.restoreAllMocks();
});

describe('lab page league check on stream open', () => {
	it('reads no extra status when the stream opens before the first status arrives', async () => {
		let answer!: (r: Response) => void;
		statusAnswers.push(new Promise<Response>((r) => { answer = r; }));
		await attach();
		streams[0].onopen!();
		await vi.advanceTimersByTimeAsync(0);
		expect(statusReads).toBe(1);
		answer(Response.json({ league: served }));
		await pastReloadWindow();
		expect(statusReads).toBe(1);
	});

	it('schedules no reload when the served league is still the held one', async () => {
		await attach();
		expect(statusReads).toBe(1);
		streams[0].onopen!();
		await pastReloadWindow();
		expect(statusReads).toBe(2);
	});

	it('schedules exactly one reload when the server now serves another league', async () => {
		await attach();
		served = 'Keepers';
		streams[0].onopen!();
		await pastReloadWindow();
		// 1 initial load, 1 open check, 1 reload.
		expect(statusReads).toBe(3);
		await pastReloadWindow();
		expect(statusReads).toBe(3);
	});
});
