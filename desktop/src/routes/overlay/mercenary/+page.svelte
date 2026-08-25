<script lang="ts">
	/**
	 * Merc verdict overlay (POE-199) — a DISPLAY-ONLY overlay window.
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
	 * 6. **Error visibility** — this file has no failure path of its own: it
	 *    invokes nothing and listens to nothing. Every operation that CAN fail
	 *    (build, position, click-through, teardown) belongs to the owning window
	 *    and logs there, through `app_log_from_frontend`.
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
	 * `set_overlay_clickthrough(interactiveWidth: 0)` in the layout and
	 * `pointer-events: none` below — the two halves of one promise.
	 *
	 * `onMount` is not reliable in an overlay window and cross-window JS state is
	 * not either, so everything comes from the Rust-backed `get_ssot` poll that
	 * `routes/overlay/+layout.svelte` starts for every overlay route — this file
	 * only reads the rune.
	 *
	 * The verdict itself is computed HERE, from the same pure engine the page
	 * uses (`evaluateCapture`), against the same enabled-guide set (Rust's
	 * `sourcesOff` echo). It is never stored in Rust: it is a function of the
	 * capture, the rulesets, the guide toggles and the league, and any stored
	 * copy would be one poll away from lying. Same inputs on both windows is
	 * what makes the page and this strip agree (POE-199 L5).
	 */
	import { ssot } from '$lib/stores/ssot.svelte';
	import { MERC_SOURCES } from '$lib/mercenaries/rulesets';
	import { enabledSources } from '$lib/mercenaries/merc-prefs';
	import { evaluateCapture } from '$lib/mercenaries/verdict';
	import {
		WINDOW_GONE_NOTE,
		captureRetired,
		guideLines,
		headerLine,
		overlayShowsVerdict,
		unreadNote
	} from '$lib/mercenaries/overlay-view';

	const merc = $derived(ssot.mercenary);
	const capture = $derived(merc.capture);
	const visible = $derived(overlayShowsVerdict(merc));
	const enabled = $derived(enabledSources(merc.sourcesOff));
	const verdict = $derived(
		capture === null ? null : evaluateCapture(capture, MERC_SOURCES, enabled, ssot.league)
	);
	const lines = $derived(guideLines(verdict));
	const retired = $derived(captureRetired(merc));
	const unread = $derived(capture === null ? null : unreadNote(capture));
</script>

<div class="overlay-root">
	{#if visible && capture}
		<div class="panel">
			<p class="header">{headerLine(capture)}</p>

			{#each lines as line (line.id)}
				<p class="guide">
					<span class="guide-name">{line.label}</span>
					<span class="badge tone-{line.tone}">{line.headline}</span>
					{#if line.detail}<span class="detail">{line.detail}</span>{/if}
				</p>
			{/each}
			{#if lines.length === 0}
				<!-- Every guide switched off: the strip says so rather than
				     drawing an empty panel that looks like a broken read. -->
				<p class="note">every guide switched off — no verdict</p>
			{/if}

			<!-- What the read could not settle, on the surface the player decides
			     from. Compact, but never dropped. -->
			{#if unread}
				<p class="note">{unread}</p>
			{/if}
			{#if retired}
				<p class="note">{WINDOW_GONE_NOTE}</p>
			{/if}
		</div>
	{/if}
</div>

<style>
	/* Click-through is installed in Rust (`set_overlay_clickthrough`, interactive
	   width 0); `pointer-events: none` is the webview half of the same promise.
	   A click reaching this window would take focus, and a focused own-window
	   stops the capture loop that produces the verdict. */
	.overlay-root {
		position: fixed;
		inset: 0;
		pointer-events: none;
		background: transparent;
	}

	.panel {
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: 8px 10px;
		background: rgb(15 17 23 / 82%);
		border: 1px solid var(--color-lab-border);
		border-radius: 6px;
		color: var(--color-lab-text);
		font-size: 13px;
	}

	.header {
		font-size: 14px;
		font-weight: 700;
		color: var(--color-lab-text);
	}

	.guide {
		display: flex;
		align-items: baseline;
		gap: 6px;
		min-width: 0;
	}

	.guide-name {
		font-size: 12px;
		color: var(--color-lab-text-secondary);
	}

	.badge {
		font-size: 12px;
		font-weight: 700;
		letter-spacing: 0.05em;
	}

	.detail {
		font-size: 11px;
		color: var(--color-lab-text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
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
		line-height: 1.3;
		color: var(--color-lab-yellow);
	}
</style>
