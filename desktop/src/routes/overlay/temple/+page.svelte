<script lang="ts">
	/**
	 * Temple builder overlay (POE-171) — a DISPLAY-ONLY overlay window.
	 *
	 * Read `docs/OVERLAY-GUIDE.md` before changing anything here. The guards this
	 * window is bound by, and where each is satisfied:
	 *
	 * 1. **Capabilities** — the `temple` label is in
	 *    `src-tauri/capabilities/default.json`'s `windows` list. Without it the
	 *    Tauri APIs this window's layout calls are simply unavailable.
	 * 2. **Physical persistence / 3. logical construction** — both live in the
	 *    owning window (`routes/(app)/+layout.svelte::createTempleOverlay`),
	 *    which converts with Tauri's `scaleFactor()` for the constructor and
	 *    then applies exact `PhysicalPosition`/`PhysicalSize`. Nothing in this
	 *    file touches geometry, and `window.devicePixelRatio` appears nowhere.
	 * 4. **Move instead of recreate** — this window is created once per module
	 *    switch-on and destroyed on switch-off; it never destroys or recreates
	 *    itself, and it does not reposition.
	 * 5. **Settings survival** — deliberately NOT applicable: the temple overlay
	 *    persists no settings, so there is no field for `persist_overlay_settings`
	 *    to copy. See the note in the layout's `createTempleOverlay`.
	 * 6. **Error visibility** — this file has no failure path of its own to
	 *    report: it invokes nothing and listens to nothing. Every operation that
	 *    CAN fail (build, position, click-through, teardown) belongs to the
	 *    owning window and logs there, through `app_log_from_frontend`.
	 *
	 * `onMount` is not reliable in an overlay window and cross-window JS state is
	 * not either, so the board comes from the Rust-backed `get_ssot` poll that
	 * `routes/overlay/+layout.svelte` starts for every overlay route — this file
	 * only reads the rune.
	 *
	 * **Two gates, in two places, deliberately.** Whether the window is on screen
	 * at all is Rust's: the focus poller shows and hides the `temple` window with
	 * the game, next to the comparator, which is the mechanism the guide names
	 * and the only one that cannot be defeated by a webview that is slow to
	 * repaint. What is left here is the one thing Rust cannot answer — whether
	 * there is a board worth drawing — and that is `overlayShowsBoard(status)`.
	 *
	 * **What it draws when the read is imperfect is the point.** The unread
	 * plates, the fallback door read, a low-confidence panel and the advisor's
	 * own warnings all reach the overlay, because the surface the player is
	 * actually looking at while deciding is this one, and a recommendation that
	 * hides what it is uncertain about is the failure mode POE-171 is written
	 * against.
	 */
	import TempleLattice from '$lib/temple/TempleLattice.svelte';
	import {
		formatRisk,
		gambleLabel,
		leadReason,
		leaveMapBanner,
		markerFallbackNotice,
		moveLine,
		overlayShowsBoard,
		topGamble,
		topRecommendation,
		unknownRoomsBadge
	} from '$lib/temple/view';
	import { ssot } from '$lib/stores/ssot.svelte';

	const temple = $derived(ssot.temple);
	const visible = $derived(overlayShowsBoard(temple.status));
	const move = $derived(topRecommendation(temple.advice));
	const gamble = $derived(topGamble(temple.advice));
	const leaveBanner = $derived(leaveMapBanner(temple.advice));
	const recommendedDoors = $derived(move?.doors ?? []);
	const unknownBadge = $derived(unknownRoomsBadge(temple));
	const markerNotice = $derived(markerFallbackNotice(temple.layout));
	const lowConfidence = $derived(temple.layout?.confidence === 'low');
	const warnings = $derived(temple.advice?.warnings ?? []);
</script>

<div class="overlay-root">
	{#if visible}
		<div class="panel">
			<div class="board">
				<TempleLattice layout={temple.layout} highlightDoors={recommendedDoors} compact />
			</div>
			<div class="advice">
				{#if leaveBanner}
					<!-- As prominent as the kill: R5 says this map is done. -->
					<p class="leave">{leaveBanner}</p>
				{/if}
				{#if move}
					<p class="headline">{moveLine(move)}</p>
					{#if leadReason(move)}
						<p class="reason">{leadReason(move)}</p>
					{/if}
				{:else if temple.status === 'no_current_room'}
					<p class="reason">between rooms — layout only</p>
				{:else}
					<p class="reason">reading…</p>
				{/if}
				{#if gamble}
					<p class="gamble">
						<span class="gamble-tag">{gambleLabel(gamble)}</span>
						{gamble.headline} · {gamble.doorsLabel}
						{#if formatRisk(gamble.risk)}<span class="risk">{formatRisk(gamble.risk)}</span>{/if}
					</p>
				{/if}

				<!-- Everything the read could not settle, on the surface the
				     player decides from. Compact, but never dropped. -->
				{#if lowConfidence}
					<p class="warn">low-confidence panel read — do not act on the doors</p>
				{/if}
				{#if unknownBadge}
					<p class="warn">{unknownBadge}</p>
				{/if}
				{#if markerNotice}
					<p class="warn">{markerNotice}</p>
				{/if}
				{#each warnings as warning (warning)}
					<p class="warn">{warning}</p>
				{/each}
			</div>
		</div>
	{/if}
</div>

<style>
	/* Click-through is installed in Rust (`set_overlay_clickthrough`; this window
	   declares no hot rects, so the hook never claims a click from it);
	   `pointer-events: none` is the webview half of the same promise. */
	.overlay-root {
		position: fixed;
		inset: 0;
		pointer-events: none;
		background: transparent;
	}

	.panel {
		display: flex;
		gap: 10px;
		padding: 8px 10px;
		background: rgb(15 17 23 / 82%);
		border: 1px solid var(--color-lab-border);
		border-radius: 6px;
		color: var(--color-lab-text);
		font-size: 13px;
	}

	.board {
		flex: 0 0 200px;
	}

	.advice {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}

	.leave {
		padding: 3px 6px;
		font-weight: 700;
		color: var(--color-lab-bg);
		background: var(--color-lab-yellow);
		border-radius: 3px;
	}

	.headline {
		font-size: 15px;
		font-weight: 700;
		color: var(--color-lab-purple);
	}

	.reason {
		font-size: 11px;
		color: var(--color-lab-text-secondary);
	}

	.gamble {
		margin-top: 2px;
		font-size: 11px;
		color: var(--color-lab-text-muted);
	}

	.gamble-tag {
		color: var(--color-lab-yellow);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.risk {
		color: var(--color-lab-yellow);
	}

	/* Same yellow the unresolved corridor and the unread plate use. One colour
	   for "this is not settled" across the whole board. */
	.warn {
		font-size: 11px;
		line-height: 1.3;
		color: var(--color-lab-yellow);
	}
</style>
