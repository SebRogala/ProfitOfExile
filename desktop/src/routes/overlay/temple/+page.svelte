<script lang="ts">
	/**
	 * Temple builder overlay — a waiting notice, two offer boxes and a room
	 * widget (POE-244, reworked in POE-248, POE-249).
	 *
	 * The window is the whole game monitor and click-through everywhere; what
	 * the player sees is three widgets inside it, and since POE-244 they are of
	 * the two different kinds the engine supports. Each one answers the question
	 * the player has at a different point of one incursion cycle, which is why
	 * they are three surfaces and not three lines in one box.
	 *
	 * - `temple.waiting` is the WAITING NOTICE (POE-249): one line saying the
	 *   module heard Alva's start phrase and the layout sheet is not up yet. It
	 *   is the only surface that exists BEFORE there is anything read, and it is
	 *   gone the moment there is a board (`overlayShowsWaiting`). USER-PLACED,
	 *   with a shipped position measured clear of the board's read regions — see
	 *   `widgetDefaults` below.
	 * - `temple.offers` is the pair of OFFER BOXES (POE-249): one box per
	 *   architect block, stacked in the sheet's LEFT MARGIN to mirror the side
	 *   panel's own block order, each carrying what that kill builds, Vertolka's
	 *   rating for the line it builds into and the advisor's first reason for
	 *   that block — with the advisor's pick framed in cyan and the other box
	 *   faint. The frame IS the pointer (owner: no arrows anywhere). ANCHORED —
	 *   the module places it, because a column that mirrors the panel has to be
	 *   wherever the game drew those blocks.
	 *
	 *   It REPLACES `temple.advice`, the single kill callout, on the owner's
	 *   ask: the callout named the block the advisor chose and said one reason
	 *   about it, and the block the player is choosing AGAINST was not on the
	 *   overlay at all — so the comparison happened in the game's own panel, in
	 *   text, which is the reading this overlay exists to spare. The callout's
	 *   forced note and reason survive inside a box, and its panel-fallback
	 *   anchor as the first box's fallback top; what was DROPPED is its
	 *   `KILL … →` title and that title's `kill either` fallback — the
	 *   architect and the kind lead each box's own headline instead.
	 * - `temple.door` is the ROOM WIDGET: the same isometric rectangle the panel
	 *   draws next to the room name, with the OPEN doors green, the advisor's
	 *   door bigger and purple, and the kill marked as a cyan glyph on the
	 *   chosen architect's own icon spot inside the room. USER-PLACED and
	 *   persisted, and it stays up for as long as there is a move to make
	 *   (`overlayShowsDoors`, POE-248) — through the whole incursion and past
	 *   the capture standing down, which is when the panel and its own diamond
	 *   are long gone.
	 *
	 * `temple.board` — the lattice redrawn over the game — is gone. The board is
	 * already on screen behind this window, and the copy cost space that has to
	 * be kept clear of the module's own OCR crops. `TempleLattice.svelte`
	 * remains on the Temple page.
	 *
	 * # What this overlay deliberately no longer shows
	 *
	 * All of it is on the Temple page, which is the surface for READING; this one
	 * is for seeing. Retired here in POE-244:
	 *
	 * - the reader's status lines (`reading…`, `between rooms — layout only`) —
	 *   an overlay that says it is thinking is an overlay in the way;
	 * - the top GAMBLE and its risk %, which is a second option to weigh, and
	 *   weighing is what the page is for;
	 * - the unread-plate badge and the marker-fallback notice in full;
	 * - the advisor's own `warnings` list.
	 *
	 * The one honesty surface that did NOT move is `doorWarning()`, on the door
	 * widget: it says do not act on the shape above it, and that widget is the
	 * one still on screen while the player is acting. The `leaveMap` banner
	 * stays for the same reason — it is a decision about the map, not a reading.
	 *
	 * Retired again in POE-248, after the first live session: the kill callout's
	 * ARROW (owner: no arrows anywhere), the room widget's two text lines
	 * (`KILL <architect> → <room>` and `open <edge>`), and the red and grey
	 * seals. What is left on the widget is the outline, the open doors, the
	 * advisor's door and one cyan mark.
	 *
	 * # The rule that outranks everything else here
	 *
	 * **Nothing may cover a read region.** The module OCRs the side panel, the
	 * budget line, the panel's diamond, both boxes on every plate, and the beam
	 * patch at every corridor midpoint — 42 rectangles, published as
	 * `layout.rois` (POE-244) — and it reads them again on the next tick. A
	 * panel drawn over one is input the app wrote itself. `overlay-geometry.ts`
	 * places every surface against that set, and refuses rather than compromises:
	 * when nothing is free, nothing is drawn.
	 *
	 * `onMount` is not reliable in an overlay window and cross-window JS state is
	 * not either, so the board comes from the Rust-backed `get_ssot` poll that
	 * `routes/overlay/+layout.svelte` starts for every overlay route — this file
	 * only reads the rune.
	 *
	 * **Two gates, in two places, deliberately.** Whether the window is on screen
	 * at all is Rust's: the focus poller shows and hides the `temple` window with
	 * the game. What is left here is the one thing Rust cannot answer — whether
	 * there is anything worth drawing — and that is `overlayShowsBoard` /
	 * `overlayShowsDoors` / `overlayShowsWaiting`. Those gates live INSIDE the
	 * snippets and not around `WidgetHost`: a host that is not mounted has no
	 * `widget-config` listener, so a window flipped into config mode while there
	 * is no board would be genuinely interactive with no Save and no Cancel on
	 * it.
	 */
	import { invoke } from '@tauri-apps/api/core';
	import TempleDoorDiamond from '$lib/temple/TempleDoorDiamond.svelte';
	import TempleOfferBoxes from '$lib/temple/TempleOfferBoxes.svelte';
	import TempleWaitingNotice from '$lib/temple/TempleWaitingNotice.svelte';
	import WidgetHost from '$lib/overlay/widgets/WidgetHost.svelte';
	import { TEMPLE_WINDOW_LABEL } from '$lib/overlay/manager';
	import type { OverlayDefaultGeometry } from '$lib/overlay/overlay-defaults';
	import type { HostFrame, WidgetRect } from '$lib/overlay/widgets/widget-geometry';
	import type { WidgetSpec } from '$lib/overlay/widgets/widget-registry';
	import {
		bannerPlacement,
		captureToCss,
		doorDefaultPlacement,
		neverCoverRects,
		roiRect,
		waitingDefaultPlacement
	} from '$lib/temple/overlay-geometry';
	import {
		chosenOffer,
		doorWarning,
		leaveMapBanner,
		offerBoxes,
		overlayShowsBoard,
		overlayShowsDoors,
		overlayShowsWaiting,
		secondDoor,
		suggestedDoors
	} from '$lib/temple/view';
	import { ssot } from '$lib/stores/ssot.svelte';

	const temple = $derived(ssot.temple);
	/** The offer boxes' gate: there is a panel on screen. Whether there is
	 *  anything to SAY about it is `offerBoxes`, which is empty without advice —
	 *  so the leave-the-map banner inside this same snippet survives a read that
	 *  produced a map verdict and no ranking, which is what it is gated on. */
	const offersVisible = $derived(overlayShowsBoard(temple.status));
	/** The room widget's gate, and deliberately not a status one (POE-248):
	 *  there is a move to make and a room to draw it on. The offer boxes live
	 *  with the PANEL, this lives with the INCURSION. */
	const doorVisible = $derived(overlayShowsDoors(temple));
	/** The notice's gate (POE-249): Rust heard a start phrase and there is no
	 *  board on screen yet. Both halves are `view.ts`'s — Alva can speak over an
	 *  open sheet, and a notice that blinks over a board the player is reading
	 *  is the failure POE-246 fixed one layer down. */
	const waitingVisible = $derived(overlayShowsWaiting(temple));
	/** One box per architect block the panel printed, in the panel's own order.
	 *  Empty without advice or without a panel, which is what takes the boxes
	 *  off screen between the reads of a new cycle. */
	const boxes = $derived(offerBoxes(temple));
	/** The architect block the ranking chose, and every block this read parsed.
	 *  The room widget needs both: the chosen block's own OCR rect is what says
	 *  which half of the room the game drew its icon in, and one rect with no
	 *  siblings orders nothing. */
	const chosen = $derived(chosenOffer(temple));
	const offers = $derived(temple.panel?.offers ?? []);
	const suggested = $derived(suggestedDoors(temple.advice));
	/** The conditional door — what a SECOND Stone of Passage would buy. Rust's
	 *  answer, drawn faint and kept apart from `suggested`: that list is what to
	 *  open with the key in hand. */
	const secondary = $derived(secondDoor(temple.advice));
	const leaveBanner = $derived(leaveMapBanner(temple.advice));

	/** The banner's measured box, CSS px. Zero until the first frame — the same
	 *  measure-then-place trick the offer boxes and the config bar use, and for the
	 *  same reason: its width is what decides whether it clears the panel. */
	let bannerWidth = $state(0);
	let bannerHeight = $state(0);

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
	 * It sits in the DOOR widget since POE-244. The offer boxes are anchored,
	 * which means they are not drawn in config mode and are not placed by the
	 * user — the probe belongs on the surface that behaves like every other
	 * widget.
	 *
	 * That moved its precondition with it. The probe now needs a published
	 * `layout.diamond` — a read that settled a CURRENT ROOM — where before it
	 * only needed `overlayShowsBoard`. A board read between rooms
	 * (`no_current_room`) draws no diamond and therefore no probe, so the smoke
	 * check has to be run standing IN a room.
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

	/** Each offer's own block in CSS px, in the same order as the boxes — null
	 *  for a read that carried no rect for it. A missing rect is NOT the screen
	 *  origin: `offerStackPlacement` stacks that box under the one above it, or
	 *  places it at the panel crop's top when it is the first. */
	function blockRects(scaleFactor: number): (WidgetRect | null)[] {
		return boxes.map((box) => {
			const rect = box.offer.rect;
			return rect ? captureToCss(rect, scaleFactor) : null;
		});
	}

	/**
	 * Where the door diamond ships, for a user who has never placed it.
	 *
	 * Beside the game's own diamond and below the panel, which is where the eye
	 * already is — and clear of every read region, which a fixed number in the
	 * registry cannot promise on a screen it has never seen. Null falls back to
	 * that fixed number, which is the right answer when there is no board to
	 * anchor to.
	 *
	 * A widget the user HAS placed never reaches this: `placementFor` consults a
	 * default only when there is no stored row. The consequence for one that has
	 * not been placed is that it follows the panel as the panel moves, which is
	 * the same thing the offer boxes do and is what "shipped next to the game's
	 * diamond" has to mean.
	 */
	function doorDefaults(spec: WidgetSpec, frame: HostFrame): OverlayDefaultGeometry | null {
		if (spec.id !== 'temple.door') return null;
		const obstacles = neverCoverRects(temple.layout, frame.scaleFactor);
		// Empty means the layout is absent or the scale factor has not resolved.
		// Neither is "the whole screen is free", so nothing is offered.
		if (obstacles.length === 0) return null;
		const placed = doorDefaultPlacement({
			panel: roiRect(temple.layout, 'panel', frame.scaleFactor),
			diamond: roiRect(temple.layout, 'diamond', frame.scaleFactor),
			box: { w: spec.defaults.w, h: spec.defaults.h },
			obstacles,
			host: frame.host
		});
		return placed === null ? null : { ...spec.defaults, x: placed.x, y: placed.y };
	}

	/**
	 * Where the waiting notice ships, for a user who has never placed it
	 * (POE-249).
	 *
	 * Top-centre of the window and clear of every read region, which is the same
	 * bargain the door's default makes: the registry's fixed rectangle cannot
	 * know where this screen's panel crop is, so when there IS a board the
	 * module offers a position measured against it, and when there is not the
	 * registry number applies.
	 *
	 * Null is the ordinary answer here rather than the edge case, and it is the
	 * DESIGNED cold start. This notice's whole moment is before the first read
	 * of a cycle: Alva speaks, the sheet is shut, and on a fresh session there
	 * is no `temple.layout` at all — so there is nothing to place against and
	 * the shipped rectangle is what draws. That is ADR-019's carve-out, not a
	 * hole in it: a fixed rectangle the user can see and move is not a placer's
	 * silent output. Once a board HAS been read this session the host remembers
	 * the last answer (`lastDefaults`), so the notice keeps a position vetted
	 * against that screen's crops.
	 */
	function waitingDefaults(spec: WidgetSpec, frame: HostFrame): OverlayDefaultGeometry | null {
		if (spec.id !== 'temple.waiting') return null;
		const obstacles = neverCoverRects(temple.layout, frame.scaleFactor);
		// The same rule as the door's, for the same reason: empty is "the layout
		// is absent or the scale factor has not resolved", never "the screen is
		// free".
		if (obstacles.length === 0) return null;
		const placed = waitingDefaultPlacement({
			box: { w: spec.defaults.w, h: spec.defaults.h },
			obstacles,
			host: frame.host
		});
		return placed === null ? null : { ...spec.defaults, x: placed.x, y: placed.y };
	}

	/**
	 * The one `defaultsFor` hook the host calls, per widget.
	 *
	 * A dispatcher rather than one function with two branches inside it: each
	 * widget's shipped position is its own argument — the door wants the game's
	 * diamond, the notice wants the top centre — and each states the empty-set
	 * rule itself, which is what keeps that rule a property of the function a
	 * future third widget would be written beside. The anchored offer boxes are
	 * never asked (`defaultsFor` runs over placed widgets), and an id nothing here
	 * names answers null, which is "use the registry's number".
	 */
	function widgetDefaults(spec: WidgetSpec, frame: HostFrame): OverlayDefaultGeometry | null {
		return doorDefaults(spec, frame) ?? waitingDefaults(spec, frame);
	}
</script>

<WidgetHost
	module={TEMPLE_WINDOW_LABEL}
	defaultsFor={widgetDefaults}
	onAction={(action) => handleAction(action)}
>
	{#snippet content(spec, configMode)}
		{#if spec.id === 'temple.door' && doorVisible && temple.layout?.diamond}
			<TempleDoorDiamond
				diamond={temple.layout.diamond}
				layout={temple.layout}
				{suggested}
				{secondary}
				room={temple.panel?.room ?? null}
				offer={chosen}
				{offers}
				warning={doorWarning(temple.layout)}
			/>
			{#if debugProbe}
				<button class="probe" data-hot data-action="hot-probe">hot-rect probe</button>
			{/if}
		{:else if spec.id === 'temple.waiting' && waitingVisible}
			<!-- The one surface that draws before anything has been read
			     (POE-249). It carries no placement logic of its own: it is a
			     PLACEABLE widget, so the host puts it where the user did, or
			     where `waitingDefaults` offered, or where the registry ships
			     it. -->
			<TempleWaitingNotice />
		{:else if configMode}
			<!-- Nothing to draw. In config mode an unlabelled empty frame is
			     indistinguishable from a widget that is broken, so the frame
			     still says which widget it is. -->
			<p class="placeholder">{spec.label}</p>
		{/if}
	{/snippet}

	{#snippet anchored(spec, frame)}
		{#if spec.id === 'temple.offers' && offersVisible}
			{@const obstacles = neverCoverRects(temple.layout, frame.scaleFactor)}
			{#if leaveBanner}
				{@const at = bannerPlacement({
					box: { w: bannerWidth, h: bannerHeight },
					obstacles,
					host: frame.host
				})}
				<!-- As prominent as the kill, and gated only on there being a
				     panel on screen — NOT on there being a ranked move. R5's
				     verdict is about the MAP, so it has to survive a read that
				     produced a `mapAction` and no recommendation to point at.
				     It wants the top centre, but it goes through the SAME
				     avoidance as everything else: on the committed 1920x1080
				     frame a centred banner reaches x 1200 and the side panel's
				     OCR crop starts at 1131, so "centred and pinned" was over a
				     read region on the one screen size this repository has. -->
				<p
					class="leave"
					style="left:{at?.x ?? 0}px;top:{at?.y ?? 0}px;{at ? '' : 'visibility:hidden;'}"
					bind:offsetWidth={bannerWidth}
					bind:offsetHeight={bannerHeight}
				>
					{leaveBanner}
				</p>
			{/if}
			<TempleOfferBoxes
				{boxes}
				{obstacles}
				blocks={blockRects(frame.scaleFactor)}
				panel={roiRect(temple.layout, 'panel', frame.scaleFactor)}
				host={frame.host}
				maxWidth={spec.defaults.w}
			/>
		{/if}
	{/snippet}
</WidgetHost>

<style>
	/* Config mode with no room read: the frame still has to say which widget it
	   is, or the user is dragging an empty rectangle. */
	.placeholder {
		padding: 4px 8px;
		font-size: 11px;
		color: var(--color-lab-text-muted);
	}

	/* `left`/`top` come from `bannerPlacement` in the markup — no `transform`
	   to unwind, so the helper's numbers are the whole answer and the measured
	   box is the box that was placed. */
	.leave {
		position: absolute;
		padding: 4px 12px;
		font-size: 15px;
		font-weight: 700;
		color: var(--color-lab-bg);
		background: var(--color-lab-yellow);
		border-radius: 3px;
		white-space: nowrap;
	}

	/* The hot-rect probe (the guide's smoke item 4). Deliberately plain and
	   deliberately small: what is being checked is that the mouse hook consumes
	   a click inside this box and passes one just outside it to the game. */
	.probe {
		margin-top: 4px;
		padding: 2px 8px;
		font-size: 11px;
		color: var(--color-lab-bg);
		background: var(--color-lab-green);
		border: none;
		border-radius: 3px;
	}
</style>
