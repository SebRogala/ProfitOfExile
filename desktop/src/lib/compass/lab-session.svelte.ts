import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { emit as tauriEmit, listen as tauriListen } from '@tauri-apps/api/event';
import { fetchLabLayout } from './layout-loader';
import {
	createNavState,
	handleNavEvent,
	loadLayout,
	setStrategy,
	type NavEvent,
	type NavState,
	type RouteStrategy
} from './navigation';

/** The `get_compass_settings` shape the lab widgets read. */
export type CompassSettings = {
	mode?: string;
	strategy?: RouteStrategy;
	difficulty?: string;
	shrine_warn_enabled?: boolean;
	shrine_warn_size?: string;
	shrine_warn_corner?: string;
	shrine_warn_on_take?: string;
};

/** A `lab-nav` event that reaches widget hooks and the replay history. */
export type LabNavEvent = NavEvent | { type: 'DarkshrineActivated' };

/** Everything `lab-nav` and `get_lab_catchup` can carry. */
export type LabNavMessage =
	| LabNavEvent
	| { type: 'LayoutChanged'; difficulty?: string }
	| { type: 'StrategyChanged'; strategy: RouteStrategy };

type LayoutUpdatedPayload = { action?: string; difficulty?: string } | null | undefined;

type Unlisten = () => void;

export const LAB_LAYOUT_NOTICE_EVENT = 'lab-layout-notice';
export const LAB_LAYOUT_NOTICE_REQUEST = 'lab-layout-notice-request';

export type MissingLayoutNotice = { id: number; difficulty: string };
export type LabLayoutNoticeSnapshot = { revision: number; notice: MissingLayoutNotice | null };

// Keep revisions ordered across session reacquisition and lab-window recreation,
// so an old queued snapshot cannot overwrite the replacement owner's state.
let layoutNoticeClock = Date.now();
function nextLayoutNoticeRevision(): number {
	return layoutNoticeClock = Math.max(Date.now(), layoutNoticeClock + 1);
}

export type LabSessionPorts = {
	invoke: <T = unknown>(command: string, args?: Record<string, unknown>) => Promise<T>;
	listen: <T>(event: string, handler: (event: { payload: T }) => void) => Promise<Unlisten>;
	emit: (event: string, payload: LabLayoutNoticeSnapshot) => Promise<void>;
	fetchLayout: typeof fetchLabLayout;
	setInterval: (callback: () => void, ms: number) => ReturnType<typeof setInterval>;
	clearInterval: (id: ReturnType<typeof setInterval>) => void;
	log: (msg: string) => void;
};

export type LabSessionHooks = {
	onNavEvent?(event: LabNavEvent, navAfter: NavState): void;
	onReset?(): void;
	onSettings?(settings: CompassSettings): void;
};

export type LabSession = ReturnType<typeof createLabSession>;

/** Context key under which the lab window page provides its one session. */
export const LAB_SESSION = Symbol('lab-session');

const POLL_MS = 2000;

const productionPorts: LabSessionPorts = {
	invoke: (command, args) => tauriInvoke(command, args),
	listen: (event, handler) => tauriListen(event, handler),
	emit: async (event, payload) => tauriEmit(event, payload),
	fetchLayout: fetchLabLayout,
	setInterval: (callback, ms) => setInterval(callback, ms),
	clearInterval: (id) => clearInterval(id),
	log: (msg) => {
		tauriInvoke('app_log_from_frontend', { msg }).catch((e) =>
			console.warn('[lab-session] logToApp IPC failed:', msg, e)
		);
	}
};

/**
 * The lab window's one navigation session: settings poll, layout, Client.txt
 * catch-up and the lab-nav / lab-layout-updated listeners, shared by the
 * compass, map and timer widgets. Starts on the first acquire, stops on the
 * last release. Each widget keeps its own view state and reacts through hooks.
 *
 * Counters fence stale async results (decision FE-M1 D3): `lifetime`
 * (bumped by stop) fences listener handlers; `generation` (bumped by stop and
 * by a layout reset or explicit selection) fences settings and layout results.
 * `catchupLifecycle` fences recovery only on stop/reset, so a selection cannot
 * discard the plaza and room history. `layoutRequest` orders layout reloads;
 * settings request counters order accepted settings snapshots independently.
 * Loader retries are not cancelled, only their results ignored.
 */
export function createLabSession(overrides: Partial<LabSessionPorts> = {}) {
	const ports: LabSessionPorts = { ...productionPorts, ...overrides };

	let navState = $state.raw<NavState>(createNavState());
	let layoutLoaded = $state(false);
	let lockedDifficulty = $state<string | null>(null);
	let pendingLayoutReset = $state(false);
	let settings = $state.raw<CompassSettings | null>(null);

	let history: { event: LabNavEvent; navAfter: NavState }[] = [];
	const subscribers = new Set<LabSessionHooks>();
	let refcount = 0;
	let lifetime = 0;
	let generation = 0;
	let catchupLifecycle = 0;
	let layoutRequest = 0;
	let settingsRequest = 0;
	let acceptedSettingsRequest = 0;
	let layoutCompletion: Promise<void> = Promise.resolve();
	let interval: ReturnType<typeof setInterval> | null = null;
	let registrations: Promise<Unlisten>[] = [];
	let pollErrorLogged = false;
	let layoutNotice: LabLayoutNoticeSnapshot = { revision: nextLayoutNoticeRevision(), notice: null };

	function log(msg: string): void {
		ports.log(`[lab-session] ${msg}`);
	}

	function publishLayoutNotice(): void {
		void ports.emit(LAB_LAYOUT_NOTICE_EVENT, layoutNotice).catch((e) =>
			log(`layout notice delivery failed: ${e}`)
		);
	}

	function setMissingLayout(difficulty: string | null): void {
		if ((layoutNotice.notice?.difficulty ?? null) === difficulty) return;
		const revision = nextLayoutNoticeRevision();
		layoutNotice = { revision, notice: difficulty ? { id: revision, difficulty } : null };
		publishLayoutNotice();
	}

	function fetchLayoutFromServer(preferred?: string): Promise<void> {
		layoutCompletion = loadSelectedLayout(preferred === 'Eternal' ? 'Uber' : preferred);
		return layoutCompletion;
	}

	async function loadSelectedLayout(preferred?: string): Promise<void> {
		if (preferred) {
			lockedDifficulty = preferred;
			generation++;
		}
		const g = generation;
		const request = ++layoutRequest;
		const selected = lockedDifficulty;
		const layout = await ports.fetchLayout({
			preferredDifficulty: preferred,
			lockedDifficulty,
			log
		});
		if (!layout || g !== generation || request !== layoutRequest) return;
		if ('missing' in layout) {
			if (!selected) return;
			setMissingLayout(selected);
			navState = {
				...createNavState(),
				strategy: navState.strategy,
				inLab: navState.inLab,
				finished: navState.finished
			};
			layoutLoaded = false;
			history = [];
			return;
		}
		setMissingLayout(null);
		navState = loadLayout(navState, layout);
		if (!lockedDifficulty) lockedDifficulty = layout.difficulty;
		layoutLoaded = true;
		log(`layout loaded: ${layout.difficulty} (${layout.rooms.length} rooms)`);
	}

	function applyEvent(event: LabNavMessage): void {
		if (event.type === 'LayoutChanged') {
			void fetchLayoutFromServer(event.difficulty);
			return;
		}
		if (event.type === 'StrategyChanged') {
			navState = setStrategy(navState, event.strategy);
			return;
		}
		if (event.type === 'LabExited') {
			layoutRequest++;
			setMissingLayout(null);
		}
		const next = handleNavEvent(navState, event as NavEvent);
		navState = next;
		history.push({ event, navAfter: next });
		for (const hooks of [...subscribers]) hooks.onNavEvent?.(event, next);
		if (event.type === 'LabExited' && pendingLayoutReset) applyLayoutReset();
	}

	function applyLayoutReset(): void {
		setMissingLayout(null);
		navState = createNavState();
		layoutLoaded = false;
		pendingLayoutReset = false;
		history = [];
		generation++;
		catchupLifecycle++;
		for (const hooks of [...subscribers]) hooks.onReset?.();
		log('layout reset applied (midnight UTC)');
		void init();
	}

	async function init(): Promise<void> {
		const g = generation;
		const lifecycle = catchupLifecycle;
		const request = ++settingsRequest;
		const initial = await ports.invoke<CompassSettings>('get_compass_settings').catch((e) => {
			log(`init settings failed: ${e}`);
			return null;
		});
		if (lifecycle !== catchupLifecycle) return;
		if (initial && g === generation && request > acceptedSettingsRequest) {
			acceptedSettingsRequest = request;
			settings = initial;
			if (initial.difficulty && !lockedDifficulty) {
				lockedDifficulty = initial.difficulty === 'Eternal' ? 'Uber' : initial.difficulty;
			}
			if (initial.strategy) navState = setStrategy(navState, initial.strategy);
			for (const hooks of [...subscribers]) hooks.onSettings?.(initial);
		}
		if (g === generation) void fetchLayoutFromServer();
		await waitForSelectedLayout(lifecycle);
		if (lifecycle !== catchupLifecycle) return;
		const catchup = await ports
			.invoke<{ events?: LabNavMessage[] }>('get_lab_catchup')
			.catch((e) => {
				log(`catchup failed: ${e}`);
				return null;
			});
		await waitForSelectedLayout(lifecycle);
		if (lifecycle !== catchupLifecycle) return;
		const events = catchup?.events ?? [];
		if (!events.length) return;
		log(`replaying ${events.length} catchup events`);
		for (const event of events) {
			// A replayed LabExited may apply a deferred reset; the rest of this
			// batch then belongs to the old layout.
			if (lifecycle !== catchupLifecycle) return;
			applyEvent(event);
		}
	}

	async function waitForSelectedLayout(lifecycle: number): Promise<void> {
		let completion: Promise<void>;
		do {
			completion = layoutCompletion;
			await completion;
		} while (lifecycle === catchupLifecycle && completion !== layoutCompletion);
	}

	function poll(): void {
		const g = generation;
		const request = ++settingsRequest;
		ports.invoke<CompassSettings>('get_compass_settings').then(
			(next) => {
				if (g !== generation || request < acceptedSettingsRequest) return;
				acceptedSettingsRequest = request;
				pollErrorLogged = false;
				const previousDifficulty = settings?.difficulty;
				const hadSettings = settings !== null;
				settings = next;
				if (next?.strategy && next.strategy !== navState.strategy) {
					navState = setStrategy(navState, next.strategy);
				}
				if (next?.difficulty && next.difficulty !== previousDifficulty) {
					if (hadSettings) void fetchLayoutFromServer(next.difficulty);
					else if (!lockedDifficulty) lockedDifficulty = next.difficulty === 'Eternal' ? 'Uber' : next.difficulty;
				}
				for (const hooks of [...subscribers]) hooks.onSettings?.(next);
			},
			(e) => {
				if (g !== generation || request < acceptedSettingsRequest) return;
				if (pollErrorLogged) return;
				pollErrorLogged = true;
				log(`settings poll failed: ${e}`);
			}
		);
	}

	function onLayoutUpdated(payload: LayoutUpdatedPayload): void {
		if (payload?.action === 'reset') {
			if (navState.inLab) {
				pendingLayoutReset = true;
				log('layout reset deferred (in lab)');
			} else {
				applyLayoutReset();
			}
			return;
		}
		void fetchLayoutFromServer(payload?.difficulty);
	}

	function start(): void {
		const l = lifetime;
		registrations = [
			ports.listen<LabNavMessage>('lab-nav', (event) => {
				if (l !== lifetime) return;
				applyEvent(event.payload);
			}),
			ports.listen<LayoutUpdatedPayload>('lab-layout-updated', (event) => {
				if (l !== lifetime) return;
				onLayoutUpdated(event.payload);
			}),
			ports.listen(LAB_LAYOUT_NOTICE_REQUEST, () => {
				if (l !== lifetime) return;
				publishLayoutNotice();
			})
		];
		for (const registration of registrations) {
			void registration.catch((e) => log(`event registration failed: ${e}`));
		}
		poll();
		interval = ports.setInterval(poll, POLL_MS);
		void init();
	}

	function stop(): void {
		if (interval !== null) ports.clearInterval(interval);
		interval = null;
		lifetime++;
		generation++;
		catchupLifecycle++;
		setMissingLayout(null);
		for (const registration of registrations) {
			registration.then((unlisten) => unlisten()).catch(() => {});
		}
		registrations = [];
		navState = createNavState();
		layoutLoaded = false;
		lockedDifficulty = null;
		pendingLayoutReset = false;
		settings = null;
		history = [];
		pollErrorLogged = false;
	}

	/**
	 * Subscribe a widget. Replays the history (each event with the nav state
	 * right after it) and the last settings snapshot to this subscriber alone,
	 * then delivers live. Runs hooks synchronously and reads reactive state, so
	 * callers acquire inside `untrack`.
	 */
	function acquire(hooks: LabSessionHooks): () => void {
		for (const { event, navAfter } of history) hooks.onNavEvent?.(event, navAfter);
		if (settings) hooks.onSettings?.(settings);
		subscribers.add(hooks);
		refcount++;
		if (refcount === 1) start();
		let released = false;
		return () => {
			if (released) return;
			released = true;
			subscribers.delete(hooks);
			refcount--;
			if (refcount === 0) stop();
		};
	}

	return {
		get navState() {
			return navState;
		},
		get layoutLoaded() {
			return layoutLoaded;
		},
		get lockedDifficulty() {
			return lockedDifficulty;
		},
		get pendingLayoutReset() {
			return pendingLayoutReset;
		},
		get settings() {
			return settings;
		},
		acquire
	};
}
