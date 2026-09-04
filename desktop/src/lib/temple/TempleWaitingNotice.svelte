<script lang="ts">
	/**
	 * "waiting for the temple panel" — the one surface the module has before it
	 * has read anything (POE-249).
	 *
	 * Alva's start line fires when the portal opens, which is up to a whole
	 * incursion before the layout sheet is on screen. Between the two the module
	 * is armed, grabbing the screen every 650 ms and finding no panel — and
	 * every other overlay surface it has is gated on a board, so the app looks
	 * switched off during the one stretch where the player is waiting on it. The
	 * lifecycle doc's row 1 is this line.
	 *
	 * # It says one thing and says it once
	 *
	 * No status, no tick count, no "reading…". A notice that reports the
	 * module's internal state is an overlay in the way (the POE-244 retirements
	 * say so about the reader's own status lines); what the player needs is that
	 * the thing is armed and there is nothing to show yet. `overlayShowsWaiting`
	 * takes it off screen the moment there is a board — the flag and a read are
	 * not mutually exclusive, because Alva can speak over an open sheet.
	 *
	 * # Deliberately no purple and no cyan
	 *
	 * Both are VERDICTS on this overlay: purple is the advisor's door and its
	 * callout frame, cyan is the kill. This is not a verdict — it is the absence
	 * of one — so it is drawn in the neutral text colour inside the same dark
	 * box the callout uses, and the border is the muted grey. A player glancing
	 * at a coloured box over the game reads the colour before the words.
	 *
	 * # No placement logic here
	 *
	 * `temple.waiting` is PLACEABLE: `WidgetHost` positions it, from the user's
	 * stored rectangle or from the default the route offers through
	 * `defaultsFor` (`waitingDefaultPlacement`). That is the difference between
	 * this component and `TempleKillCallout.svelte`, which is anchored and
	 * therefore has to measure and place itself.
	 */
</script>

<p class="waiting">waiting for the temple panel</p>

<style>
	/* The callout's box, in the neutral palette: same dark ground and radius, so
	   the two surfaces read as the same app, and a 1 px muted border where the
	   callout carries a 2 px purple one. */
	.waiting {
		padding: 6px 10px;
		font-size: 14px;
		color: var(--color-lab-text);
		background: rgb(15 17 23 / 88%);
		border: 1px solid var(--color-lab-text-muted);
		border-radius: 6px;
		white-space: nowrap;
		pointer-events: none;
	}
</style>
