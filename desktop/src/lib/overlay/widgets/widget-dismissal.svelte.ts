import { tick, untrack } from 'svelte';
import type { BossLootShortcut, DismissSettings, PendingDismiss } from '$lib/stores/ssot.svelte';
import type { WidgetSpec } from './widget-registry';

export function createDismissal({ spec, shown, contentKey, settings }: {
	spec: WidgetSpec;
	shown(): boolean;
	contentKey(): unknown;
	settings(): DismissSettings | null;
}) {
	let hidden = $state(false);
	let startedAt = $state<number | null>(null);
	let wasShown = false;
	let previousKey: unknown;
	const seconds = $derived.by(() => {
		const configured = settings();
		if (!configured || !spec.dismiss?.timeout) return 0;
		const value = Object.hasOwn(configured.overlays, spec.id) ? configured.overlays[spec.id].timeout : 'inherit';
		return value === 'inherit' ? configured.timeoutSeconds : value.override;
	});

	$effect(() => {
		const visible = shown();
		const key = contentKey();
		if (visible && (!wasShown || !Object.is(key, previousKey))) {
			hidden = false;
			startedAt = Date.now();
		} else if (!visible) {
			hidden = false;
			startedAt = null;
		}
		wasShown = visible;
		previousKey = key;
	});

	$effect(() => {
		if (hidden || startedAt === null || seconds === 0) return;
		const timer = setTimeout(() => { hidden = true; }, Math.max(0, startedAt + seconds * 1000 - Date.now()));
		return () => clearTimeout(timer);
	});

	return {
		get hidden() { return hidden; },
		get ready() { return settings() !== null; },
		applyKey(pressed: BossLootShortcut) {
			const configured = settings();
			if (!configured || !spec.dismiss?.key || !shown()) return;
			const value = Object.hasOwn(configured.overlays, spec.id) ? configured.overlays[spec.id].shortcut : 'inherit';
			const effective = value === 'inherit' ? configured.shortcut : value.override;
			if (pressed.key === effective.key && pressed.shift === effective.shift &&
				pressed.ctrl === effective.ctrl && pressed.alt === effective.alt) hidden = true;
		}
	};
}

export function createDismissalHost({ window, dismissals, pending, listen, invoke, frame, ack = true }: {
	window: string;
	dismissals: ReturnType<typeof createDismissal>[];
	pending(): readonly PendingDismiss[];
	listen(event: string, handler: (event: { payload: PendingDismiss }) => void): Promise<() => void>;
	invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
	frame(): Promise<void>;
	ack?: boolean;
}) {
	const applied = new Set<number>();
	const queued = new Map<number, PendingDismiss>();
	let active = false;
	let running = false;
	let disposed = false;
	function report(message: string, error: unknown) {
		const msg = `[${window}-overlay] ${message}: ${error}`;
		console.warn(msg);
		void invoke('app_log_from_frontend', { msg }).catch((cause) =>
			console.error('[overlay] app log unreachable:', cause)
		);
	}

	async function drain() {
		if (!active || running || disposed || !dismissals.every((dismissal) => dismissal.ready)) return;
		running = true;
		try {
			while (queued.size && !disposed) {
				// Settle show/content effects before applying the hide, including settings catch-up.
				await tick();
				if (disposed || !dismissals.every((dismissal) => dismissal.ready)) break;
				const next = [...queued.values()].sort((a, b) => a.gen - b.gen)[0];
				for (const dismissal of dismissals) dismissal.applyKey(next.shortcut);
				await tick();
				if (disposed) break;
				await frame();
				if (disposed) break;
				await frame();
				if (disposed) break;
				if (ack) await invoke('overlay_dismiss_ack', { window, gen: next.gen });
				applied.add(next.gen);
				queued.delete(next.gen);
			}
		} catch (error) {
			report('dismissal failed', error);
		} finally {
			running = false;
		}
	}
	function enqueue(entries: readonly PendingDismiss[]) {
		for (const entry of entries) {
			if (!applied.has(entry.gen) && !queued.has(entry.gen)) queued.set(entry.gen, entry);
		}
		void drain();
	}

	$effect(() => {
		let unlisten: (() => void) | undefined;
		void (async () => {
			try {
				unlisten = await listen('overlay-dismiss-key', (event) => enqueue([event.payload]));
				if (disposed) { unlisten(); return; }
				if (ack) await invoke('overlay_dismiss_host', { window });
				if (disposed) return;
				active = true;
				await drain();
			} catch (error) {
				report('dismissal setup failed', error);
			}
		})();
		return () => { disposed = true; active = false; unlisten?.(); };
	});
	$effect(() => {
		const entries = pending();
		// Keep event-only generations queued until their effective settings are available.
		for (const dismissal of dismissals) void dismissal.ready;
		untrack(() => enqueue(entries));
	});
}
