<script lang="ts">
	/**
	 * Merc verdict overlay (POE-199) — a DISPLAY-ONLY overlay window.
	 *
	 * It draws in two parts, and the split is the 2026-08-25 smoke fix. The
	 * STATUS line draws for every running status, capture or not ("I have no
	 * idea whether something is being captured or not, and I have to constantly
	 * alt-tab"); the verdict block draws on top of it once there is a capture.
	 * The panel as a whole is on screen for as long as a recruit window is being
	 * worked — scanning, reading, done — and for `LINGER_MS` (4 s) after the
	 * module goes idle, whether that is "window gone" over a retired capture or
	 * "waiting" over nothing; then it clears entirely (owner decision,
	 * 2026-09-01). Both gates, the linger and every word live in
	 * `$lib/mercenaries/overlay-view`, which has a unit-test harness this file
	 * does not; this file owns only the clock — see "Idle linger" below.
	 *
	 * Read `docs/OVERLAY-GUIDE.md` before changing anything here. The guards this
	 * window is bound by, and where each is satisfied:
	 *
	 * 1. **Capabilities** — the `mercenary` label is in
	 *    `src-tauri/capabilities/default.json`'s `windows` list (with
	 *    `overlay-mercenary-pos`, the position-config window). Without them the
	 *    Tauri APIs the owning layout calls are simply unavailable.
	 * 2. **Physical persistence / 3. logical construction** — both live in the
	 *    owning window (`routes/(app)/+layout.svelte`), which converts the
	 *    persisted physical geometry with Tauri's `scaleFactor()` for the
	 *    constructor and then applies exact `PhysicalPosition`/`PhysicalSize`.
	 *    Nothing in this file touches geometry, and `window.devicePixelRatio`
	 *    appears nowhere.
	 * 4. **Move instead of recreate** — repositioning goes through Rust's
	 *    `move_overlay` from the Settings position flow; this window never
	 *    destroys or recreates itself.
	 * 5. **Settings survival** — the position IS persisted (`mercenary_overlay`
	 *    in `settings.rs`, copied by `persist_overlay_settings` and covered by
	 *    `test_overlay_settings_survive_persist_cycle`). Unlike the temple
	 *    overlay, this strip is placed by the user and must come back where they
	 *    left it.
	 * 6. **Error visibility** — this file has exactly ONE failure path, the
	 *    content-height resize below, and it logs through
	 *    `app_log_from_frontend` like every other overlay path. Everything else
	 *    that CAN fail (build, position, click-through, teardown) belongs to the
	 *    owning window and logs there.
	 *
	 * **Click-through is not a detail here, it is the feature working.** The
	 * capture loop gates on `AppState.game_in_foreground` — the RAW foreground
	 * read — while this window is shown and hidden on `game_focused`, which is
	 * HELD over our own windows so an overlay click does not blank every overlay.
	 * The two are deliberately never unified (`lib.rs`, the two fields; the split
	 * is stated in the focus poller). What follows from it is that this window
	 * must never take focus: a focused own-window makes the raw flag false and
	 * the capture loop stops reading the screen, so an interactive verdict strip
	 * would switch off the thing producing the verdict. Hence
	 * `set_overlay_clickthrough` in the layout with no hot rects ever declared,
	 * and `pointer-events: none` below — the two halves of one promise.
	 *
	 * `onMount` is not reliable in an overlay window and cross-window JS state is
	 * not either, so everything comes from the Rust-backed `get_ssot` poll that
	 * `routes/overlay/+layout.svelte` starts for every overlay route — this file
	 * reads the rune, and uses a `$effect` rather than `onMount` for the one
	 * piece of setup it owns.
	 *
	 * # Height follows content
	 *
	 * The strip has NO persisted height (owner decision, 2026-08-25). It draws a
	 * status line, a header, a line per guide and a line per row, so its height
	 * is a function of what the read found; and a shipped number is additionally
	 * wrong on every display that scales, because a height budget is reasoned in
	 * CSS pixels and Tauri applies physical ones. So the panel measures itself
	 * and Rust resizes the window to fit — `fit_overlay_height`, which converts
	 * with the WINDOW's own scale factor, clamps to the monitor work area so the
	 * strip can never grow over the taskbar, and re-applies the position because
	 * the WebView2 transparency resize workaround has been observed to disturb
	 * it. Width and position are never touched by this path; they stay whatever
	 * the user set in Settings → Overlay Positions — the PANEL shrink-wraps to
	 * its content inside that width (see `.panel`), the window does not.
	 *
	 * This cannot oscillate. The panel's height is content-driven and its width
	 * comes from the fixed-inset root, so a height change never re-wraps text
	 * and never feeds back into the measurement. The guard against a resize
	 * storm is `overlayHeightRequest`, not that argument.
	 *
	 * The verdict itself is computed HERE, from the same pure engine the page
	 * uses (`evaluateCapture`), against the same enabled-guide set (Rust's
	 * `sourcesOff` echo). It is never stored in Rust: it is a function of the
	 * capture, the rulesets, the guide toggles and the league, and any stored
	 * copy would be one poll away from lying. Same inputs on both windows is
	 * what makes the page and this strip agree (POE-199 L5).
	 */
	import { untrack } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { ssot } from '$lib/stores/ssot.svelte';
	import { MERCENARY_WINDOW_LABEL } from '$lib/overlay/manager';
	import { overlayHeightRequest } from '$lib/overlay/content-height';
	import { MERC_SOURCES } from '$lib/mercenaries/rulesets';
	import { enabledSources } from '$lib/mercenaries/merc-prefs';
	import { evaluateCapture } from '$lib/mercenaries/verdict';
	import {
		lingerAdvance,
		lingerInit,
		lingerRemainingMs,
		overlayHeader,
		overlayShowsVerdict,
		overlayShown,
		liveRowGlyphs,
		statusLine,
		statusPulse,
		stripSide,
		unreadNote,
		verdictBlock,
		type StripSide
	} from '$lib/mercenaries/overlay-view';

	const merc = $derived(ssot.mercenary);
	const capture = $derived(merc.capture);

	// --- Idle linger (see the header) ---
	//
	// The gate is pure and the clock is the one input the slice cannot carry, so
	// this route owns exactly two things: the linger record, advanced whenever
	// the polled status changes, and ONE timeout that re-reads the clock when
	// the linger runs out — without it nothing would re-evaluate the gate until
	// the next SSOT poll, up to three seconds late, and reading the clock every
	// frame would redraw a static panel for nothing.
	//
	// The timeout is armed for the REMAINDER, not for a fixed four seconds: the
	// effect re-runs on every poll that replaces the slice, and a fixed delay
	// re-armed every three seconds would never fire.
	let linger = $state(lingerInit());
	let nowMs = $state(Date.now());
	$effect(() => {
		const status = merc.status;
		const next = untrack(() => lingerAdvance(linger, merc, Date.now()));
		untrack(() => {
			linger = next;
			nowMs = Date.now();
		});
		const remaining = lingerRemainingMs(next, Date.now());
		// One guard, not three paths. `remaining` cannot be null while the status
		// is `idle` — `lingerAdvance` leaves `idleSinceMs` set on every idle slice
		// — so the null arm only narrows the type for the non-idle statuses. `0`
		// means the linger already ran out and the derived gate already reads as
		// hidden, so arming there would rewrite `nowMs` on every poll for nothing.
		if (status !== 'idle' || remaining === null || remaining === 0) return;
		// 50 ms PAST the boundary: a timer that fires exactly on it would read the
		// clock as one tick short of expired and leave the strip up until the next
		// poll.
		const timer = setTimeout(() => {
			nowMs = Date.now();
		}, remaining + 50);
		return () => clearTimeout(timer);
	});

	const shown = $derived(overlayShown(merc, linger, nowMs));
	const showsVerdict = $derived(overlayShowsVerdict(merc));
	const status = $derived(statusLine(merc));
	const enabled = $derived(enabledSources(merc.sourcesOff));
	// Not on a first look: the icons are unread, and `verdictBlock` says the
	// verdict is pending instead (`MercCapture.partial`).
	const verdict = $derived(
		capture === null || capture.partial
			? null
			: evaluateCapture(capture, MERC_SOURCES, enabled, ssot.league)
	);
	// ONE headline for every enabled guide, not one per guide: the strip used to
	// spend two lines saying SKIP twice (2026-08-25 smoke). The page keeps the
	// full per-guide view; the chips under a WORTH are the rungs to comp against.
	const block = $derived(verdictBlock(verdict, capture));
	const pulse = $derived(statusPulse(merc));
	const unread = $derived(unreadNote(merc));
	// WHICH cells still need a hover, not just how many. The live-only gate is
	// inside `liveRowGlyphs`, where it is tested.
	const glyphRows = $derived(liveRowGlyphs(merc));

	// --- Which edge the panel hugs ---
	//
	// The panel shrink-wraps inside a window the user sized, so the slack is on
	// one side; it should face the screen's middle, where the game is. The
	// side is `stripSide`'s (tested); this route only fetches the two inputs
	// the slice cannot carry — the window's own position and size, in the
	// physical pixels the screen slice is measured in.
	//
	// Re-measured on EVERY poll rather than on a move event: Settings moves this
	// window with `move_overlay` without recreating it, and the guide records
	// window events as unreliable in overlay windows. Two IPC calls every three
	// seconds is the price of never being stuck on the wrong edge.
	let side = $state<StripSide>('left');
	let sideFailureReported = false;
	$effect(() => {
		void merc;
		const screen = ssot.screen;
		const win = getCurrentWindow();
		Promise.all([win.outerPosition(), win.outerSize()])
			.then(([pos, size]) => {
				side = stripSide({ x: pos.x, width: size.width }, screen);
			})
			.catch((e) => {
				// Guard 6, once: the panel stays on the left, which is what it
				// always did, and the log says why it never moves.
				if (sideFailureReported) return;
				sideFailureReported = true;
				logMerc(`own position unreadable, the panel stays left-aligned: ${e}`);
			});
	});

	// --- Height follows content (see the header) ---

	let panelEl = $state<HTMLElement | null>(null);
	/** The last CSS height this window ASKED Rust for. See `overlayHeightRequest`. */
	let lastSentHeight: number | null = null;
	/** Whether a measurement is already queued for the next frame. */
	let framePending = false;

	function logMerc(msg: string): void {
		console.warn(`[overlay] merc strip: ${msg}`);
		invoke('app_log_from_frontend', { msg: `[merc-overlay] ${msg}` })
			.catch(e => console.error('[overlay] merc strip: app log unreachable:', e));
	}

	/**
	 * Measure and, if it moved, ask Rust to refit. Runs at most once per frame.
	 *
	 * Measured inside the animation frame rather than from the observer entry so
	 * a burst of mutations in one tick produces ONE reading of the settled
	 * layout instead of one call per mutation.
	 */
	function refit(): void {
		framePending = false;
		if (panelEl === null) return;
		const request = overlayHeightRequest(panelEl.getBoundingClientRect().height, lastSentHeight);
		if (request === null) return;
		lastSentHeight = request;
		invoke<number>('fit_overlay_height', {
			label: MERCENARY_WINDOW_LABEL,
			contentHeight: request
		}).then(applied => {
			// The command answers with the height it actually applied, back in
			// CSS pixels. A smaller one means the monitor work area would not fit
			// the strip, so the last rows ARE clipped — the player is looking at
			// a partial verdict and nothing else on screen would say so. Reported
			// once, not per frame: `overlayHeightRequest` only lets a genuinely
			// changed height get this far.
			if (applied < request - 1) {
				logMerc(
					`content wants ${Math.round(request)} css px, work area allows ${Math.round(applied)} — the last rows are clipped`
				);
			}
		}).catch(e => {
			// Guard 6. A strip stuck at its constructor seed is a clipped verdict,
			// which looks like a bad OCR read rather than a window that failed to
			// resize — so this must not be silent. The failed height is cleared so
			// the next observation retries rather than being deduped away.
			lastSentHeight = null;
			logMerc(`fit_overlay_height(${request}) failed: ${e}`);
		});
	}

	$effect(() => {
		const el = panelEl;
		if (el === null) return;
		const observer = new ResizeObserver(() => {
			if (framePending) return;
			framePending = true;
			requestAnimationFrame(refit);
		});
		observer.observe(el);
		return () => {
			observer.disconnect();
			framePending = false;
		};
	});
</script>

<div class="overlay-root side-{side}">
	{#if shown}
		<div class="panel" bind:this={panelEl}>
			{#if showsVerdict && capture}
				{@const header = overlayHeader(capture)}
				<div class="header">
					<p class="name">{header.name}</p>
					<p class="detail">{header.detail}</p>
				</div>

				<!-- The verdict: the headline on its own line, then one chip per
				     rung that said WORTH. Every case it can be in — including
				     "no guides enabled" — is worded in `overlay-view`. -->
				{#if block}
					<div class="verdict">
						<p class="headline tone-{block.tone}">{block.headline}</p>
						{#if block.chips.length > 0}
							<div class="chips">
								{#each block.chips as chip, i (i)}
									<span class="chip">
										<span class="chip-guide">{chip.guide}</span>
										<span class="chip-ruleset">{chip.ruleset}</span>
										{#if chip.tierLabel}<span class="tier tier-{chip.tier}">{chip.tierLabel}</span>{/if}
									</span>
								{/each}
							</div>
						{/if}
					</div>
				{/if}

				<!-- The rows in the panel's own order, cells left-aligned in slot
				     order at a square rhythm, so the second cell of a row IS the
				     second icon of that row in the game (2026-09-07 redesign: the
				     old right-aligned glyph run read against the panel's
				     left-aligned icons). -->
				{#if glyphRows.length > 0}
					<div class="rows">
						{#each glyphRows as glyphRow (glyphRow.index)}
							<p class="row-skill">{glyphRow.skill}</p>
							<div class="cells">
								<!-- One box per cell so each carries its own tone: ✓
								     read, ? would be settled by a hover, ✕ not read
								     at all. -->
								{#each glyphRow.glyphs as cell, slot (slot)}
									<span class="cell tone-{cell.tone}">{cell.glyph}</span>
								{/each}
								{#if glyphRow.note}<span class="row-note">{glyphRow.note}</span>{/if}
							</div>
						{/each}
					</div>
				{/if}

				<!-- What the read could not settle, on the surface the player
				     decides from. Compact, but never dropped. -->
				{#if unread}
					<p class="note">{unread}</p>
				{/if}
			{/if}

			<!-- The module's pulse. Drawn for every running status, capture or
			     not — an empty overlay and a dead one used to look the same. At
			     the foot since the redesign: the verdict is what the player
			     reads first, the pulse is what they glance at. -->
			{#if status}
				<p class="status">
					<span class="dot pulse-{pulse}"></span>
					<span>{status}</span>
				</p>
			{/if}
		</div>
	{/if}
</div>

<style>
	/* Click-through is installed in Rust (`set_overlay_clickthrough`; this window
	   declares no hot rects, so the hook never claims a click from it);
	   `pointer-events: none` is the webview half of the same promise. A click
	   reaching this window would take focus, and a focused own-window stops the
	   capture loop that produces the verdict. */
	.overlay-root {
		position: fixed;
		inset: 0;
		pointer-events: none;
		background: transparent;
	}

	/* The font is set HERE: the overlay layout imports the palette alone, so
	   without it the strip fell to WebView2's default serif (seen on every
	   screenshot before the 2026-09-07 redesign). The stack is `app.css`'s.

	   Width follows content too, INSIDE the window: the panel shrink-wraps to
	   its widest line and the rest of the window stays transparent, so the
	   width set in Settings → Overlay Positions is a cap, not the drawn width
	   (owner: "that big gap to the right", 2026-09-07). The window itself is
	   never resized on this axis — see "Height follows content" above — and a
	   name longer than the cap wraps rather than pushing the panel past it. */
	.panel {
		display: flex;
		flex-direction: column;
		gap: 6px;
		width: max-content;
		max-width: 100%;
		padding: 8px 10px;
		background: rgb(15 17 23 / 82%);
		border: 1px solid var(--color-lab-border);
		border-radius: 6px;
		color: var(--color-lab-text);
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
		font-size: 13px;
		line-height: 1.3;
	}

	/* The slack faces the screen's middle — see "Which edge the panel hugs". */
	.side-right .panel {
		margin-left: auto;
	}

	.header {
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}

	/* Wraps rather than clips: a mangled name is the clue the header read is
	   wrong, and an ellipsis would hide it. */
	.name {
		font-size: 13px;
		font-weight: 700;
		line-height: 1.25;
		color: var(--color-lab-text);
		overflow-wrap: anywhere;
	}

	.detail {
		font-size: 11px;
		color: var(--color-lab-text-secondary);
	}

	.verdict {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	/* The answer, as a tinted bar: the one element on the strip that means act
	   now, so it gets the one filled surface. The tint follows the tone. */
	.headline {
		padding: 3px 8px;
		border-radius: 3px;
		border: 1px solid currentcolor;
		font-size: 12px;
		font-weight: 700;
		letter-spacing: 0.05em;
	}

	.headline.tone-pass {
		background: rgba(34, 197, 94, 0.12);
		border-color: rgba(34, 197, 94, 0.45);
	}

	.headline.tone-fail {
		background: rgba(239, 68, 68, 0.12);
		border-color: rgba(239, 68, 68, 0.45);
	}

	.headline.tone-unknown {
		background: rgba(234, 179, 8, 0.12);
		border-color: rgba(234, 179, 8, 0.45);
	}

	.headline.tone-muted {
		background: rgba(136, 136, 136, 0.1);
		border-color: rgba(136, 136, 136, 0.35);
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 1px 6px;
		border-radius: 3px;
		border: 1px solid rgba(34, 197, 94, 0.5);
		font-size: 10.5px;
		line-height: 16px;
		white-space: nowrap;
	}

	.chip-guide {
		font-weight: 600;
		color: var(--color-lab-text);
	}

	.chip-ruleset {
		color: var(--color-lab-text-secondary);
	}

	/* The rung's tier as a tag whose fill deepens up the ladder — mv, mid, end,
	   gg (`rulesets.ts` TIERS) — so the grade reads at a glance before the word
	   does. A tier key outside the four gets the outline alone. */
	.tier {
		padding: 0 4px;
		border-radius: 2px;
		font-size: 9px;
		font-weight: 700;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		line-height: 14px;
		border: 1px solid rgba(34, 197, 94, 0.5);
		color: var(--color-lab-text);
	}

	.tier-mv {
		background: rgba(34, 197, 94, 0.18);
	}

	.tier-mid {
		background: rgba(34, 197, 94, 0.35);
	}

	.tier-end {
		background: rgba(34, 197, 94, 0.6);
		color: var(--color-lab-bg);
	}

	.tier-gg {
		background: rgba(34, 197, 94, 0.9);
		color: var(--color-lab-bg);
	}

	/* Name column, then the cells. The name never pushes the cells around: it
	   is the cells' left edge that has to line up from row to row. */
	.rows {
		display: grid;
		grid-template-columns: 140px minmax(0, 1fr);
		column-gap: 10px;
		row-gap: 5px;
		align-items: center;
	}

	.row-skill {
		font-size: 11px;
		color: var(--color-lab-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.cells {
		display: flex;
		align-items: center;
		gap: 5px;
		min-width: 0;
	}

	/* 22 px boxes at a 27 px pitch, matching the 5 px row gap under 22 px rows:
	   a square rhythm like the panel's, whose cell pitch equals its row pitch. */
	.cell {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		flex: 0 0 auto;
		border-radius: 3px;
		border: 1px solid currentcolor;
		font-size: 11px;
		font-weight: 700;
	}

	.cell.tone-pass {
		background: rgba(34, 197, 94, 0.12);
		border-color: rgba(34, 197, 94, 0.55);
	}

	.cell.tone-unknown {
		background: rgba(234, 179, 8, 0.16);
		border-color: rgba(234, 179, 8, 0.7);
	}

	.cell.tone-fail {
		background: rgba(239, 68, 68, 0.14);
		border-color: rgba(239, 68, 68, 0.6);
	}

	/* A row the panel shows with no supports, or one whose icons are still
	   being read. Muted, because nothing failed here — the `✕` box is what
	   "there is a cell I could not read" looks like. */
	.row-note {
		font-size: 11px;
		line-height: 22px;
		color: var(--color-lab-text-muted);
	}

	/* The same three buckets the page paints its headlines in. */
	.tone-pass {
		color: var(--color-lab-green);
	}

	.tone-fail {
		color: var(--color-lab-red);
	}

	.tone-unknown {
		color: var(--color-lab-yellow);
	}

	.tone-bonus,
	.tone-muted {
		color: var(--color-lab-text-muted);
	}

	/* One colour for "this is not settled" across every overlay in the app. */
	.note {
		font-size: 11px;
		color: var(--color-lab-yellow);
	}

	/* The pulse line. Muted rather than coloured: it is always present, so a
	   colour here would compete with the headline and the honesty note, which
	   are the two things on this strip that mean act now. The dot is what
	   says "alive". */
	.status {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 11px;
		color: var(--color-lab-text-secondary);
	}

	.dot {
		width: 6px;
		height: 6px;
		flex: 0 0 auto;
		border-radius: 50%;
		background: var(--color-lab-green);
	}

	.dot.pulse-beating {
		animation: beat 1.6s ease-in-out infinite;
	}

	.dot.pulse-off {
		background: var(--color-lab-text-muted);
	}

	@keyframes beat {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.35;
		}
	}
</style>
