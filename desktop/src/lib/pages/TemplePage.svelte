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
	import TempleValueTable from '$lib/temple/TempleValueTable.svelte';
	import {
		TEMPLE_STATUS_LABEL,
		TEMPLE_STATUS_TONE,
		convenienceNote,
		forcedKillNote,
		formatRisk,
		gambleLabel,
		incursionsText,
		lastReadText,
		leaveMapBanner,
		markerFallbackNotice,
		marketNote,
		modeLabel,
		offerBuilds,
		offerHeadline,
		secondDoor,
		topRecommendation,
		unknownRoomsBadge
	} from '$lib/temple/view';
	import {
		KNOBS,
		PRESET_NOTE,
		PRESET_OPTIONS,
		VALUE_MARK_LEGEND,
		copyValuesInto,
		overrideCount,
		parseKnob,
		parsePreset,
		valueRows,
		valueTableKey,
		withCell,
		withoutOverrides,
		type KnobSpec
	} from '$lib/temple/values';
	import type {
		TempleConfig,
		TempleCustom,
		TemplePreset,
		TempleProfile,
		TempleValueRow
	} from '$lib/temple/slice';
	import {
		rearmTemple,
		setTempleConfig,
		setTempleCustom,
		setTemplePreset,
		setTempleProfile,
		ssot,
		templeDebugCapture,
		templeValueTable
	} from '$lib/stores/ssot.svelte';

	const temple = $derived(ssot.temple);
	const layout = $derived(temple.layout);
	const panel = $derived(temple.panel);
	const advice = $derived(temple.advice);

	/** The doors the top recommendation says to open — drawn on the board. */
	const recommendedDoors = $derived(topRecommendation(advice)?.doors ?? []);
	/** The corridor a SECOND Stone of Passage would buy, or null. Printed under
	 *  the top recommendation only — it is one fact about the head of the list
	 *  and not a ranked move — and deliberately NOT drawn on the board, which
	 *  shows what to open with the key in hand. It is what the overlay's faint
	 *  purple seal says, in words, on the surface that has room for them. */
	const secondStone = $derived(secondDoor(advice));
	/** The convenience door in words, or null — the corridor to open with the
	 *  key when the move opens nothing, and the walk it shortens. Printed under
	 *  the top recommendation like the second stone's door, and for the same
	 *  reason: it is one fact about the head of the list, and it is what the
	 *  overlay's faint seal means when there is no primary door. */
	const convenience = $derived(convenienceNote(advice));

	/** What the prices behind every chaos value on this page are — one line,
	 *  always present (POE-258). It is re-derived on every SSOT poll rather
	 *  than composed in Rust, so the age it prints follows the clock instead of
	 *  standing still between the market poller's five-minute ticks. */
	const market = $derived(marketNote(temple.market));

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
	/**
	 * Optional path of a saved capture (a dump's `screen.png`) to read INSTEAD
	 * of the screen. The Rust command has always accepted one; the page did
	 * not expose it. It exists so a board that is long gone can still be run
	 * through the whole read path on a real Windows OCR engine — the
	 * 2026-09-03 laptop dump is the regression board for POE-230/234/243.
	 */
	let debugImagePath = $state('');
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
		const path = debugImagePath.trim();
		const { report, error } = await templeDebugCapture(path === '' ? null : path);
		debugFailed = error !== null;
		debugReport = error ?? JSON.stringify(report, null, 2);
		debugBusy = false;
	}

	// --- settings controls ----------------------------------------------------

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

	// --- the preset and its table (POE-259) -----------------------------------

	/** Long enough that tabbing across a row of cells is one write, short
	 *  enough that a pause reads as done. `$lib/prefs.svelte.ts`'s number, for
	 *  its reason: every write rewrites the whole settings file, and each one
	 *  also re-arms the reader.
	 *
	 *  Same accepted trade-off as that file's: an edit made within the window
	 *  of closing the app is lost. It degrades to the previous value and never
	 *  corrupts the table — and unlike a pref, the cell is still on screen
	 *  showing what was typed until the write lands. */
	const CUSTOM_WRITE_DEBOUNCE_MS = 300;

	/** The 25 x 3 table Rust priced for the preset in force. Fetched rather
	 *  than published: it is 75 values nothing but this editor reads, and the
	 *  SSOT snapshot goes out every three seconds. */
	let valueTable = $state<TempleValueRow[]>([]);
	/** What `valueTable` was fetched for — `valueTableKey`'s answer at the time,
	 *  which is the whole of what the numbers depend on. */
	let fetchedValueKey = $state<string | null>(null);
	let valueTableError = $state<string | null>(null);

	/**
	 * Cell and rate edits that Rust has not accepted yet, or has refused.
	 *
	 * The controls read this in preference to the slice, so a burst of edits
	 * inside one debounce window builds on each other rather than each
	 * starting from the last echo. Cleared when a write is accepted — from
	 * there the slice is the truth again — and KEPT when one is refused, so
	 * the player can see and fix what they typed.
	 */
	let customDraft = $state<TempleCustom | null>(null);
	let customTimer: ReturnType<typeof setTimeout> | null = null;
	/** Whether a `temple_set_custom` is out. Not `$state`: nothing renders it,
	 *  it only orders the writes. */
	let customInFlight = false;
	/** What an ACCEPTED write still had to say. Rust's, not re-derived here. */
	let customNotes = $state<string[]>([]);
	/** Why each refused rate was refused, by field. */
	let knobRefused = $state<Record<string, string>>({});

	const customTable = $derived(customDraft ?? temple.custom);
	/** The editor's rows. Under Default there is no override to report, so the
	 *  cells are read-only text and the table is passed no Custom at all. */
	const valueTableRows = $derived(
		valueRows(valueTable, temple.preset === 'custom' ? customTable : null)
	);
	const wantValueKey = $derived(valueTableKey(temple.preset, temple.market, temple.custom));

	// The slice is whole-replaced on every poll, so an effect that simply read
	// `temple.preset` would re-fetch 75 values every three seconds. The key is
	// what makes this fire on a CHANGE rather than on a poll.
	$effect(() => {
		const key = wantValueKey;
		const preset = temple.preset;
		if (key === fetchedValueKey) return;
		fetchedValueKey = key;
		void loadValueTable(preset, key);
	});

	async function loadValueTable(preset: TemplePreset, key: string): Promise<void> {
		const { rows, error } = await templeValueTable(preset);
		// A newer request was issued while this one was in flight; its answer
		// is the one the page is waiting for.
		if (key !== fetchedValueKey) return;
		valueTable = rows;
		valueTableError = error;
	}

	function pickPreset(raw: string): void {
		const preset = parsePreset(raw);
		if (preset === null || preset === temple.preset) return;
		// The preset ONLY. Writing the Custom table here would delete the
		// player's numbers the first time they looked at Default, which is the
		// one thing the two-field split exists to prevent.
		void apply(() => setTemplePreset(preset));
	}

	/** Queue a Custom write, coalescing a burst of edits into one command. */
	function writeCustom(next: TempleCustom): void {
		customDraft = next;
		armCustomWrite();
	}

	function armCustomWrite(): void {
		if (customTimer !== null) clearTimeout(customTimer);
		customTimer = setTimeout(() => {
			customTimer = null;
			void flushCustom();
		}, CUSTOM_WRITE_DEBOUNCE_MS);
	}

	async function flushCustom(): Promise<void> {
		const sent = customDraft;
		if (sent === null) return;
		// One write at a time. The command takes the WHOLE table, so two in
		// flight at once can be answered in either order and leave Rust — and
		// `settings.json` — holding the older of the two. Re-arming rather
		// than dropping is what keeps the newest draft: it is still in
		// `customDraft`, and something has to carry it once this write is done.
		if (customInFlight) {
			armCustomWrite();
			return;
		}
		customInFlight = true;
		try {
			const { error, notes } = await setTempleCustom(sent);
			settingsError = error;
			customNotes = notes;
			if (error === null && customDraft === sent) customDraft = null;
		} finally {
			customInFlight = false;
		}
	}

	/** One override cell. `null` puts that tier back on the formula. */
	function editCell(key: string, tier: number, chaos: number | null): void {
		writeCustom(withCell(customTable, key, tier, chaos));
	}

	function editKnob(spec: KnobSpec, raw: string): void {
		const parsed = parseKnob(raw, spec);
		if (parsed.kind === 'invalid') {
			knobRefused = { ...knobRefused, [spec.field]: parsed.reason };
			return;
		}
		const { [spec.field]: _cleared, ...rest } = knobRefused;
		knobRefused = rest;
		writeCustom({ ...customTable, [spec.field]: parsed.chaos });
	}

	/** D3's one explicit click — the ONLY way Default's numbers enter Custom. */
	async function copyDefaultValues(): Promise<void> {
		const { rows, error } = await templeValueTable('default');
		if (error !== null) {
			settingsError = error;
			return;
		}
		writeCustom(copyValuesInto(customTable, rows));
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
			<span
				class="meta"
				title="Where the chaos values on this page come from. Prices are polled from the server every five minutes; a missing or stale read leaves every room at the preset's base value."
				>{market}</span
			>
			<span class="spacer"></span>
			<Button
				onclick={() => void apply(rearmTemple)}
				title="Force the next tick to re-read the board, whatever the read gate thinks. Use this when a plate came out wrong."
			>
				Re-read
			</Button>
			<input
				type="text"
				bind:value={debugImagePath}
				placeholder="saved screen.png (optional)"
				class="debug-path"
				title="Leave empty to capture the screen. Paste the full path of a dump's screen.png to run that capture through the read path instead."
			/>
			<Button
				onclick={runDebugCapture}
				disabled={debugBusy}
				title="Capture the screen now (or read the saved capture named on the left) and write a debug dump (screenshot, diamond crop, OCR regions, ocr-lines.json, report.json)."
			>
				{debugBusy ? 'Capturing…' : debugImagePath.trim() === '' ? 'Debug capture' : 'Debug read file'}
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
							<!-- Rust's conditional answer (POE-248): what a SECOND stone would
							     buy, given this move. Never folded into `doorsLabel` — with one
							     key in hand that would name a door the player cannot open. -->
							{#if i === 0 && secondStone}
								<span class="move-doors second">second stone: {secondStone}</span>
							{/if}
							<!-- The other thing the faint seal can mean: the move opens
							     nothing, and this is the door to spend the key on anyway,
							     for the walk. Rust's line names the door and the walk. -->
							{#if i === 0 && convenience}
								<span class="move-doors second">{convenience}</span>
							{/if}
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

	<!-- 5. Settings: the two map rules and the strategy profile. -->
	<section class="card settings-card">
		<h2 class="card-title">Settings</h2>
		{#if settingsError}
			<p class="error">{settingsError}</p>
		{/if}

		<!-- The room valuation first: it decides what every number on this page
		     means, and the two weights below are stated relative to it. -->
		<div class="setting">
			<span class="setting-label">Room valuation</span>
			<div class="preset-head">
				<SegmentedButtons
					value={temple.preset}
					options={PRESET_OPTIONS}
					onselect={pickPreset}
					title="Which valuation the advisor ranks on. Switching never copies Default's numbers into Custom and never clears Custom."
				/>
				<span class="meta">{market}</span>
			</div>
			<p class="meta">{PRESET_NOTE[temple.preset]}</p>

			{#if temple.preset === 'custom'}
				<div class="knobs">
					{#each KNOBS as knob (knob.field)}
						<label class="number">
							<span>{knob.label}</span>
							<input
								type="number"
								min="0"
								max={knob.max ?? undefined}
								step={knob.step}
								value={customTable[knob.field]}
								aria-invalid={knobRefused[knob.field] !== undefined}
								class:refused={knobRefused[knob.field] !== undefined}
								onchange={(e) => editKnob(knob, e.currentTarget.value)}
							/>
							<span class="meta">{knob.hint}</span>
							{#if knobRefused[knob.field]}
								<span class="cell-error">{knobRefused[knob.field]}</span>
							{/if}
						</label>
					{/each}
				</div>

				<div class="table-actions">
					<Button
						onclick={() => void copyDefaultValues()}
						title="Write Default's current number into every cell. The one way Default's values enter Custom — switching preset never copies."
					>
						Copy Default into Custom
					</Button>
					<Button
						variant="danger"
						onclick={() => writeCustom(withoutOverrides(customTable))}
						title="Drop every per-room number and put the whole board back on the formula. The rates are left alone."
					>
						Clear overrides
					</Button>
					<span class="meta">
						{overrideCount(customTable)} room-tier{overrideCount(customTable) === 1 ? '' : 's'} priced
						by hand
					</span>
				</div>

				{#each customNotes as note (note)}
					<p class="warn">{note}</p>
				{/each}
			{/if}

			{#if valueTableError}
				<p class="error">{valueTableError}</p>
			{/if}
			<TempleValueTable
				rows={valueTableRows}
				editable={temple.preset === 'custom'}
				onedit={editCell}
			/>
			<p class="meta">{VALUE_MARK_LEGEND}</p>
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
			<p class="meta">
				Both weights are RELATIVE, in units where the best tier-3 room on the board is worth 9 —
				not chaos. The app rescales them to whatever that room is worth today, so the same number
				keeps meaning the same thing when the market moves.
			</p>
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
				<span class="meta">
					What the Apex of Atzoatl is worth on its own, with the top tier-3 room at 9.
				</span>
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
					Traversal weight per corridor from the Entrance, same units — top tier-3 room = 9. 0 for
					the Doryani rush.
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
	.debug-path {
		min-width: 220px;
		padding: 4px 8px;
		font-size: 0.8rem;
		color: var(--color-lab-text);
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
	}

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

	/* Dimmer than the doors above it, the page's own reading of the overlay's
	   "faint is the alternative" rule: a conditional answer must not compete
	   with the move being recommended. */
	.move-doors.second {
		color: var(--color-lab-text-muted);
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

	/* The preset picker and the market line it applies to, on one row. */
	.preset-head {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.6rem;
	}

	/* The five rates, one per line so each keeps its unit hint beside it — the
	   hints are the whole reason a player can set these at all. */
	.knobs {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		width: 100%;
	}

	.table-actions {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	/* A refused rate is marked by its border AND by the reason printed beside
	   it — a colour alone is a mark somebody cannot see. */
	.number input.refused {
		border-color: var(--color-lab-red);
	}

	.cell-error {
		font-size: 0.7rem;
		color: var(--color-lab-red);
	}
</style>
