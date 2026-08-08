/**
 * Persisted UI picks — sort mode, colour filter, row limit, and every other
 * select the user can set and expects to find set on the next visit.
 *
 * Web counterpart of the desktop `ui_prefs` map (see docs/adr/013): same
 * interface, backed by localStorage since there is no Rust side here. The
 * frontend owns the keys. Search-style transient inputs stay out of this.
 */

const PREFIX = 'poe-pref-';

export interface PersistedString {
	value: string;
}

/**
 * A string preference that survives page reloads. Bind it like state:
 * `bind:value={pref.value}` — writes go through to localStorage, reads are
 * reactive. localStorage is synchronous, so there is no load race.
 */
export function persisted(key: string, initial: string): PersistedString {
	const stored = typeof localStorage === 'undefined' ? null : localStorage.getItem(PREFIX + key);
	let value = $state(stored ?? initial);
	return {
		get value() { return value; },
		set value(v: string) {
			value = v;
			try { localStorage.setItem(PREFIX + key, v); } catch { /* private mode */ }
		},
	};
}
