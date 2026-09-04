<script lang="ts">
	/**
	 * Temple of Atzoatl builder (POE-171) — the module's full surface.
	 *
	 * Reads `ssot.temple` and its own settings echo, and NEVER `ssot.modules`
	 * (ADR-014): the module toggle lives in the Sidebar, and this page stays
	 * browsable with the module switched off, showing the last board it read and
	 * every control the user owns. The status badge says "module off" because
	 * the SSOT composer forces the status, not because the page went looking for
	 * the flag.
	 *
	 * It decides nothing. Every ranking, every reason, every warning and the
	 * leave-the-map verdict come from the Rust advisor; `$lib/temple/view` words
	 * them and this file lays them out.
	 */
	import Button from '$lib/components/Button.svelte';
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte';
	import TempleLattice from '$lib/temple/TempleLattice.svelte';
	import {
		TEMPLE_STATUS_LABEL,
		TEMPLE_STATUS_TONE,
		forcedKillNote,
		formatRisk,
		gambleLabel,
		incursionsText,
		lastReadText,
		leaveMapBanner,
		markerFallbackNotice,
		modeLabel,
		offerBuilds,
		offerHeadline,
		topRecommendation,
		unknownRoomsBadge
	} from '$lib/temple/view';
	import type { TempleConfig, TempleProfile } from '$lib/temple/slice';
	import {
		rearmTemple,
		setTempleConfig,
		setTempleKeys,
		setTempleProfile,
		ssot,
		templeDebugCapture
	} from '$lib/stores/ssot.svelte';

	const temple = $derived(ssot.temple);
	const layout = $derived(temple.layout);
	const panel = $derived(temple.panel);
	const advice = $derived(temple.advice);

	/** The doors the top recommendation says to open — drawn on the board. */
	const recommendedDoors = $derived(topRecommendation(advice)?.doors ?? []);

	const unknownBadge = $derived(unknownRoomsBadge(temple));
	const markerNotice = $derived(markerFallbackNotice(layout));
	const leaveBanner = $derived(leaveMapBanner(advice));
	/** Set when the read saw one of the panel's two architect blocks — every
	 *  ranked kill below is then that one architect's, forced rather than
	 *  chosen (POE-243). */
	const forcedNote = $derived(forcedKillNote(advice));

	// --- commands -------------------------------------------------------------

	/** The last rejection from any settings command, shown next to the controls. */
	let settingsError = $state<string | null>(null);
	let debugBusy = $state(false);
	let debugReport = $state<string | null>(null);
	let debugFailed = $state(false);

	/** Run one settings command and keep whatever it refused on screen. */
	async function apply(run: () => Promise<string | null>): Promise<void> {
		settingsError = await run();
	}

	async function runDebugCapture(): Promise<void> {
		debugBusy = true;
		debugReport = null;
		debugFailed = false;
		const { report, error } = await templeDebugCapture();
		debugFailed = error !== null;
		debugReport = error ?? JSON.stringify(report, null, 2);
		debugBusy = false;
	}

	// --- settings controls ----------------------------------------------------

	/** 0 is legal (every passage already open) and 2 is the game's maximum. */
	const KEY_OPTIONS = [
		{ value: '0', label: '0' },
		{ value: '1', label: '1' },
		{ value: '2', label: '2' }
	];

	/** Write one config flag, carrying the other one through unchanged. */
	function setConfigFlag(field: keyof TempleConfig, value: boolean): void {
		void apply(() => setTempleConfig({ ...temple.config, [field]: value }));
	}

	/** Write one profile field, carrying the other three through unchanged. */
	function setProfileField<K extends keyof TempleProfile>(
		field: K,
		value: TempleProfile[K]
	): void {
		void apply(() => setTempleProfile({ ...temple.profile, [field]: value }));
	}

	/**
	 * Read a number out of an input, refusing what the scorer cannot use.
	 *
	 * Rust rejects a negative or non-finite weight too, and that is the
	 * authority — this only keeps a half-typed "-" from firing a command per
	 * keystroke and filling the log with rejections.
	 */
	function numberFrom(raw: string): number | null {
		const parsed = Number(raw);
		return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
	}
</script>

<div class="temple-page">
	<div class="page-head">
		<h1>Temple of Atzoatl</h1>
		<span class="badge tone-{TEMPLE_STATUS_TONE[temple.status]}"
			>{TEMPLE_STATUS_LABEL[temple.status]}</span
		>
		{#if modeLabel(temple.mode)}
			<span class="badge tone-bonus" title="Which of the profile's two modes the advisor is in."
				>{modeLabel(temple.mode)}</span
			>
		{/if}
	</div>

	<p class="intro">
		Open the temple's layout panel in game. The module reads the board once per panel open and
		ranks the two decisions an incursion asks for — which architect to kill, and which passage to
		open. Every recommendation carries the rules that produced it.
	</p>

	<!-- 1. What the module is doing, and the two levers that fix it. -->
	<section class="card status-card">
		<div class="card-head">
			<h2 class="card-title">Reader</h2>
			<span class="meta">
				{#if lastReadText(temple.lastReadAt)}
					last read {lastReadText(temple.lastReadAt)}
				{:else}
					no board read yet
				{/if}
			</span>
			{#if temple.calibration}
				<span class="meta">
					anchor scale {temple.calibration.scale.toFixed(3)} at
					{temple.calibration.screen_w}×{temple.calibration.screen_h}
				</span>
			{/if}
			{#if layout}
				<span class="meta">panel NCC {layout.ncc.toFixed(3)} · {layout.confidence} confidence</span>
			{/if}
			<span class="spacer"></span>
			<Button
				onclick={() => void apply(rearmTemple)}
				title="Force the next tick to re-read the board, whatever the read gate thinks. Use this when a plate came out wrong."
			>
				Re-read
			</Button>
			<Button
				onclick={runDebugCapture}
				disabled={debugBusy}
				title="Capture the screen now and write a debug dump (screenshot, diamond crop, OCR regions, report.json)."
			>
				{debugBusy ? 'Capturing…' : 'Debug capture'}
			</Button>
		</div>

		{#if temple.lastError}
			<p class="error">Last error: {temple.lastError}</p>
		{/if}
		{#if temple.readNotice}
			<!-- A WARNING and not an error: the read completed and the board on
			     screen is real, it was just short a text region. Same tone as the
			     marker notice below, which is the same class of fact. -->
			<p class="warn">{temple.readNotice}</p>
		{/if}
		{#if unknownBadge}
			<p class="warn">{unknownBadge} — the advisor treats an unread plate as junk, not as empty.</p>
		{/if}
		{#if markerNotice}
			<p class="warn">{markerNotice}</p>
		{/if}
		{#if layout && layout.confidence === 'low'}
			<p class="warn">
				The panel read at low confidence — the door sets are a best effort and nothing should be
				acted on.
			</p>
		{/if}
		{#if debugReport}
			<pre class="report" class:error={debugFailed}>{debugReport}</pre>
		{/if}
	</section>

	<!-- 2. The board. Drawn whether or not there is advice to go with it. -->
	<section class="card board-card">
		<div class="card-head">
			<h2 class="card-title">Board</h2>
			{#if layout?.current}
				<span class="meta">standing in {layout.current}</span>
			{:else}
				<span class="meta">between rooms — no position to rank from</span>
			{/if}
			<span class="legend">
				<span class="legend-item legend-open">— open</span>
				<span class="legend-item legend-uncertain">-- reported open, hidden by the frame</span>
				<span class="legend-item legend-unresolved">·· could not be read</span>
				<span class="legend-item legend-recommended">— recommended</span>
			</span>
		</div>
		{#if layout}
			<TempleLattice {layout} highlightDoors={recommendedDoors} />
		{:else}
			<p class="meta">
				Nothing read yet. Open the temple's layout panel with the module switched on.
			</p>
		{/if}
	</section>

	<!-- 3. The side panel, as text gave it. -->
	<section class="card panel-card">
		<div class="card-head">
			<h2 class="card-title">Side panel</h2>
			<span class="meta">{panel?.room ?? 'room title not read'}</span>
			<span class="meta">{incursionsText(panel?.incursionsRemaining ?? null)}</span>
		</div>
		{#if panel && panel.offers.length > 0}
			<ul class="offers">
				{#each panel.offers as offer (offer.index)}
					<li class="offer">
						<span class="offer-head">{offerHeadline(offer)}</span>
						<span class="offer-builds" class:unresolved={offer.displayName === null}>
							builds {offerBuilds(offer)}
						</span>
						<!-- The printed name is kept because it is what the player reads off
						     the panel — but it is never the answer on its own. -->
						<span class="meta">panel prints “{offer.printedTarget}”</span>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="meta">No architect block read.</p>
		{/if}
	</section>

	<!-- 4. The decision. -->
	<section class="card advice-card">
		<div class="card-head">
			<h2 class="card-title">Advice</h2>
			{#if temple.status === 'no_current_room'}
				<span class="meta">
					the reader has no current room, so nothing is ranked — the layout above still stands
				</span>
			{/if}
		</div>

		{#if leaveBanner}
			<!-- As prominent as the kill, by contract: R5 says the map is done. -->
			<p class="leave-banner">{leaveBanner}</p>
		{/if}

		{#if advice}
			{#each advice.warnings as warning (warning)}
				<p class="warn">{warning}</p>
			{/each}

			{#if advice.recommendations.length > 0}
				<ol class="ranked">
					{#each advice.recommendations as move, i (`${i}-${move.headline}-${move.doorsLabel}`)}
						<li class="move" class:top={i === 0}>
							<span class="move-headline">{move.headline}</span>
							<!-- The panel prints two architect blocks; when the read saw one,
							     this kill was forced rather than chosen (POE-243). Marked on
							     every rank, because the whole list is one architect's. -->
							{#if forcedNote}<span class="forced">({forcedNote})</span>{/if}
							<span class="move-doors">open {move.doorsLabel}</span>
							<span class="meta">score {move.ev.toFixed(2)}</span>
							<!-- The reasons ARE the audit trail: a bare score cannot be checked. -->
							<ul class="reasons">
								{#each move.reasons as reason (reason)}
									<li>{reason}</li>
								{/each}
							</ul>
						</li>
					{/each}
				</ol>
			{:else}
				<p class="meta">Nothing ranked.</p>
			{/if}

			{#if advice.gambles.length > 0}
				<h3 class="sub-title">Gambles</h3>
				<p class="meta">
					Excluded by the risk filter, not by score — each carries the fraction of rollouts that
					lost the room.
				</p>
				<ol class="ranked">
					{#each advice.gambles as gamble, i (`${i}-${gamble.headline}-${gamble.doorsLabel}`)}
						<li class="move gamble">
							<span class="badge tone-unknown">{gambleLabel(gamble)}</span>
							<span class="move-headline">{gamble.headline}</span>
							<span class="move-doors">open {gamble.doorsLabel}</span>
							<span class="meta">score {gamble.ev.toFixed(2)}</span>
							<span class="meta">risk {formatRisk(gamble.risk) ?? 'not measured'}</span>
							<ul class="reasons">
								{#each gamble.reasons as reason (reason)}
									<li>{reason}</li>
								{/each}
							</ul>
						</li>
					{/each}
				</ol>
			{/if}
		{:else}
			<p class="meta">No advice — there is no board, or no room to rank from.</p>
		{/if}
	</section>

	<!-- 5. Settings: the board fact the panel does not print, and the profile. -->
	<section class="card settings-card">
		<h2 class="card-title">Settings</h2>
		{#if settingsError}
			<p class="error">{settingsError}</p>
		{/if}

		<div class="setting">
			<span class="setting-label">Opening stones dropped</span>
			<SegmentedButtons
				value={String(temple.keys)}
				options={KEY_OPTIONS}
				onselect={(v) => void apply(() => setTempleKeys(Number(v)))}
				title="The panel does not print how many keys this incursion dropped, so you set it. 1 is the common case."
			/>
			<span class="meta">The panel does not print this; 1 is the common case.</span>
		</div>

		<div class="setting">
			<span class="setting-label">Map rules</span>
			<label class="check">
				<input
					type="checkbox"
					checked={temple.config.artefactsOfTheVaal}
					onchange={(e) => setConfigFlag('artefactsOfTheVaal', e.currentTarget.checked)}
				/>
				<span>Artefacts of the Vaal</span>
				<span class="meta">Atlas passive — four incursions per map instead of three.</span>
			</label>
			<label class="check">
				<input
					type="checkbox"
					checked={temple.config.scarabOfTimelines}
					onchange={(e) => setConfigFlag('scarabOfTimelines', e.currentTarget.checked)}
				/>
				<span>Incursion Scarab of Timelines</span>
				<span class="meta">
					Requires finishing every incursion, so "leave this map" is never advised.
				</span>
			</label>
		</div>

		<div class="setting">
			<span class="setting-label">Strategy profile</span>
			<label class="number">
				<span>Apex score</span>
				<input
					type="number"
					min="0"
					step="0.5"
					value={temple.profile.apexScore}
					onchange={(e) => {
						const v = numberFrom(e.currentTarget.value);
						if (v !== null) setProfileField('apexScore', v);
					}}
				/>
				<span class="meta">What the Apex of Atzoatl is worth on its own.</span>
			</label>
			<label class="number">
				<span>Path cost</span>
				<input
					type="number"
					min="0"
					step="0.25"
					value={temple.profile.pathCost}
					onchange={(e) => {
						const v = numberFrom(e.currentTarget.value);
						if (v !== null) setProfileField('pathCost', v);
					}}
				/>
				<span class="meta">
					Traversal weight per corridor from the Entrance. 0 for the Doryani rush.
				</span>
			</label>
			<label class="check">
				<input
					type="checkbox"
					checked={temple.profile.rerollUntilFavourable}
					onchange={(e) => setProfileField('rerollUntilFavourable', e.currentTarget.checked)}
				/>
				<span>Reroll until favourable <span class="badge tone-unknown">proposed</span></span>
				<span class="meta">
					Prefer a change over an upgrade while no favourable line exists. Marked proposed in the
					strategy notes — not yet confirmed in play.
				</span>
			</label>
			<label class="check">
				<input
					type="checkbox"
					checked={temple.profile.r4KeepUpgradeTargets}
					onchange={(e) => setProfileField('r4KeepUpgradeTargets', e.currentTarget.checked)}
				/>
				<span>R4 keep upgrade targets <span class="badge tone-unknown">experimental</span></span>
				<span class="meta">
					Keep a slot in the drop pool while an adjacent upgrade room can still hit it.
				</span>
			</label>
		</div>
	</section>
</div>

<style>
	.temple-page {
		max-width: 1400px;
		margin: 0 auto;
		color: var(--color-lab-text);
	}

	.page-head {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.6rem;
		margin-bottom: 0.5rem;
	}

	h1 {
		font-size: 1.2rem;
		color: var(--color-lab-text);
	}

	.intro {
		font-size: 0.8rem;
		color: var(--color-lab-text-secondary);
		margin-bottom: 1rem;
	}

	.card {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		border-radius: 6px;
		padding: 0.75rem;
		margin-bottom: 0.75rem;
	}

	.card-head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.card-title {
		font-size: 0.95rem;
		font-weight: 600;
		color: var(--color-lab-text);
	}

	.sub-title {
		font-size: 0.85rem;
		font-weight: 600;
		margin-top: 0.9rem;
	}

	.spacer {
		flex: 1;
	}

	.meta {
		font-size: 0.7rem;
		color: var(--color-lab-text-muted);
	}

	.error {
		font-size: 0.75rem;
		color: var(--color-lab-red);
		margin-top: 0.5rem;
	}

	.warn {
		font-size: 0.75rem;
		color: var(--color-lab-yellow);
		margin-top: 0.5rem;
	}

	.report {
		margin-top: 0.5rem;
		padding: 0.5rem;
		font-size: 0.7rem;
		color: var(--color-lab-text-secondary);
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		border-radius: 4px;
		max-height: 320px;
		overflow: auto;
		white-space: pre-wrap;
	}

	.report.error {
		color: var(--color-lab-red);
	}

	.badge {
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		border-radius: 3px;
		border: 1px solid currentcolor;
		padding: 0 4px;
		white-space: nowrap;
	}

	.tone-pass {
		color: var(--color-lab-green);
	}

	.tone-fail {
		color: var(--color-lab-red);
	}

	.tone-warn,
	.tone-unknown {
		color: var(--color-lab-yellow);
	}

	.tone-bonus {
		color: var(--color-lab-blue);
	}

	.tone-muted {
		color: var(--color-lab-text-muted);
	}

	.legend {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 0.7rem;
		font-size: 0.68rem;
		margin-left: auto;
	}

	.legend-open {
		color: var(--color-lab-green);
	}

	.legend-uncertain {
		color: var(--color-lab-green-muted);
	}

	.legend-unresolved {
		color: var(--color-lab-yellow);
	}

	.legend-recommended {
		color: var(--color-lab-purple);
	}

	.offers {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(300px, 100%), 1fr));
		gap: 0.6rem;
		margin-top: 0.6rem;
	}

	.offer {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		padding: 0.5rem;
		border: 1px solid var(--color-lab-border);
		border-radius: 4px;
	}

	.offer-head {
		font-size: 0.8rem;
		font-weight: 600;
	}

	.offer-builds {
		font-size: 0.8rem;
		color: var(--color-lab-green);
	}

	.offer-builds.unresolved {
		color: var(--color-lab-yellow);
	}

	/* R5's verdict, as loud as the kill it replaces. */
	.leave-banner {
		margin: 0.6rem 0;
		padding: 0.6rem 0.75rem;
		font-size: 0.95rem;
		font-weight: 600;
		color: var(--color-lab-bg);
		background: var(--color-lab-yellow);
		border-radius: 4px;
	}

	.ranked {
		list-style: none;
		margin-top: 0.6rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.move {
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		padding: 0.5rem;
		border: 1px solid var(--color-lab-border);
		border-radius: 4px;
	}

	.move.top {
		border-color: var(--color-lab-purple);
	}

	.move.gamble {
		border-style: dashed;
	}

	.move-headline {
		font-size: 0.85rem;
		font-weight: 600;
	}

	/* The "this was not a choice" mark, in the board's one unsettled colour —
	   the same yellow the unread plate and the unresolved corridor carry. */
	.forced {
		font-size: 0.72rem;
		color: var(--color-lab-yellow);
	}

	.move-doors {
		font-size: 0.78rem;
		color: var(--color-lab-text-secondary);
	}

	.reasons {
		list-style: none;
		margin-top: 0.25rem;
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		font-size: 0.72rem;
		color: var(--color-lab-text-secondary);
	}

	.setting {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.35rem;
		padding: 0.6rem 0;
		border-top: 1px solid var(--color-lab-border);
	}

	.setting-label {
		font-size: 0.8rem;
		font-weight: 600;
	}

	.check,
	.number {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 0.4rem;
		font-size: 0.78rem;
	}

	.number input {
		width: 6rem;
		background: var(--color-lab-bg);
		color: var(--color-lab-text);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		padding: 2px 5px;
	}
</style>
