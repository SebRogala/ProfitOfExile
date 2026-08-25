/**
 * What the merc verdict overlay says, in one testable place (POE-199).
 *
 * The overlay is the surface the player reads WHILE the recruit window is open,
 * so it carries the same wording rules the page does and one extra: it is the
 * compact form. One line per enabled guide, one header line, one honesty line
 * for what the reader could not settle. Nothing here decides anything —
 * `verdict.ts` owns every outcome and `capture-view.ts` owns the vocabulary
 * (`HEADLINE_LABEL`, `HEADLINE_TONE`), which this file reuses rather than
 * respells.
 *
 * It lives outside the route for the reason every view module in this app does:
 * a `.svelte` file has no unit-test harness here, and the wording is the part
 * that can be wrong quietly.
 *
 * **Uncertainty is printed, never hidden** — the same rule the page and the
 * temple overlay follow. A retired capture keeps its verdict and says the
 * window is gone; unread icons are counted out loud, because an icon the reader
 * could not identify is the difference between a SKIP and a WORTH.
 */

import { isConfident, type MercSourceVerdict, type MercVerdict } from './verdict';
import { HEADLINE_LABEL, HEADLINE_TONE, type OutcomeTone } from './capture-view';
import type { MercCapture, MercStatus, MercenarySlice } from './capture';

/**
 * The statuses in which the overlay has something worth drawing.
 *
 * `off` and `unavailable` are the two that mean the module is not producing
 * verdicts at all; drawing a strip in either would leave a panel over the game
 * that no longer tracks anything. The three RUNNING states all draw — including
 * `idle`, which is where the loop returns after a capture is retired, and where
 * the "window gone" marker does its work.
 */
export const OVERLAY_VISIBLE_STATUSES: MercStatus[] = ['idle', 'scanning', 'live'];

/**
 * Whether the overlay draws at all.
 *
 * Two conditions, and both are needed: the module must be producing verdicts,
 * and there must be a capture to word. A running module with nothing captured
 * yet draws NOTHING rather than an empty frame — the player has not opened a
 * recruit window, and a panel sitting over the game saying so is noise during
 * every map.
 *
 * Whether the window is on SCREEN is a different gate in a different place: the
 * Rust focus poller shows and hides this window with the game, next to the
 * comparator and the temple.
 */
export function overlayShowsVerdict(slice: MercenarySlice): boolean {
	return OVERLAY_VISIBLE_STATUSES.includes(slice.status) && slice.capture !== null;
}

/**
 * Whether the recruit window has gone away since this capture was taken.
 *
 * The same rule the page's `captureLive` uses, and the same precedence: the
 * STATUS is authoritative and `capture.live` only agrees. A retired capture is
 * kept and marked rather than dropped (temple precedent: surface uncertainty,
 * do not hide it) — the player may still be deciding on the mercenary they just
 * closed the window on.
 */
export function captureRetired(slice: MercenarySlice): boolean {
	if (slice.capture === null) return false;
	return !(slice.status === 'live' && slice.capture.live === true);
}

/** What the strip says about a capture whose window is gone. */
export const WINDOW_GONE_NOTE = 'recruit window gone — last read';

/**
 * The header line: who this is, as far as the reader got.
 *
 * Every field is best-effort in the capture, and a field that was not read says
 * so rather than being dropped — a strip reading `Nytra · lvl 68` with the class
 * silently missing looks like a mercenary with no class, which is not a thing.
 */
export function headerLine(capture: MercCapture): string {
	const header = capture.header;
	return [
		header.name ?? 'name not read',
		header.class ?? 'class not read',
		header.level === null ? 'level not read' : `lvl ${header.level}`
	].join(' · ');
}

/** One guide's line on the strip. */
export interface OverlayGuideLine {
	id: string;
	/** The guide's own name — `Guide A`. */
	label: string;
	/** `WORTH` / `SKIP` / `UNKNOWN`, from the page's own vocabulary. */
	headline: string;
	/** The page's colour bucket for that headline. */
	tone: OutcomeTone;
	/**
	 * What passed, for a WORTH: the ruleset names with their tiers, joined.
	 * Empty for every other headline — there is nothing to name.
	 */
	detail: string;
}

/**
 * One line per ENABLED guide, in the order the guides are declared.
 *
 * A guide the user switched off is left out entirely rather than shown as OFF:
 * the page has room to say "switched off in Settings", the strip over the game
 * does not, and a line that is only ever the word OFF costs a line of the
 * player's screen for nothing. The verdict's own `off` headline is what this
 * filters on, so the page and the overlay agree about which guides are in play
 * without either re-deriving the enabled set.
 */
export function guideLines(verdict: MercVerdict | null): OverlayGuideLine[] {
	if (verdict === null) return [];
	return verdict.sources
		.filter((source) => source.headline !== 'off')
		.map((source) => ({
			id: source.id,
			label: source.label,
			headline: HEADLINE_LABEL[source.headline],
			tone: HEADLINE_TONE[source.headline],
			detail: bestDetail(source)
		}));
}

/**
 * What a WORTH is worth — the passing rulesets by name, tier included.
 *
 * The tier is the point on guide B's ladder: "WORTH" alone does not tell the
 * player whether this is the cheapest rung or the top one, which is the whole
 * question when deciding what to pay. Untiered rulesets print their bare name.
 */
function bestDetail(source: MercSourceVerdict): string {
	if (source.headline !== 'worth') return '';
	return source.rulesets
		.filter((ruleset) => source.best.includes(ruleset.id))
		.map((ruleset) => (ruleset.tier ? `${ruleset.label} (${ruleset.tier})` : ruleset.label))
		.join(', ');
}

/**
 * How many support cells the reader could not identify.
 *
 * Counted on the two CONFIDENT states from `verdict.ts` rather than on a list
 * of the other three, so a state added later counts as unread until someone
 * decides otherwise — the fail-closed direction. Support cells only: they are
 * the icons a hover can confirm, and the count exists to tell the player that
 * hovering would settle something.
 */
export function unreadIconCount(capture: MercCapture): number {
	return capture.rows.reduce(
		(total, row) => total + row.supports.filter((cell) => !isConfident(cell.state)).length,
		0
	);
}

/**
 * The unread line, or null when everything was read.
 *
 * Null rather than an empty string so the route renders nothing at all: a
 * strip that always carries a line about unread icons trains the player to
 * ignore it, which is the one thing this line cannot afford.
 */
export function unreadNote(capture: MercCapture): string | null {
	const unread = unreadIconCount(capture);
	if (unread === 0) return null;
	return `${unread} ${unread === 1 ? 'icon' : 'icons'} unread — hover to confirm`;
}
