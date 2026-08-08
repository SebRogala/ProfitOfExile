/**
 * Persisted UI picks — sort mode, colour filter, row limit, and every other
 * select the user can set and expects to find set on the next launch.
 *
 * Backed by the Rust `ui_prefs` settings map (see docs/adr/013). The frontend
 * owns the keys; Rust stores the map blindly and persists it with the rest of
 * the settings file. Search-style transient inputs stay out of this.
 *
 * The map is fetched once at first use. Components mount before that fetch
 * resolves, so each `persisted()` value starts at its default and is overwritten
 * when the load lands — the same start-default-then-overwrite pattern the
 * low-confidence toggle already uses. A value the user changes before the load
 * resolves wins over the loaded one, including for components mounted later:
 * pre-load writes are buffered and re-applied on top of the loaded map.
 *
 * Writes are debounced per key: every write persists the whole settings file
 * on the Rust side, and a text input bound to a pref fires per keystroke.
 * Accepted trade-off: a pick changed within the debounce window of closing
 * the app is lost — it degrades to the previous value, never corrupts.
 */
import { invoke } from '@tauri-apps/api/core';

const WRITE_DEBOUNCE_MS = 300;

let loaded: Record<string, string> | null = null;
let loadPromise: Promise<void> | null = null;
const pendingWrites = new Map<string, string>();
const writeTimers = new Map<string, ReturnType<typeof setTimeout>>();

function ensureLoaded(): Promise<void> {
	if (!loadPromise) {
		loadPromise = invoke<Record<string, string>>('get_ui_prefs')
			.then((p) => { loaded = p; })
			.catch((e) => { console.warn('[prefs] get_ui_prefs failed:', e); loaded = {}; })
			.then(() => {
				// Writes made before the load landed win over the loaded values —
				// both here and for any component that seeds from `loaded` later.
				for (const [k, v] of pendingWrites) loaded![k] = v;
				pendingWrites.clear();
			});
	}
	return loadPromise;
}

function scheduleWrite(key: string, v: string) {
	const prior = writeTimers.get(key);
	if (prior) clearTimeout(prior);
	writeTimers.set(key, setTimeout(() => {
		writeTimers.delete(key);
		invoke('set_ui_pref', { key, value: v })
			.catch((e) => console.warn('[prefs] set_ui_pref failed:', key, e));
	}, WRITE_DEBOUNCE_MS));
}

export interface PersistedString {
	value: string;
}

/**
 * A string preference that survives restarts. Bind it like state:
 * `bind:value={pref.value}` — writes go through to Rust, reads are reactive.
 */
export function persisted(key: string, initial: string): PersistedString {
	let value = $state(initial);
	let dirty = false;
	void ensureLoaded().then(() => {
		if (!dirty && loaded && key in loaded) value = loaded[key];
	});
	return {
		get value() { return value; },
		set value(v: string) {
			if (v === value && dirty) return;
			value = v;
			dirty = true;
			// Keep the module cache current: a component remounted later (tab
			// switch) seeds a fresh persisted() from `loaded`, not from Rust.
			if (loaded) loaded[key] = v;
			else pendingWrites.set(key, v);
			scheduleWrite(key, v);
		},
	};
}
