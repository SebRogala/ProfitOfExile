import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';
import { fetchLabLayout } from './layout-loader';
import {
	createNavState,
	handleNavEvent,
	loadLayout,
	setStrategy,
	type LabLayout,
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

export type LabSessionPorts = {
	invoke: <T = unknown>(command: string, args?: Record<string, unknown>) => Promise<T>;
	listen: <T>(event: string, handler: (event: { payload: T }) => void) => Promise<Unlisten>;
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
 * Two counters fence stale async results (decision FE-M1 D3): `lifetime`
 * (bumped by stop) fences listener handlers; `generation` (bumped by stop and
 * by a layout reset) fences the init chain, layout results and poll results.
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
	let interval: ReturnType<typeof setInterval> | null = null;
	let registrations: Promise<Unlisten>[] = [];
	let pollErrorLogged = false;

	function log(msg: string): void {
		ports.log(`[lab-session] ${msg}`);
	}

	async function fetchLayoutFromServer(preferred?: string): Promise<void> {
		const g = generation;
		if (preferred) lockedDifficulty = preferred;
		const layout: LabLayout | null = await ports.fetchLayout({
			preferredDifficulty: preferred,
			lockedDifficulty,
			log
		});
		if (!layout || g !== generation) return;
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
		const next = handleNavEvent(navState, event as NavEvent);
		navState = next;
		history.push({ event, navAfter: next });
		for (const hooks of [...subscribers]) hooks.onNavEvent?.(event, next);
		if (event.type === 'LabExited' && pendingLayoutReset) applyLayoutReset();
	}

	function applyLayoutReset(): void {
		navState = createNavState();
		layoutLoaded = false;
		pendingLayoutReset = false;
		history = [];
		generation++;
		for (const hooks of [...subscribers]) hooks.onReset?.();
		log('layout reset applied (midnight UTC)');
		void init();
	}

	async function init(): Promise<void> {
		const g = generation;
		const initial = await ports.invoke<CompassSettings>('get_compass_settings').catch((e) => {
			log(`init settings failed: ${e}`);
			return null;
		});
		if (g !== generation) return;
		if (initial) {
			settings = initial;
			if (initial.difficulty) lockedDifficulty = initial.difficulty;
			if (initial.strategy) navState = setStrategy(navState, initial.strategy);
			for (const hooks of [...subscribers]) hooks.onSettings?.(initial);
		}
		await fetchLayoutFromServer();
		if (g !== generation) return;
		const catchup = await ports
			.invoke<{ events?: LabNavMessage[] }>('get_lab_catchup')
			.catch((e) => {
				log(`catchup failed: ${e}`);
				return null;
			});
		if (g !== generation) return;
		const events = catchup?.events ?? [];
		if (!events.length) return;
		log(`replaying ${events.length} catchup events`);
		for (const event of events) {
			// A replayed LabExited may apply a deferred reset; the rest of this
			// batch then belongs to the old layout.
			if (g !== generation) return;
			applyEvent(event);
		}
	}

	function poll(): void {
		const g = generation;
		ports.invoke<CompassSettings>('get_compass_settings').then(
			(next) => {
				if (g !== generation) return;
				pollErrorLogged = false;
				settings = next;
				if (next?.strategy && next.strategy !== navState.strategy) {
					navState = setStrategy(navState, next.strategy);
				}
				if (next?.difficulty && !lockedDifficulty) lockedDifficulty = next.difficulty;
				for (const hooks of [...subscribers]) hooks.onSettings?.(next);
			},
			(e) => {
				if (g !== generation) return;
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
			})
		];
		poll();
		interval = ports.setInterval(poll, POLL_MS);
		void init();
	}

	function stop(): void {
		if (interval !== null) ports.clearInterval(interval);
		interval = null;
		lifetime++;
		generation++;
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
