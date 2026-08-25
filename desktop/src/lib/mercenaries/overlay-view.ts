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
 *
 * **The strip is also the module's only visible pulse.** Reported on the
 * 2026-08-25 smoke: "I have no idea whether something is being captured or not,
 * and I have to constantly alt-tab." So the visibility gate is now in two
 * parts — [`overlayVisible`] draws a one-line [`statusLine`] whenever the
 * module is running at all, and [`overlayShowsVerdict`] adds the verdict block
 * on top of it once there is a capture to word. An idle module used to draw
 * NOTHING, which is indistinguishable from an overlay that never got built.
 *
 * Each fact appears on exactly ONE line. The live status line carries the
 * unread count, so [`unreadNote`] withholds it there rather than printing it
 * twice — a strip that repeats itself teaches the reader to skip both lines.
 */

import { isConfident, type MercSourceVerdict, type MercVerdict } from './verdict';
import { HEADLINE_LABEL, HEADLINE_TONE, READ_GLYPH, type OutcomeTone } from './capture-view';
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
 * Whether the overlay draws ANYTHING — the outer gate.
 *
 * One condition: the module is running. A running module with nothing captured
 * yet draws the status strip and nothing else, which is the POE-199 smoke fix.
 * The previous rule required a capture too, so an armed-but-empty module was
 * pixel-identical to a module that had failed to start, and the only way to
 * tell them apart was to alt-tab to the page.
 *
 * `off` and `unavailable` still draw nothing: there is no loop behind them, so
 * a strip would be a panel over the game reporting on nothing.
 *
 * Whether the window is on SCREEN is a different gate in a different place: the
 * Rust focus poller shows and hides this window with the game, next to the
 * comparator and the temple.
 */
export function overlayVisible(slice: MercenarySlice): boolean {
	return OVERLAY_VISIBLE_STATUSES.includes(slice.status);
}

/**
 * Whether the VERDICT BLOCK draws — the header, the guide lines, the glyphs.
 *
 * The inner gate: everything this covers is a statement about a capture, so it
 * needs one. [`overlayVisible`] is the outer gate and the status line rides on
 * that alone.
 */
export function overlayShowsVerdict(slice: MercenarySlice): boolean {
	return overlayVisible(slice) && slice.capture !== null;
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
 * What the strip says while the module is on and nothing has triggered a look.
 *
 * It names the manual escape hatch, because this is the exact moment the player
 * wonders whether the thing is working: a mercenary they can see on screen that
 * produced no voice line the watcher caught. `Scan now` is on the page, not
 * here — this window can never be clicked (see the route's click-through note),
 * so the line says WHERE the button is rather than pretending to be one.
 */
export const IDLE_LINE = 'waiting for a mercenary · Scan now on the page';

/** What the strip says while a burst is armed and looking. */
export const SCANNING_LINE = 'scanning for the recruit window…';

/**
 * What a scanning line adds when the verdict below it is a PREVIOUS read.
 *
 * `scanning` outranks the window-gone marker because a burst armed after the
 * last window closed is the more current fact — but the verdict block does not
 * disappear while that burst runs, so plain `SCANNING_LINE` would leave a stale
 * verdict on screen with nothing saying it was stale. That is the exact failure
 * `WINDOW_GONE_NOTE` exists to prevent, so the mark is carried rather than
 * dropped: precedence decides the HEADLINE, not whether the reader is told.
 */
export const STALE_VERDICT_SUFFIX = ' · last read below';

/**
 * The unread count as a phrase, or null when there is nothing to hover.
 *
 * One home for the wording, because two lines print it: the live status line
 * carries it inline, and [`unreadNote`] carries it on its own line for a
 * capture the module is no longer reading.
 */
function unreadPhrase(unread: number): string | null {
	if (unread === 0) return null;
	return `${unread} ${unread === 1 ? 'icon' : 'icons'} unread — hover to confirm`;
}

/**
 * The one compact line the strip ALWAYS carries while the module is on.
 *
 * This is the POE-199 smoke fix: an idle module used to draw nothing at all, so
 * "the overlay is waiting" and "the overlay never got built" looked identical
 * and the only way to tell was to alt-tab. Every running status now says which
 * one it is, on one line, in the player's own terms.
 *
 * The order of the checks IS the precedence, and it is deliberate:
 *
 * 1. `live` with a capture — what the reader is looking at right now, counts
 *    included, so a cell still needing a hover is visible without alt-tabbing.
 * 2. `scanning` — something IS looking. This outranks the window-gone marker,
 *    because a burst armed after the last window closed is the more current
 *    fact; but the verdict block below is still showing the PREVIOUS read, so
 *    the line carries [`STALE_VERDICT_SUFFIX`] rather than dropping the mark.
 *    Precedence decides the headline, never whether the reader is told.
 * 3. any other running status holding a capture — [`captureRetired`] is asked
 *    rather than re-derived, so the marker and the rule cannot drift.
 * 4. nothing captured — waiting.
 *
 * `live` with a NULL capture falls to (4). Rust never publishes that pair
 * (`run.rs` sets the status and the capture in one write), and if it ever did,
 * "waiting" is the honest reading: nothing has been read.
 *
 * Null when the module is off or unavailable — there is nothing to report and
 * the route draws no panel.
 */
export function statusLine(slice: MercenarySlice): string | null {
	if (!overlayVisible(slice)) return null;
	const capture = slice.capture;
	if (capture !== null && slice.status === 'live') return liveLine(capture);
	if (slice.status === 'scanning') {
		return captureRetired(slice) ? SCANNING_LINE + STALE_VERDICT_SUFFIX : SCANNING_LINE;
	}
	if (captureRetired(slice)) return WINDOW_GONE_NOTE;
	return IDLE_LINE;
}

/**
 * The live line: how much was read, and how much of it is trustworthy.
 *
 * The row count is there because it is the cheap sanity check the player can
 * make against the screen — a six-row recruit window read as two rows is a
 * geometry problem, and the strip is where they will notice it. `all icons
 * read` is stated rather than left blank: silence would be indistinguishable
 * from a line that forgot to update.
 */
function liveLine(capture: MercCapture): string {
	const rows = capture.rows.length;
	const unread = unreadPhrase(unreadIconCount(capture));
	return `reading · ${rows} ${rows === 1 ? 'row' : 'rows'} · ${unread ?? 'all icons read'}`;
}

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
 * The unread line under the verdict, or null when there is nothing to say.
 *
 * Null rather than an empty string so the route renders nothing at all: a
 * strip that always carries a line about unread icons trains the player to
 * ignore it, which is the one thing this line cannot afford.
 *
 * Null under `live` too, and for the same reason one step further on — the
 * live [`statusLine`] already carries the count, and the per-row glyphs under
 * it say WHICH cells. Printing the total a second time would make the strip
 * repeat itself on the one status where it has the most to say.
 */
export function unreadNote(slice: MercenarySlice): string | null {
	if (slice.capture === null || slice.status === 'live') return null;
	return unreadPhrase(unreadIconCount(slice.capture));
}

/** One row's compact read, for the glyph strip under a live verdict. */
export interface OverlayRowGlyphs {
	/** The row's own index in the recruit window, top to bottom. */
	index: number;
	/** The skill name, or the marker for a name the OCR did not resolve. */
	skill: string;
	/**
	 * One [`READ_GLYPH`] per support cell, space-separated — `✓ ✓ ? ✕`.
	 * [`NO_CELLS_NOTE`] when the row has no cells at all.
	 */
	glyphs: string;
}

/** What a row with no support cells says instead of an empty glyph run. */
export const NO_CELLS_NOTE = 'no cells read';

/**
 * The per-row glyphs the live strip draws under the verdict (POE-199 smoke).
 *
 * The counts on the status line say HOW MANY cells still need a hover; this
 * says WHICH — the whole point, since the player has to put the cursor on a
 * specific icon and the alternative was alt-tabbing to the page to find out
 * which one. `READ_GLYPH` is reused rather than respelled so the strip and the
 * page cannot drift into two vocabularies for the same three states.
 *
 * Empty unless the module is `live` WITH a capture, and that gate is here
 * rather than in the route for the reason every gate in this file is: a
 * `.svelte` file has no unit-test harness in this app. The glyphs describe a
 * window that is on screen — a retired capture's cells cannot be hovered any
 * more, so offering them would be an instruction the player cannot follow.
 *
 * A row whose skill name did not resolve says so rather than printing its raw
 * OCR text: the raw string is noise on a compact strip, and a blank there would
 * read as a row with no skill.
 *
 * An empty `supports` is a real capture shape (a row the reader found but whose
 * cells it could not place), and an empty glyph string would render as a skill
 * with no supports — a different and wrong claim. Hence the marker.
 */
export function liveRowGlyphs(slice: MercenarySlice): OverlayRowGlyphs[] {
	if (slice.capture === null || slice.status !== 'live') return [];
	return rowGlyphs(slice.capture);
}

/**
 * The per-row glyphs for one capture, whatever the module is doing now.
 *
 * [`liveRowGlyphs`] is the gate the route uses; this is the mapping under it,
 * exported separately because the two are different things to get wrong and a
 * test for the glyph vocabulary should not have to build a slice.
 */
export function rowGlyphs(capture: MercCapture): OverlayRowGlyphs[] {
	return capture.rows.map((row) => ({
		index: row.index,
		skill: row.skill.name ?? 'skill not read',
		glyphs:
			row.supports.length === 0
				? NO_CELLS_NOTE
				: row.supports.map((cell) => READ_GLYPH[cell.state]).join(' ')
	}));
}
