/**
 * What a widget-config session had to FORCE, so its end can undo exactly that
 * (POE-226).
 *
 * Settings can only arrange widgets in a window that exists and is on screen,
 * and neither is guaranteed: a module-coupled overlay is built off the module
 * flag (`module-lifecycle.ts`) and hidden by the Rust focus poller whenever the
 * game is not in front — which, while the user is looking at Settings, it never
 * is. So step 1 of the ordering contract (`docs/OVERLAY-GUIDE.md`, "Config-mode
 * ordering contract") force-shows the one label, the way `set_debug_mode`
 * force-shows every overlay, and this file is the record of what that cost.
 *
 * It is a reducer rather than two booleans in the layout because the four
 * shown × enabled combinations each restore differently and the wrong one is
 * silent in both directions: forgetting to disable a module the user never
 * turned on leaves a screen-capture loop running against their wishes, and
 * disabling one they DID have on takes their overlay away with no toggle
 * touched. A `.svelte` file has no unit-test harness here, so the decision
 * lives out where `widget-config-session.test.ts` can drive all four.
 *
 * Sessions are keyed by module. Only the temple has widgets today, but the
 * state is a map rather than one slot so a second module's session cannot
 * overwrite the first's record and strand a forced window.
 */

/** The state of a module's overlay when its config session began. */
export interface WidgetConfigPreState {
	/** Whether its window existed AND was visible. */
	shown: boolean;
	/** Whether its module flag was on. */
	enabled: boolean;
}

/** What one live session forced, and therefore owes back. */
export interface WidgetConfigForced {
	/** The window was hidden (or absent) and had to be shown. */
	shown: boolean;
	/** The module was off and had to be switched on. */
	enabled: boolean;
}

/** Every live session, by module. */
export type WidgetConfigSessions = Readonly<Record<string, WidgetConfigForced>>;

/** What the caller must do before it can set config mode on the window. */
export interface WidgetConfigStartActions {
	/** Switch the module flag on — the window is built off it, so this is what
	 *  makes one exist at all. */
	enableModule: boolean;
	/** Show the window. */
	showWindow: boolean;
}

/** What the caller must do once the host has left config mode. */
export interface WidgetConfigEndActions {
	hideWindow: boolean;
	disableModule: boolean;
}

/** No session anywhere. */
export function widgetConfigSessionsInit(): WidgetConfigSessions {
	return {};
}

/**
 * Whether `module` is being arranged right now.
 *
 * Read by the window's own CREATION path, which ends by hiding the window when
 * the game is not focused — correct for an overlay built while the player is
 * alt-tabbed, and exactly wrong for one built BECAUSE the user pressed
 * Configure, since the user is in Settings and the game is never focused then.
 * A session is live from the moment `widgetConfigStart` records it, which is
 * before the module flag is switched on, so the creation this flow triggers
 * cannot start before the answer is available.
 */
export function widgetConfigLive(sessions: WidgetConfigSessions, module: string): boolean {
	return sessions[module] !== undefined;
}

/**
 * Open (or re-open) a session for `module`, given what its overlay looks like
 * right now.
 *
 * The actions are derived from the CURRENT reading every time, including on a
 * second Configure press for a module that already has a session — the guide's
 * way out of a window that missed the event. Between the two presses the focus
 * poller may have hidden the window again, so re-deriving is what makes the
 * second press actually re-open it rather than emit into a window nobody can
 * see.
 *
 * The RECORD, by contrast, is sticky: once a session has forced something it
 * still owes it back at the end, whatever the state was on a later press. A
 * record that was re-derived would let a second press — taken after the poller
 * hid the window — decide the session had never forced anything.
 */
export function widgetConfigStart(
	sessions: WidgetConfigSessions,
	module: string,
	pre: WidgetConfigPreState
): { sessions: WidgetConfigSessions; actions: WidgetConfigStartActions } {
	const actions: WidgetConfigStartActions = {
		enableModule: !pre.enabled,
		showWindow: !pre.shown
	};
	const already = sessions[module];
	return {
		sessions: {
			...sessions,
			[module]: {
				shown: (already?.shown ?? false) || actions.showWindow,
				enabled: (already?.enabled ?? false) || actions.enableModule
			}
		},
		actions
	};
}

/**
 * Close the session for `module` and say what to undo.
 *
 * A module with NO session undoes nothing. `widget-config-end` is emitted by
 * the host, which is also reachable without Settings (the catch-up query on
 * mount, or a config mode entered some other way) — hiding a window nobody
 * forced up, or switching off a module the user turned on themselves, would be
 * this feature reaching outside what it did.
 *
 * `gameFocused` is the state of the world NOW, and it vetoes the hide. The
 * session forced the window up because the game was not in front; if it is in
 * front by the time the user Saves, the window is where the focus poller wants
 * it and hiding it would take the overlay off the player's screen until TWO
 * more focus transitions put it back — the poller acts on changes, and nothing
 * changed. Restoring "hidden" is only a restore while the reason for hiding
 * still holds.
 *
 * The hide comes before the disable when both are owed: disabling tears the
 * window down, so the order is only about not calling `hide` on a label that
 * has just gone away.
 */
export function widgetConfigEnd(
	sessions: WidgetConfigSessions,
	module: string,
	gameFocused: boolean
): { sessions: WidgetConfigSessions; actions: WidgetConfigEndActions } {
	const forced = sessions[module];
	if (!forced) {
		return { sessions, actions: { hideWindow: false, disableModule: false } };
	}
	const rest = { ...sessions };
	delete rest[module];
	return {
		sessions: rest,
		actions: { hideWindow: forced.shown && !gameFocused, disableModule: forced.enabled }
	};
}
