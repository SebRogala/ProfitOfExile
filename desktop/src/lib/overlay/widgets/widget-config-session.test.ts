/**
 * What a widget-config session forces on the way in, and owes back on the way
 * out.
 *
 * Every case here is silent when it is wrong. Forcing a module on and forgetting
 * to switch it off leaves a screen-capture loop running the user never asked
 * for; switching off one they had on themselves takes their overlay away with
 * no toggle touched; and forgetting the show leaves Settings waiting on a
 * Save/Cancel bar nobody can see.
 */
import { describe, expect, it } from 'vitest';
import {
	widgetConfigEnd,
	widgetConfigLive,
	widgetConfigSessionsInit,
	widgetConfigStart
} from './widget-config-session';

/** Start a session on a fresh map and hand back both halves. */
function start(shown: boolean, enabled: boolean) {
	return widgetConfigStart(widgetConfigSessionsInit(), 'temple', { shown, enabled });
}

/** The ordinary end: the user pressed Save with the game still behind Settings,
 *  which is the state every session opens in. */
const UNFOCUSED = false;

describe('opening a widget-config session', () => {
	it('forces nothing when the window is already up and the module is on', () => {
		expect(start(true, true).actions).toEqual({ enableModule: false, showWindow: false });
	});

	it('shows the window when the module is on but the game is not focused', () => {
		// The Rust focus poller hides a module overlay whenever the game is not in
		// front — which, with the user in Settings, it never is.
		expect(start(false, true).actions).toEqual({ enableModule: false, showWindow: true });
	});

	it('enables the module and shows the window when the module is off', () => {
		// A module-coupled overlay exists only while its flag is on, so with the
		// module off there is no window at all to arrange.
		expect(start(false, false).actions).toEqual({ enableModule: true, showWindow: true });
	});

	it('enables the module without a second show when its window is somehow up', () => {
		// Reachable while the driver has not yet caught up with a flag that just
		// went off. The window is there, so showing it again would be a no-op —
		// but the module still has to be switched back on for config mode to
		// survive the driver's next pass.
		expect(start(true, false).actions).toEqual({ enableModule: true, showWindow: false });
	});
});

describe('closing a widget-config session', () => {
	it('restores nothing when nothing was forced', () => {
		const opened = start(true, true);
		expect(widgetConfigEnd(opened.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: false,
			disableModule: false
		});
	});

	it('hides the window again when only the show was forced', () => {
		const opened = start(false, true);
		expect(widgetConfigEnd(opened.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: true,
			disableModule: false
		});
	});

	it('hides the window and switches the module back off when both were forced', () => {
		const opened = start(false, false);
		expect(widgetConfigEnd(opened.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: true,
			disableModule: true
		});
	});

	it('switches the module back off without hiding when only it was forced', () => {
		const opened = start(true, false);
		expect(widgetConfigEnd(opened.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: false,
			disableModule: true
		});
	});

	it('forgets the session, so a second end restores nothing a second time', () => {
		const opened = start(false, false);
		const first = widgetConfigEnd(opened.sessions, 'temple', UNFOCUSED);
		expect(widgetConfigEnd(first.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: false,
			disableModule: false
		});
	});

	it('touches nothing for a module Settings never opened a session for', () => {
		// The host emits `widget-config-end` after any exit from config mode,
		// including one entered through its own catch-up query. Hiding a window
		// nobody forced up would be this feature reaching outside what it did.
		expect(widgetConfigEnd(widgetConfigSessionsInit(), 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: false,
			disableModule: false
		});
	});

	it('leaves another module’s live session alone', () => {
		const temple = widgetConfigStart(widgetConfigSessionsInit(), 'temple', {
			shown: false,
			enabled: false
		});
		const merc = widgetConfigStart(temple.sessions, 'mercenary', { shown: false, enabled: true });
		const ended = widgetConfigEnd(merc.sessions, 'mercenary', UNFOCUSED);
		expect(ended.actions).toEqual({ hideWindow: true, disableModule: false });
		// The temple session is untouched and still owes both.
		expect(widgetConfigEnd(ended.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: true,
			disableModule: true
		});
	});
});

describe('a second Configure press on a live session', () => {
	it('re-shows a window the focus poller hid since the first press', () => {
		const first = widgetConfigStart(widgetConfigSessionsInit(), 'temple', {
			shown: false,
			enabled: false
		});
		const second = widgetConfigStart(first.sessions, 'temple', { shown: false, enabled: true });
		// The module is on by now, so only the show is owed again — and it IS
		// owed, because the guide makes a second press the user's way out of a
		// window that missed the event.
		expect(second.actions).toEqual({ enableModule: false, showWindow: true });
	});

	it('still owes the module it forced on at the first press', () => {
		const first = widgetConfigStart(widgetConfigSessionsInit(), 'temple', {
			shown: false,
			enabled: false
		});
		// By the second press the module reads as on — because the first press
		// turned it on. A record re-derived here would conclude the session had
		// never forced it and leave the module running afterwards.
		const second = widgetConfigStart(first.sessions, 'temple', { shown: true, enabled: true });
		expect(widgetConfigEnd(second.sessions, 'temple', UNFOCUSED).actions).toEqual({
			hideWindow: true,
			disableModule: true
		});
	});
});

describe('the game taking focus back during a session', () => {
	it('leaves the window up rather than hiding what the poller just showed', () => {
		// The session forced the window up because the game was behind Settings.
		// If the game is in front by Save, the focus poller has already shown this
		// window and wants it shown — and it acts on TRANSITIONS, so a hide here
		// would cost the player their overlay until two more of them.
		const opened = start(false, true);
		expect(widgetConfigEnd(opened.sessions, 'temple', true).actions).toEqual({
			hideWindow: false,
			disableModule: false
		});
	});

	it('still switches a force-enabled module back off', () => {
		// Focus says where the WINDOW belongs. It says nothing about a module the
		// user never turned on, which must come back off either way.
		const opened = start(false, false);
		expect(widgetConfigEnd(opened.sessions, 'temple', true).actions).toEqual({
			hideWindow: false,
			disableModule: true
		});
	});
});

describe('whether a module is being arranged', () => {
	it('says no before any session has opened', () => {
		expect(widgetConfigLive(widgetConfigSessionsInit(), 'temple')).toBe(false);
	});

	it('says yes for a session that forced nothing at all', () => {
		// The window's creation path reads this to decide whether to run its
		// not-focused-so-hide step. A session that happened to need no forcing is
		// still a session, and a window built under it must not be hidden.
		expect(widgetConfigLive(start(true, true).sessions, 'temple')).toBe(true);
	});

	it('says no again once the session has ended', () => {
		const opened = start(false, false);
		const ended = widgetConfigEnd(opened.sessions, 'temple', UNFOCUSED);
		expect(widgetConfigLive(ended.sessions, 'temple')).toBe(false);
	});

	it('says no for a module whose sibling has a live session', () => {
		expect(widgetConfigLive(start(false, false).sessions, 'mercenary')).toBe(false);
	});
});
