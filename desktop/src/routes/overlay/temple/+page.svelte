<script lang="ts">
	/**
	 * Temple builder overlay (POE-171, rebuilt on the widget engine in POE-225).
	 *
	 * The window is now the whole primary monitor and click-through everywhere;
	 * what the player sees is two independently placeable WIDGETS inside it —
	 * `temple.board` (the lattice) and `temple.advice` (the recommendation and
	 * everything the read could not settle). `WidgetHost.svelte` owns the
	 * placement, config mode, hot rects and click routing, and
	 * `docs/OVERLAY-GUIDE.md`'s six guards are answered there; this file owns
	 * only what each widget draws, which is the same content the single 620×260
	 * panel drew before the split (D10 — no new content in this batch).
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
	 * That gate lives INSIDE the widget snippet and not around `WidgetHost`. A
	 * host that is not mounted has no `widget-config` listener, so a window
	 * flipped into config mode while there is no board would be genuinely
	 * interactive with no Save and no Cancel on it — a rectangle over the game
	 * eating every click with no way out. Mounted always, drawing nothing when
	 * there is nothing: the widget boxes are empty and claim no clicks, and in
	 * config mode each frame carries its own name so it can still be placed.
	 *
	 * **What it draws when the read is imperfect is the point.** The unread
	 * plates, the fallback door read, a low-confidence panel and the advisor's
	 * own warnings all reach the overlay, because the surface the player is
	 * actually looking at while deciding is this one, and a recommendation that
	 * hides what it is uncertain about is the failure mode POE-171 is written
	 * against.
	 */
	import { invoke } from '@tauri-apps/api/core';
	import TempleLattice from '$lib/temple/TempleLattice.svelte';
	import WidgetHost from '$lib/overlay/widgets/WidgetHost.svelte';
	import { TEMPLE_WINDOW_LABEL } from '$lib/overlay/manager';
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

	/**
	 * Whether to draw the hot-rect probe button.
	 *
	 * A QUERY PARAM and not the app's debug flag, because there is no debug
	 * signal in an overlay window: `debugMode` is main-window `$state`,
	 * `AppState.debug_mode` is not projected into `AppStatus` or into the SSOT
	 * snapshot, and adding it to the snapshot is another work item's file. The
	 * owning layout appends `?debug` when it builds this window while debug mode
	 * is already on, so the smoke check is: turn debug mode on, toggle the temple
	 * module off and on, and the button is there.
	 *
	 * `window.location.search` rather than SvelteKit's `page` store: overlay
	 * routes are rendered by a static adapter into a window with no navigation.
	 */
	const debugProbe =
		typeof window !== 'undefined' && new URLSearchParams(window.location.search).has('debug');

	/**
	 * What a forwarded click does.
	 *
	 * The whole point of the probe is that a click on it must reach US and a
	 * click one pixel outside it must reach the GAME, and the only evidence of
	 * the first half that a shipped build can produce is a log line.
	 */
	function handleAction(action: string): void {
		if (action !== 'hot-probe') return;
		invoke('app_log_from_frontend', {
			msg: '[temple-widgets] hot-rect probe clicked — the mouse hook forwarded a click'
		}).catch((e) => console.error('[widgets] probe log unreachable:', e));
	}
</script>

<WidgetHost module={TEMPLE_WINDOW_LABEL} onAction={(action) => handleAction(action)}>
	{#snippet content(spec, configMode)}
		{#if !visible}
			<!-- No board worth drawing. Nothing on screen — unless the user is
			     placing widgets, in which case an unlabelled empty frame is
			     indistinguishable from a widget that is broken. -->
			{#if configMode}
				<p class="placeholder">{spec.label}</p>
			{/if}
		{:else if spec.id === 'temple.board'}
			<div class="board panel">
				<TempleLattice layout={temple.layout} highlightDoors={recommendedDoors} compact />
			</div>
		{:else if spec.id === 'temple.advice'}
			<div class="advice panel">
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

				{#if debugProbe}
					<button class="probe" data-hot data-action="hot-probe">hot-rect probe</button>
				{/if}
			</div>
		{/if}
	{/snippet}
</WidgetHost>

<style>
	/* Each widget carries the panel chrome the single container used to. */
	.panel {
		padding: 8px 10px;
		background: rgb(15 17 23 / 82%);
		border: 1px solid var(--color-lab-border);
		border-radius: 6px;
		color: var(--color-lab-text);
		font-size: 13px;
	}

	/* The widget box decides the width, not this: the host applies the
	   registry's shipped width as a max-width until the user resizes the widget,
	   and a fixed 200 here would ignore a resize in config mode. The lattice SVG
	   is `width: 100%`, so the column has to be definite for it to have anything
	   to fill. */
	.board {
		width: 100%;
	}

	.advice {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
		/* The widget box is content-sized inside a host the size of the monitor,
		   so without a bound a one-line headline reads as `max-content` and runs
		   the full width of the screen. The bound is the host's max-width. */
		max-width: 100%;
	}

	/* Config mode with no board: the frame still has to say which widget it is,
	   or the user is dragging two identical empty rectangles. */
	.placeholder {
		padding: 4px 8px;
		font-size: 11px;
		color: var(--color-lab-text-muted);
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

	/* The hot-rect probe (smoke item 4). Deliberately plain and deliberately
	   small: what is being checked is that the mouse hook consumes a click
	   inside this box and passes one just outside it to the game. */
	.probe {
		align-self: flex-start;
		margin-top: 4px;
		padding: 2px 8px;
		font-size: 11px;
		color: var(--color-lab-bg);
		background: var(--color-lab-green);
		border: none;
		border-radius: 3px;
	}
</style>
