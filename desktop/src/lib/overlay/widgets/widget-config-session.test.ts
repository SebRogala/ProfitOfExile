/**
 * What a widget-config session forces on the way in, and owes back on the way
 * out.
 *
 * Since POE-241 that is the WINDOW's visibility and nothing else — the session
 * itself raises the window's desired state in the layout, so no module flag is
 * touched and no capture loop starts. Both remaining answers are silent when
 * they are wrong: forgetting the show leaves Settings waiting on a Save/Cancel
 * bar nobody can see, and forgetting the hide leaves an overlay standing over a
 * game the user had it hidden for.
 */
import { describe, expect, it } from 'vitest';
import {
	widgetConfigEnd,
	widgetConfigLive,
	widgetConfigSessionsInit,
	widgetConfigStart
} from './widget-config-session';

/** Start a session on a fresh map and hand back both halves. */
function start(shown: boolean) {
	return widgetConfigStart(widgetConfigSessionsInit(), 'temple', { shown });
}

/** The ordinary end: the user pressed Save with the game still behind Settings,
 *  which is the state every session opens in. */
const UNFOCUSED = false;

describe('opening a widget-config session', () => {
	it('forces nothing when the window is already up', () => {
		expect(start(true).actions).toEqual({ showWindow: false });
	});

	it('shows the window when it is hidden or not built yet', () => {
		// Two readings collapse to one action: the Rust focus poller hides a
		// module overlay whenever the game is not in front — which, with the user
		// in Settings, it never is — and a module whose flag is off has no window
		// at all until this session asks for one.
		expect(start(false).actions).toEqual({ showWindow: true });
	});
});

describe('closing a widget-config session', () => {
	it('restores nothing when the window was already up at the start', () => {
		const opened = start(true);
		expect(widgetConfigEnd(opened.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: false
		});
	});

	it('hides the window again when the show was forced', () => {
		const opened = start(false);
		expect(widgetConfigEnd(opened.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: true
		});
	});

	it('forgets the session, so a second end restores nothing a second time', () => {
		const opened = start(false);
		const first = widgetConfigEnd(opened.sessions, 'temple', UNFOCUSED);
		expect(widgetConfigEnd(first.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: false
		});
	});

	it('touches nothing for a module Settings never opened a session for', () => {
		// The host emits `widget-config-end` after any exit from config mode,
		// including one entered through its own catch-up query. Hiding a window
		// nobody forced up would be this feature reaching outside what it did.
		expect(widgetConfigEnd(widgetConfigSessionsInit(), 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: false
		});
	});

	it('leaves another module’s live session alone', () => {
		const temple = widgetConfigStart(widgetConfigSessionsInit(), 'temple', { shown: false });
		const merc = widgetConfigStart(temple.sessions, 'mercenary', { shown: true });
		const ended = widgetConfigEnd(merc.sessions, 'mercenary', UNFOCUSED);
		expect(ended.actions).toEqual({ hideWindow: false });
		// The temple session is untouched and still owes its show.
		expect(widgetConfigEnd(ended.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: true
		});
	});
});

describe('a second Configure press on a live session', () => {
	it('re-shows a window the focus poller hid since the first press', () => {
		const first = widgetConfigStart(widgetConfigSessionsInit(), 'temple', { shown: false });
		const second = widgetConfigStart(first.sessions, 'temple', { shown: false });
		// The guide makes a second press the user's way out of a window that
		// missed the event, so the show is owed again — derived from the reading
		// NOW, not from the record.
		expect(second.actions).toEqual({ showWindow: true });
	});

	it('still owes the show it forced at the first press', () => {
		const first = widgetConfigStart(widgetConfigSessionsInit(), 'temple', { shown: false });
		// By the second press the window is up — because the first press showed
		// it. A record re-derived here would conclude the session had never forced
		// anything and leave the overlay standing afterwards.
		const second = widgetConfigStart(first.sessions, 'temple', { shown: true });
		expect(widgetConfigEnd(second.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: true
		});
	});
});

describe('the game taking focus back during a session', () => {
	it('leaves the window up rather than hiding what the poller just showed', () => {
		// The session forced the window up because the game was behind Settings.
		// If the game is in front by Save, the focus poller has already shown this
		// window and wants it shown — and it acts on TRANSITIONS, so a hide here
		// would cost the player their overlay until two more of them.
		const opened = start(false);
		expect(widgetConfigEnd(opened.sessions, 'temple', true).actions).toEqual({
			hideWindow: false
		});
	});
});

describe('whether a module is being arranged', () => {
	it('says no before any session has opened', () => {
		expect(widgetConfigLive(widgetConfigSessionsInit(), 'temple')).toBe(false);
	});

	it('says yes for a session that forced nothing at all', () => {
		// Two callers read this: the layout ORs it into the window's desired
		// state, and the window's creation path reads it to decide whether to run
		// its not-focused-so-hide step. A session that happened to need no forcing
		// is still a session, and a window built under it must not be hidden.
		expect(widgetConfigLive(start(true).sessions, 'temple')).toBe(true);
	});

	it('says no again once the session has ended', () => {
		// This is what tears a window down again for a module whose flag is off:
		// the layout's desired state loses its second term the moment the record
		// goes.
		const opened = start(false);
		const ended = widgetConfigEnd(opened.sessions, 'temple', UNFOCUSED);
		expect(widgetConfigLive(ended.sessions, 'temple')).toBe(false);
	});

	it('says no for a module whose sibling has a live session', () => {
		expect(widgetConfigLive(start(false).sessions, 'mercenary')).toBe(false);
	});
});
