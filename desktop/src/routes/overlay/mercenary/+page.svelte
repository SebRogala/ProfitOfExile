<script lang="ts">
	/**
	 * Merc verdict overlay (POE-199) — a DISPLAY-ONLY widget.
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
	 *    `src-tauri/capabilities/default.json`'s `windows` list.
	 * 2. **Physical persistence / 3. logical construction** — the WINDOW is the
	 *    game monitor, built by `lib/overlay/widget-window.ts` (logical
	 *    constructor, exact physical position/size in `tauri://created`); the
	 *    widget's placement is converted by `WidgetHost.svelte` /
	 *    `widget-geometry.ts`. Nothing in this file touches geometry except the
	 *    side measurement below, which is CSS against CSS.
	 * 4. **Move instead of recreate** — the widget moves by changing its
	 *    in-window CSS position; the monitor-sized window never moves for it.
	 * 5. **Settings survival** — the widget placement IS persisted in
	 *    `Settings.widgets`, owned by the widget host and its Rust commands.
	 * 6. **Error visibility** — window setup belongs to the owning widget window
	 *    and logs there; this file invokes no fallible overlay commands.
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
	 * `set_overlay_clickthrough` in `lib/overlay/widget-window.ts` with no hot rects ever declared,
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
	 * is a function of what the read found. The widget host sizes its height to
	 * the content; its `resizable: 'width'` placement makes width the only
	 * setting. The monitor-sized window itself is not fitted or moved.
	 *
	 * The panel's height is therefore content-driven inside the widget box. Its
	 * wrapper fills the widget box's width, so a configured width lets the panel
	 * hug the appropriate screen edge while the widget host owns the box.
	 *
	 * The verdict itself is computed HERE, from the same pure engine the page
	 * uses (`evaluateCapture`), against the same enabled-guide set (Rust's
	 * `sourcesOff` echo). It is never stored in Rust: it is a function of the
	 * capture, the rulesets, the guide toggles and the league, and any stored
	 * copy would be one poll away from lying. Same inputs on both windows is
	 * what makes the page and this strip agree (POE-199 L5).
	 */
	import { untrack } from 'svelte';
	import { ssot } from '$lib/stores/ssot.svelte';
	import { MERCENARY_WINDOW_LABEL } from '$lib/overlay/manager';
	import WidgetHost from '$lib/overlay/widgets/WidgetHost.svelte';
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
	// The panel's wrapper is the widget box, so measure it against the monitor
	// window rather than asking the window for its own position and size. The
	// box always has a width — the stored one, or the shipped one until the user
	// drags an edge (`placementFor`, `resizable: 'width'`) — so there is slack
	// whenever the panel is narrower, and the side decides which edge it hugs.
	let side = $state<StripSide>('left');
	let wrapperEl = $state<HTMLElement | null>(null);
	$effect(() => {
		void merc;
		const rect = wrapperEl?.getBoundingClientRect();
		if (!rect) return;
		side = stripSide(
			{ x: rect.left, width: rect.width },
			{ origin: [0, 0], width: window.innerWidth }
		);
	});
</script>

<WidgetHost module={MERCENARY_WINDOW_LABEL}>
	{#snippet content(spec, configMode)}
		{#if spec.id === 'mercenary.verdict' && shown}
			<div class="overlay-root side-{side}" bind:this={wrapperEl}>
			<div class="panel">
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
			</div>
		{:else if configMode}
			<p class="placeholder">{spec.label}</p>
		{/if}
	{/snippet}
</WidgetHost>

<style>
	/* Click-through is installed in Rust (`set_overlay_clickthrough`; this window
	   declares no hot rects, so the hook never claims a click from it);
	   `pointer-events: none` is the webview half of the same promise. A click
	   reaching this window would take focus, and a focused own-window stops the
	   capture loop that produces the verdict. */
	.overlay-root {
		width: 100%;
		pointer-events: none;
		background: transparent;
	}

	/* The font is set HERE: the overlay layout imports the palette alone, so
	   without it the strip fell to WebView2's default serif (seen on every
	   screenshot before the 2026-09-07 redesign). The stack is `app.css`'s.

	   Width follows content too, INSIDE the widget box: the panel shrink-wraps to
	   its widest line and the rest of the box stays transparent, while a width
	   set in config mode restores the slack for the side decision. A name longer
	   than the width wraps rather than pushing the panel past it. */
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

	.placeholder {
		padding: 4px 8px;
		font-size: 11px;
		color: var(--color-lab-text-muted);
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
