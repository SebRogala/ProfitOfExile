/**
 * What Settings → Overlay Positions actually lists (POE-226).
 *
 * The section used to be one flat list of five rows, one per overlay WINDOW.
 * It is now three groups — Lab, Merc, Temple — because the windows stopped
 * being the only unit: the temple's overlay is a single fullscreen canvas with
 * WIDGETS inside it (POE-225), so its rows are widgets and it is arranged with
 * one in-window config session rather than five per-window ones.
 *
 * The derivation lives here rather than in `SettingsPage.svelte` for the reason
 * every other decision in this feature does: a `.svelte` file has no unit-test
 * harness in this app, and the interesting parts are the two things that fail
 * SILENTLY on screen — a group drawn for a device that does not have the
 * feature (a control that places an overlay the user can never open), and a
 * geometry line that says "Not set" for a widget that is in fact placed.
 *
 * The five window rows are carried through unchanged, names included: they key
 * `OVERLAY_CONFIGS` and the per-row state records in the page, and their
 * Configure flow (`overlay-config-start` / `reclaimMouse`) is untouched by this
 * file. All this adds is which heading each one sits under.
 */
import { TEMPLE_WINDOW_LABEL } from '../manager';
import type { WidgetGeometry } from './widget-geometry';
import { placeableWidgetsFor, type WidgetSpec } from './widget-registry';

/**
 * Which per-device features are granted, as the page reads them.
 *
 * Booleans rather than the feature ids themselves, so this module needs no
 * import from the entitlements rune store and the tests need no store at all.
 * `SettingsPage.svelte` supplies them from `hasFeature(MERC_FEATURE)` and
 * `hasFeature(TEMPLE_FEATURE)` — the same two answers `routes/(app)/+layout.svelte`
 * gates the Mercenaries and Temple pages and their overlays on.
 */
export interface OverlayGroupGrants {
	merc: boolean;
	temple: boolean;
}

/** One of the five overlays Settings places by dragging a config COPY of the
 *  real window. Its `name` keys `OVERLAY_CONFIGS` and the page's per-row state. */
export interface OverlayWindowRow {
	name: string;
	label: string;
}

/** One heading in Overlay Positions, with everything drawn under it. */
export interface OverlayGroup {
	/** Stable key for the `{#each}` — never shown. */
	id: string;
	/** The heading text. */
	heading: string;
	/** The per-window rows, each keeping its own Configure flow. */
	windows: OverlayWindowRow[];
	/** The widgets this group's module places inside its fullscreen window. */
	widgets: WidgetSpec[];
	/**
	 * The module the group's ONE "Configure widgets" button arranges, or `null`
	 * when the group has no widgets.
	 *
	 * Null rather than the module id for a group whose module declares nothing
	 * placeable: a button that flips a window into config mode with no frames in
	 * it hands the user an interactive, monitor-sized rectangle over the game
	 * whose only way out is a Save/Cancel bar for zero widgets.
	 */
	configureModule: string | null;
}

/** A group before the grants are applied. */
interface GroupSpec {
	id: string;
	heading: string;
	/** The grant the WHOLE group is hidden without, or `null` for always shown. */
	grant: keyof OverlayGroupGrants | null;
	/** The module whose widgets this group lists, or `null` for a window-only group. */
	module: string | null;
	windows: OverlayWindowRow[];
}

/**
 * The three groups, in display order.
 *
 * Lab is the four lab overlays, unchanged and ungated. Merc is the verdict
 * strip: its row belongs to the merc MODULE, and a device without the `merc`
 * feature never sees that module (POE-203), so the whole group is left out
 * rather than disabled — the same reason the flat list used to drop that one
 * row. Temple has no window row at all: its overlay is the monitor and has no
 * persisted rect of its own (POE-225 D8), so everything under that heading is a
 * widget.
 */
const GROUPS: readonly GroupSpec[] = [
	{
		id: 'lab',
		heading: 'Lab',
		grant: null,
		module: null,
		windows: [
			{ name: 'comparator', label: 'Gems Compare' },
			{ name: 'compass', label: 'Lab Compass' },
			{ name: 'pathstrip', label: 'Lab Map' },
			{ name: 'timer', label: 'Lab Timer' }
		]
	},
	{
		id: 'merc',
		heading: 'Merc',
		grant: 'merc',
		module: null,
		windows: [{ name: 'mercenary', label: 'Merc Verdict' }]
	},
	{
		id: 'temple',
		heading: 'Temple',
		grant: 'temple',
		module: TEMPLE_WINDOW_LABEL,
		windows: []
	}
];

/**
 * The groups this device should see, with each module's widgets read from the
 * registry.
 *
 * Every group in `GROUPS` ships something, so there is no empty-group filter
 * here: a guard against a case the table cannot produce is a branch no test can
 * reach and no reader can check. `overlay-groups.test.ts` pins that each group
 * that survives its grant has rows.
 */
export function overlayGroups(grants: OverlayGroupGrants): OverlayGroup[] {
	return GROUPS.filter((group) => group.grant === null || grants[group.grant]).map((group) => {
		const widgets = group.module === null ? [] : placeableWidgetsFor(group.module);
		return {
			id: group.id,
			heading: group.heading,
			windows: group.windows,
			widgets,
			configureModule: widgets.length > 0 ? group.module : null
		};
	});
}

/**
 * The configuration flows that can have an interactive window over the game
 * right now, as `SettingsPage.svelte` reads them.
 */
export interface OpenConfigFlows {
	/** An OCR region window is on screen (`overlayVisible`). */
	region: boolean;
	/** A per-window position COPY is on screen (`anyPositionOverlayOpen`). */
	position: boolean;
	/** A module's in-window widget config session is running
	 *  (`widgetConfiguring`). */
	widgets: boolean;
}

/**
 * Whether Overlay Positions may START another Configure flow.
 *
 * The three flows are mutually exclusive, and the reason is the same for every
 * pair: each one makes a DIFFERENT window interactive over the game, and each
 * ends only through its own Save/Cancel. A second one started on top leaves two
 * click-eating rectangles over the game, and whichever bar the user reaches
 * stands down only one of them — the other is left interactive with its
 * controls behind the first. The page's `overlay-save`/`overlay-cancel` handler
 * cannot untangle them either: it dispatches to the FIRST open flow it finds.
 *
 * So the answer is one boolean for both buttons rather than a rule per control.
 * The row whose own flow is open does not need it — that row draws Save/Cancel
 * instead of Configure — and this is what disables all the others.
 *
 * A pure function rather than an inline `disabled={…}` because `.svelte` has no
 * unit-test harness in this app, and a missing term here fails as a second
 * window over the game that no gate can see.
 */
export function canStartConfigure(open: OpenConfigFlows): boolean {
	return !open.region && !open.position && !open.widgets;
}

/**
 * The geometry line for one widget row.
 *
 * Three answers, because a widget has three states and two of them used to be
 * spelled the same way. NO ROW is "Not set" — the widget is wherever the
 * registry ships it. A row with a ZERO size is placed but CONTENT-SIZED, which
 * is what Save writes for a widget the user moved but never resized
 * (`sizeToPersist`); printing that as `0×0` would read as a widget collapsed to
 * nothing. Anything else prints the stored rectangle.
 *
 * The "both dimensions" test is `placementFor`'s, deliberately: whatever this
 * line calls sized has to be what the host actually applies as a size, or the
 * row describes a widget the player is not looking at.
 *
 * The numbers are PHYSICAL pixels, like the five window rows above them —
 * `WidgetGeometry` is what Rust persists, and no conversion happens on the way
 * to this string.
 */
export function widgetGeometryText(geometry: WidgetGeometry | undefined): string {
	if (!geometry) return 'Not set';
	const at = `(${geometry.x}, ${geometry.y})`;
	if (geometry.width > 0 && geometry.height > 0) {
		return `${at} ${geometry.width}×${geometry.height}`;
	}
	return `${at} content-sized`;
}
