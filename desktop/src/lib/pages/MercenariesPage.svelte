<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import {
		MERC_SOURCES,
		entryKind,
		kinetistLadder,
		type MercRuleset,
		type MercSourceId,
	} from '$lib/mercenaries/rulesets';
	import {
		columnLabel,
		kindTitle,
		ladderRows,
		quantifier,
		rungOutcomes,
		sharedValue,
		type LadderCell,
	} from '$lib/mercenaries/ladder-view';
	import {
		GROUP_OUTCOME_LABEL,
		GROUP_OUTCOME_TONE,
		HEADLINE_LABEL,
		HEADLINE_TONE,
		positionOutcomeLabel,
		POSITION_OUTCOME_TONE,
		READ_GLYPH,
		READ_STATE_LABEL,
		READ_TONE,
		RULESET_OUTCOME_LABEL,
		RULESET_OUTCOME_TONE,
		SCAN_NOW_LABEL,
		SCAN_NOW_TITLE,
		STATUS_LABEL,
		STATUS_TONE,
		captureOnScreen,
		capturedAt,
		describeDebugResult,
		groupKey,
		indexGroups,
		indexPositions,
		notInRulesNames,
		POOLED_CHIP_MARK,
		poolSyncView,
		positionKey,
		skillText,
		skillTitle,
		supportText,
		supportTitle,
		templateChip,
	} from '$lib/mercenaries/capture-view';
	import { enabledSources, withSourceEnabled } from '$lib/mercenaries/merc-prefs';
	import { evaluateCapture } from '$lib/mercenaries/verdict';
	import { savedSearchUrl } from '$lib/mercenaries/trade-links';
	import {
		MERC_TRADE_MAX_SEARCHES,
		tradeHeadline,
		tradeStatusLabel,
		tradeStatusTone
	} from '$lib/mercenaries/trade-view';
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte';
	import Button from '$lib/components/Button.svelte';
	import TradeListings from '$lib/components/TradeListings.svelte';
	import { mercListingRow } from '$lib/tradeApi';
	import {
		setMercSourcesOff,
		setMercTierFloor,
		setMercTradeAuto,
		ssot
	} from '$lib/stores/ssot.svelte';

	// Every rule shown here is read from `$lib/mercenaries/rulesets` — the page
	// renders the rulesets, it never re-derives them. In particular the glyph and
	// its colour come from `entryKind`, which resolves type-first (a switched-off
	// denial is still a denial); reproducing that rule in markup is how the page
	// would end up advertising the stats a search exists to reject.
	//
	// The verdict half is the same deal one layer up: `evaluateCapture` decides
	// every outcome and `capture-view` words it. The page reads `ssot.mercenary`
	// (Rust-owned) plus its own prefs, and NEVER `ssot.modules` — the module
	// toggle lives in the sidebar and this page stays browsable with it off
	// (ADR-014).

	/** One glyph per state. Text, not shapes: it survives a colour-blind reader,
	 *  and the wording behind it is on the row's `title`. */
	const GLYPH: Record<LadderCell, string> = {
		required: '✓',
		forbidden: '✕',
		bonus: '＋',
		absent: '·',
	};

	const SOURCE_OPTIONS = MERC_SOURCES.map((s) => ({ value: s.id, label: s.label }));

	let selectedSource = $state<MercSourceId>(MERC_SOURCES[0].id);

	const source = $derived(MERC_SOURCES.find((s) => s.id === selectedSource) ?? MERC_SOURCES[0]);

	// --- SSOT + prefs ---------------------------------------------------------

	const merc = $derived(ssot.mercenary);
	const capture = $derived(merc.capture);
	/** Display-only liveness: the status is what decides, the flag only agrees.
	 *  `done` counts as on screen — the OCR paused, the window did not close
	 *  (2026-08-25), and "window gone" over a window the player is looking at
	 *  is the same lie as the other way round. */
	const captureLive = $derived(captureOnScreen(merc.status) && capture?.live === true);

	/** Rust's echo, not a local pref: the verdict overlay reads the same field,
	 *  so the two windows cannot evaluate one capture against two guide sets
	 *  (POE-199). Written through `setMercSourcesOff`, never assigned here. */
	const enabled = $derived(enabledSources(merc.sourcesOff));

	/** The verdict is derived, never stored: it is a function of the capture, the
	 *  rulesets, the source toggles and the active league, and any copy of it
	 *  would be one poll away from lying. */
	const verdict = $derived(
		capture === null ? null : evaluateCapture(capture, MERC_SOURCES, enabled, ssot.league)
	);
	const sourceVerdict = $derived(verdict?.sources.find((s) => s.id === source.id) ?? null);
	const results = $derived(sourceVerdict?.rulesets ?? []);
	const positions = $derived(indexPositions(results));
	const groups = $derived(indexGroups(results));

	function resultOf(ruleset: MercRuleset) {
		return results.find((r) => r.id === ruleset.id) ?? null;
	}

	/** `null` = nothing to say yet (no capture); the engine owns every other value,
	 *  including `off` for a source the user switched out. */
	function headlineOf(id: MercSourceId) {
		if (!enabled.has(id)) return 'off' as const;
		return verdict?.sources.find((s) => s.id === id)?.headline ?? null;
	}

	// --- Settings -------------------------------------------------------------

	/** What Rust said no to, shown next to the toggles that produced it. Rust
	 *  validates the ids, so a refusal is a thing to render, not to swallow. */
	let sourcesError = $state<string | null>(null);

	/** Write the off-list through Rust. The checkbox does not move until the
	 *  echo comes back — the value is the SSOT's, and an optimistic local flip
	 *  would put this page one poll ahead of the overlay. */
	async function setSourceEnabled(id: MercSourceId, on: boolean): Promise<void> {
		sourcesError = await setMercSourcesOff(withSourceEnabled(merc.sourcesOff, id, on));
	}

	// --- Trade (POE-202) ------------------------------------------------------

	/** Rust owns the search; the page shows what it decided. `trade.status` is
	 *  `off` whenever the module is off, but `compose_snapshot` forces only the
	 *  STATUS and leaves `result` and `url` on the slice — so the whole section
	 *  keeps rendering either way, exactly as the capture and verdict cards keep
	 *  showing the retired capture. Only the badge changes. */
	const trade = $derived(merc.trade);
	const tradeRows = $derived((trade.result?.listings ?? []).map(mercListingRow));
	const tradeLine = $derived(tradeHeadline(trade));

	/** The tier the query comps down to. 3 is the mercenary exactly as read;
	 *  lower floors add the weaker grades of each support's own family, which is
	 *  a wider — and cheaper — market than the capture. */
	const TIER_FLOOR_OPTIONS = [
		{ value: '1', label: 'any tier' },
		{ value: '2', label: 'tier 2+' },
		{ value: '3', label: 'exact' }
	];

	/** What Rust said no to, shown where it was said. Both setters write through
	 *  Rust and re-read the slice, so the value the overlay reads is the same
	 *  one. The segmented buttons render straight off `merc.tierFloor` and so
	 *  never move ahead of it; the checkbox is the browser's own control and
	 *  does flip on click, which is why a refusal has to be shown rather than
	 *  left for the next poll to explain. */
	let tradeError = $state<string | null>(null);

	async function setTradeAuto(on: boolean): Promise<void> {
		tradeError = await setMercTradeAuto(on);
	}

	async function setTierFloor(floor: string): Promise<void> {
		tradeError = await setMercTierFloor(Number(floor));
	}

	// --- Module commands ------------------------------------------------------

	const pooledKeys = $derived(new Set(merc.pooledFamilies));
	const templates = $derived(
		merc.learnedFamilies.map((raw) => ({ raw, ...templateChip(raw, pooledKeys) }))
	);
	/** The shared pool's line (POE-201). `Date.now()` is read here rather than
	 *  kept in state: the slice is re-polled every 3 s and the age is worded to
	 *  the minute, so a ticking clock would buy nothing and churn the DOM. */
	const pool = $derived(poolSyncView(merc.sync, Date.now()));

	/** The manual half of the trigger (POE-198): the Client.txt voice line arms a
	 *  burst on its own, and this arms one for the window that was already open.
	 *  Rust refuses when the module is off or capture is unavailable, and the
	 *  refusal is shown — a button that silently does nothing teaches nothing. */
	let scanError = $state<string | null>(null);
	async function scanNow(): Promise<void> {
		scanError = null;
		try {
			await invoke('merc_scan_now');
		} catch (e) {
			scanError = `${e}`;
		}
	}

	let debugBusy = $state(false);
	let debugReport = $state<string | null>(null);
	let debugFailed = $state(false);
	let templateError = $state<string | null>(null);

	/** Take a debug dump on demand — the channel that turns the first Windows run
	 *  into calibration data. The report is shown verbatim, and so is the error:
	 *  a silent failure here is the one outcome that teaches nothing. */
	const DEBUG_DELAY_MS = 5000;
	let debugCountdown = $state(0);
	async function runDebugCapture(): Promise<void> {
		debugBusy = true;
		debugReport = null;
		debugFailed = false;
		// The delay is for a single screen: alt-tab to the game and the grab
		// happens once it is in front. The countdown is cosmetic; Rust sleeps.
		debugCountdown = DEBUG_DELAY_MS / 1000;
		const ticker = setInterval(() => {
			debugCountdown = Math.max(0, debugCountdown - 1);
		}, 1000);
		try {
			debugReport = describeDebugResult(
				await invoke('merc_debug_capture', { imagePath: null, delayMs: DEBUG_DELAY_MS })
			);
		} catch (e) {
			debugFailed = true;
			debugReport = `${e}`;
		} finally {
			clearInterval(ticker);
			debugCountdown = 0;
			debugBusy = false;
		}
	}

	/** The un-poison path for a template learned from a mistimed hover. Rust owns
	 *  the store, so the list refreshes on the next poll rather than locally. */
	async function forgetTemplate(family: string, tier: number | null): Promise<void> {
		templateError = null;
		try {
			await invoke('merc_forget_template', { family, tier });
		} catch (e) {
			templateError = `${e}`;
		}
	}

	async function resetTemplates(): Promise<void> {
		templateError = null;
		try {
			await invoke('merc_reset_templates');
		} catch (e) {
			templateError = `${e}`;
		}
	}

	// --- Rulesets (slice 1) ---------------------------------------------------

	/**
	 * The tier matrix is guide-b's presentation, not a generic capability — a
	 * future source with rungs of its own gets this treatment when it exists, not
	 * speculatively now.
	 */
	const ladder = $derived(
		source.id === 'guide-b' ? kinetistLadder().filter((r) => source.rulesets.includes(r)) : []
	);
	const rows = $derived(ladderRows(ladder));
	const cards = $derived(source.rulesets.filter((r) => !ladder.includes(r)));
	const ladderVerdict = $derived(rungOutcomes(ladder, results));
	/** The rungs share one entry skeleton, so their "not in these rules" lists
	 *  repeat — the matrix prints the union once instead of four identical lines. */
	const ladderNotInRules = $derived(
		notInRulesNames(results.filter((result) => ladder.some((rung) => rung.id === result.id)))
	);

	/** Status + item level as one line — identical across all four rungs today, so
	 *  the matrix prints it once instead of four times. */
	function metaText(ruleset: MercRuleset): string {
		return ruleset.ilvlMin === undefined
			? ruleset.status
			: `${ruleset.status} · ilvl ${ruleset.ilvlMin}+`;
	}

	const ladderMeta = $derived(sharedValue(ladder.map(metaText)));

	/**
	 * Wording for a matrix cell — `absent` is a hole in the skeleton, not a rule.
	 * With a capture in hand the cell also carries what this mercenary did with
	 * that rule, which is the only place guide-b's per-position debug can live:
	 * its rungs are columns here, not cards.
	 */
	function cellTitle(cell: LadderCell, rung: MercRuleset, groupId: string, entryId: string): string {
		const base = cell === 'absent' ? 'not in this rung' : kindTitle(cell);
		const position = positions.get(positionKey(rung.id, groupId, entryId));
		if (!position || capture === null) return base;
		return `${base} · ${positionOutcomeLabel(position.kind, position.outcome)} · ${capturedAt(capture, position.site)}`;
	}

	/**
	 * Leagues this source's saved searches live in that are NOT the league the SSOT
	 * resolved. A null `ssot.league` is "not known yet", not "different" — comparing
	 * against it would flash a mismatch badge on every cold start.
	 */
	function otherLeagues(rulesets: MercRuleset[]): string[] {
		if (ssot.league === null) return [];
		const league = ssot.league;
		return [...new Set(rulesets.map((r) => r.savedSearch.league))].filter((l) => l !== league);
	}
</script>

<div class="merc-page">
	<div class="page-head">
		<h1>Mercenaries</h1>
		<SegmentedButtons
			value={selectedSource}
			options={SOURCE_OPTIONS}
			onselect={(id) => (selectedSource = id as MercSourceId)}
			title="Which guide's rules to show. Each is a transcription of that guide's saved trade searches."
		/>
	</div>

	<p class="intro">
		Each ruleset is a transcription of a saved trade search, switches included.
		<span class="legend">
			<span class="legend-item entry-required"
				><span class="glyph" aria-hidden="true">{GLYPH.required}</span> required</span
			>
			<span class="legend-item entry-forbidden"
				><span class="glyph" aria-hidden="true">{GLYPH.forbidden}</span> denied</span
			>
			<span class="legend-item entry-bonus"
				><span class="glyph" aria-hidden="true">{GLYPH.bonus}</span> bonus — switched off, so
				upside rather than a requirement</span
			>
		</span>
	</p>

	<!-- 1. Capture status: what the module is doing, and the two levers that fix it. -->
	<section class="card status-card">
		<div class="card-head">
			<h2 class="card-title">Merc OCR</h2>
			<span class="badge tone-{STATUS_TONE[merc.status]}">{STATUS_LABEL[merc.status]}</span>
			<span class="meta">
				geometry: {merc.geometrySource === 'file' ? 'merc-geometry.json' : 'built-in reference'}
			</span>
			<span class="spacer"></span>
			<Button onclick={scanNow} title={SCAN_NOW_TITLE}>{SCAN_NOW_LABEL}</Button>
			<Button
				onclick={runDebugCapture}
				disabled={debugBusy}
				title="Wait 5 s (alt-tab to the game), then capture the screen and write a debug dump (screenshot, row crops, cell crops, report.json)."
			>
				{debugBusy ? (debugCountdown > 0 ? `Capturing in ${debugCountdown}…` : 'Capturing…') : 'Debug capture (5 s)'}
			</Button>
		</div>

		{#if scanError}
			<p class="error">{scanError}</p>
		{/if}

		{#if merc.lastError}
			<p class="error">Last error: {merc.lastError}</p>
		{/if}

		{#if debugReport}
			<pre class="report" class:error={debugFailed}>{debugReport}</pre>
		{/if}

		<div class="templates">
			<span class="templates-head">Learned icon templates ({templates.length})</span>
			<span class="pool" class:pool-ok={pool.tone === 'pass'} title={pool.detail ?? undefined}>
				{pool.label}
			</span>
			<!-- Where they live is a fact about the store, not about it being empty:
			     the un-poison path needs it most when the list is NOT empty. -->
			<span class="meta">stored under merc-icons/ in the app data directory</span>
			{#if templates.length === 0}
				<span class="meta">none yet — hover a support cell in game to teach one</span>
			{:else}
				<ul class="template-list">
					{#each templates as template (template.raw)}
						<li class="template" class:pooled={template.pooled} title={template.hint}>
							{#if template.pooled}
								<!-- The marker says "shared", the title says what that means. A
								     glyph alone would be a private code; the title is the channel
								     a screen reader gets. -->
								<span class="pooled-mark" aria-hidden="true">{POOLED_CHIP_MARK}</span>
								<span class="sr-only">from the shared pool:</span>
							{/if}
							<span>{template.label}</span>
							<button
								class="forget"
								onclick={() => forgetTemplate(template.family, template.tier)}
								aria-label="forget the learned template for {template.label}"
								title={template.hint}
							>
								✕
							</button>
						</li>
					{/each}
				</ul>
				<Button
					variant="danger"
					onclick={resetTemplates}
					title="Forget every learned template. Use this when a mistimed hover poisoned the store."
				>
					Reset learned templates
				</Button>
			{/if}
			{#if templateError}
				<p class="error">{templateError}</p>
			{/if}
		</div>
	</section>

	<!-- 2. What the reader saw, before any rule is applied to it. -->
	<section class="card capture-card">
		<div class="card-head">
			<h2 class="card-title">Last capture</h2>
			<!-- The capture glyphs are about CONFIDENCE, not about rules: without
			     this line the ✕ reads as the rulesets' "denied". -->
			<span class="legend read-legend">
				<span class="legend-item read-read"
					><span class="glyph" aria-hidden="true">{READ_GLYPH.matched}</span> read</span
				>
				<span class="legend-item read-unsure"
					><span class="glyph" aria-hidden="true">{READ_GLYPH.low_confidence}</span> unsure — hover
					to confirm</span
				>
				<span class="legend-item read-unread"
					><span class="glyph" aria-hidden="true">{READ_GLYPH.unknown}</span> not read</span
				>
			</span>
			{#if capture}
				<span class="meta">
					{captureLive ? 'on screen' : 'window gone'} · {capture.rows.length} rows · scale
					{capture.scale.toFixed(2)} · screen {capture.screen[0]}×{capture.screen[1]}
				</span>
			{/if}
		</div>

		{#if merc.status === 'unavailable'}
			<p class="stub">
				Capture is unavailable here — the module needs Windows and the system OCR engine. The rules
				below are readable anyway.
			</p>
		{:else if capture === null}
			<p class="stub">No capture yet. Turn on Merc OCR (sidebar) and open a recruit window.</p>
		{:else}
			<div class="capture-header">
				<span class="hdr"><span class="hdr-key">name</span> {capture.header.name ?? '—'}</span>
				<span class="hdr"><span class="hdr-key">class</span> {capture.header.class ?? '—'}</span>
				<span class="hdr"><span class="hdr-key">level</span> {capture.header.level ?? '—'}</span>
				<span class="hdr"><span class="hdr-key">wager</span> {capture.header.wager ?? '—'}</span>
			</div>

			<div class="rows-scroll">
				<table class="rows">
					<thead>
						<tr>
							<th class="num-col" scope="col">#</th>
							<th scope="col">skill</th>
							<th scope="col">supports</th>
						</tr>
					</thead>
					<tbody>
						{#each capture.rows as row (row.index)}
							<tr>
								<th class="num-col" scope="row">{row.index + 1}</th>
								<td class="read read-{READ_TONE[row.skill.state]}" title={skillTitle(row.skill)}>
									<span class="glyph" aria-hidden="true">{READ_GLYPH[row.skill.state]}</span>
									<span>{skillText(row.skill)}</span>
									<span class="visually-hidden">({READ_STATE_LABEL[row.skill.state]})</span>
								</td>
								<td>
									<!-- The flex box is a div INSIDE the cell: a `display: flex` table
									     cell is taken out of the table layout and browsers wrap it in an
									     anonymous cell, which is a layout surprise this row does not need. -->
									<div class="supports">
										{#if row.supports.length === 0}
											<span class="meta">no support cells</span>
										{:else}
											{#each row.supports as support (support.slot)}
												<span
													class="read read-{READ_TONE[support.state]}"
													title={supportTitle(support)}
												>
													<span class="glyph" aria-hidden="true">{READ_GLYPH[support.state]}</span>
													<span>{supportText(support)}</span>
													<span class="visually-hidden">({READ_STATE_LABEL[support.state]})</span>
												</span>
											{/each}
										{/if}
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	</section>

	<!-- 3. The verdict: one headline per source, then the selected source's work. -->
	<section class="card verdict-card">
		<div class="card-head">
			<h2 class="card-title">Verdict</h2>
			{#if capture === null}
				<span class="meta">nothing captured to judge yet</span>
			{/if}
		</div>

		<div class="headline-strip">
			{#each MERC_SOURCES as strip (strip.id)}
				{@const headline = headlineOf(strip.id)}
				<div class="headline" class:selected={strip.id === source.id}>
					<span class="headline-source">{strip.label}</span>
					{#if headline === null}
						<span class="badge tone-muted" title="no capture yet">—</span>
					{:else}
						<span class="badge tone-{HEADLINE_TONE[headline]}">{HEADLINE_LABEL[headline]}</span>
					{/if}
				</div>
			{/each}
		</div>

		{#if sourceVerdict && sourceVerdict.reasons.length > 0}
			<ul class="reasons">
				{#each sourceVerdict.reasons as reason (reason)}
					<li>{reason}</li>
				{/each}
			</ul>
		{:else if sourceVerdict && sourceVerdict.headline === 'off'}
			<p class="stub">
				{source.label} is switched off in Settings below — its rules are shown, not evaluated.
			</p>
		{/if}
	</section>

	<!-- 4. Trade: what this exact mercenary is going for, searched automatically. -->
	<section class="card trade-card">
		<div class="card-head">
			<h2 class="card-title">Trade</h2>
			<span class="badge tone-{tradeStatusTone(trade)}">{tradeStatusLabel(trade)}</span>
			{#if trade.searchesUsed > 0}
				<span
					class="meta"
					title="The app searches at most {MERC_TRADE_MAX_SEARCHES} times per captured mercenary. Once the budget is spent the link still works."
				>
					{trade.searchesUsed}/{MERC_TRADE_MAX_SEARCHES} searches this capture
				</span>
			{/if}
			<span class="spacer"></span>
			{#if trade.url}
				<a class="guide-link" href={trade.url} target="_blank">trade ↗</a>
			{/if}
		</div>

		<!-- Rendered whatever the status, module off included: Rust keeps the
		     link and the listings on the slice through a force-off, and dropping
		     them here would throw away an answer that is still true about the
		     capture the rest of the page is still showing. -->
		{#if tradeLine}
			<p class="trade-headline">{tradeLine}</p>
		{/if}
		<!-- Prices are the sellers' own numbers in the sellers' own currency:
		     this page has no divine rate, so there is nothing to convert with
		     and a converted column would be invented. The order is GGG's
		     `price asc`, which IS a value order. -->
		<TradeListings rows={tradeRows} rawCurrency />

		<!-- Both settings stay rendered with the module off (ADR-014): the page is
		     browsable either way. -->
		<div class="trade-settings">
			<label class="source-toggle">
				<input
					type="checkbox"
					checked={merc.tradeAuto}
					onchange={(e) => setTradeAuto(e.currentTarget.checked)}
				/>
				<span>Search automatically</span>
			</label>
			<span class="trade-floor">
				<span class="meta">Support tiers</span>
				<SegmentedButtons
					value={String(merc.tierFloor)}
					options={TIER_FLOOR_OPTIONS}
					onselect={setTierFloor}
					title="How far below the read tier the search comps. Exact prices the mercenary as captured; a lower floor also matches the weaker grades of each support, which is a wider and cheaper market."
				/>
			</span>
		</div>

		{#if tradeError}
			<p class="settings-error">{tradeError}</p>
		{/if}
	</section>

	<section class="panel">
		<header class="panel-head">
			<h2>{source.label}</h2>
			{#if source.guideUrl}
				<a class="guide-link" href={source.guideUrl} target="_blank">guide</a>
			{:else}
				<!-- Provenance is pending, not nonexistent — the URL is a known gap. -->
				<span class="guide-pending">guide URL pending</span>
			{/if}
			{#each otherLeagues(source.rulesets) as league (league)}
				<span class="league-badge">saved in {league}</span>
			{/each}
		</header>

		<div class="grid">
			{#if rows.length > 0}
				<article class="card matrix-card">
					<header class="card-head">
						<h3 class="card-title">Kinetist — tier ladder</h3>
						{#if ladderMeta}<span class="meta">{ladderMeta}</span>{/if}
					</header>
					<p class="matrix-hint">
						One search, four rungs. Highlighted rows are the ones that move between them.
					</p>

					<!-- Never wraps: the columns hold their width and the card scrolls. -->
					<div class="matrix-scroll">
						<table class="matrix">
							<thead>
								<tr>
									<th class="rule-col" scope="col">rule</th>
									{#each ladder as rung (rung.id)}
										<th class="tier-col" scope="col">
											<span class="tier-name">{columnLabel(rung)}</span>
											<a
												class="search-link"
												href={savedSearchUrl(rung.savedSearch)}
												target="_blank"
												aria-label="open saved search — {source.label} {rung.label} {columnLabel(
													rung
												)}"
											>
												↗
											</a>
											{#if !ladderMeta}<span class="meta">{metaText(rung)}</span>{/if}
											{#if rung.floor}<span class="floor">{rung.floor}</span>{/if}
										</th>
									{/each}
								</tr>
								{#if results.length > 0}
									<!-- The rung-by-rung answer for the captured mercenary; blank
									     where no result exists (source off, or rungs not verdicted). -->
									<tr class="verdict-row">
										<th class="rule-col" scope="row">
											this mercenary
											{#if ssot.league === null}
												<!-- The rungs are columns, not cards, so this is where guide B's
												     missing-league note has to live. Once, not per column: the
												     league is one fact about the app, not four about the rungs. -->
												<span class="meta">— derived links need an active league</span>
											{/if}
										</th>
										{#each ladderVerdict as outcome, i (ladder[i].id)}
											<td class="tier-verdict">
												{#if outcome === null}
													<span class="meta">—</span>
												{:else}
													<span class="badge tone-{RULESET_OUTCOME_TONE[outcome]}"
														>{RULESET_OUTCOME_LABEL[outcome]}</span
													>
													{#if resultOf(ladder[i])?.derivedUrl}
														<a
															class="search-link"
															href={resultOf(ladder[i])!.derivedUrl}
															target="_blank"
															aria-label="open derived search — {source.label} {columnLabel(
																ladder[i]
															)}"
														>
															derived ↗
														</a>
													{/if}
												{/if}
											</td>
										{/each}
									</tr>
								{/if}
							</thead>
							<tbody>
								{#each rows as row (row.id)}
									{#if row.kind === 'group'}
										<tr class="group-row" class:varies={row.varies}>
											<th class="group-cell" colspan={ladder.length + 1} scope="colgroup">
												<span class="group-label">{row.label}</span>
												<span class="quantifier">{row.quantifier}</span>
												{#if row.offIn.length > 0}
													<span class="off-badge">off in {row.offIn.join(', ')}</span>
												{/if}
											</th>
										</tr>
									{:else}
										<tr class="entry-row" class:varies={row.varies}>
											<th class="name-cell" scope="row">
												{row.name}
												{#if row.varies}
													<!-- The tint is the visual carrier; this is the same fact
													     for readers the tint cannot reach. -->
													<span class="visually-hidden">(varies between rungs)</span>
												{/if}
											</th>
											{#each row.cells as cell, i (ladder[i].id)}
												{@const outcome =
													positions.get(
														positionKey(ladder[i].id, row.groupId, row.entryId)
													)?.outcome ?? null}
												<td
													class="cell entry-{cell}"
													class:outcome-pass={outcome !== null &&
														POSITION_OUTCOME_TONE[outcome] === 'pass'}
													class:outcome-fail={outcome !== null &&
														POSITION_OUTCOME_TONE[outcome] === 'fail'}
													class:outcome-unknown={outcome !== null &&
														POSITION_OUTCOME_TONE[outcome] === 'unknown'}
													class:outcome-bonus={outcome !== null &&
														POSITION_OUTCOME_TONE[outcome] === 'bonus'}
													title={cellTitle(cell, ladder[i], row.groupId, row.entryId)}
													aria-label={cellTitle(cell, ladder[i], row.groupId, row.entryId)}
												>
													<span class="glyph" aria-hidden="true">{GLYPH[cell]}</span>
												</td>
											{/each}
										</tr>
									{/if}
								{/each}
							</tbody>
						</table>
					</div>

					{#if ladderNotInRules.length > 0}
						<p class="not-in-rules">Not in these rules: {ladderNotInRules.join(', ')}</p>
					{/if}
				</article>
			{/if}

			{#each cards as ruleset (ruleset.id)}
				{@const result = resultOf(ruleset)}
				<article class="card">
					<header class="card-head">
						<h3 class="card-title">{ruleset.label}</h3>
						{#if result}
							<span class="badge tone-{RULESET_OUTCOME_TONE[result.outcome]}"
								>{RULESET_OUTCOME_LABEL[result.outcome]}</span
							>
						{/if}
						<a
							class="search-link"
							href={savedSearchUrl(ruleset.savedSearch)}
							target="_blank"
							aria-label="open saved search — {source.label} {ruleset.label}"
						>
							open saved search ↗
						</a>
						{#if result}
							{#if result.derivedUrl}
								<a
									class="search-link"
									href={result.derivedUrl}
									target="_blank"
									aria-label="open derived search — {source.label} {ruleset.label}, with this mercenary's bonuses switched on"
								>
									derived ↗
								</a>
							{:else}
								<span class="meta">derived link needs an active league</span>
							{/if}
						{/if}
					</header>
					<div class="meta">{metaText(ruleset)}</div>
					{#if ruleset.floor}
						<p class="floor">Floor: {ruleset.floor}</p>
					{/if}

					{#if result && result.reasons.length > 0}
						<ul class="reasons">
							{#each result.reasons as reason (reason)}
								<li>{reason}</li>
							{/each}
						</ul>
					{/if}

					{#each ruleset.groups as group (group.id)}
						{@const groupResult = groups.get(groupKey(ruleset.id, group.id)) ?? null}
						<div class="group">
							<div class="group-head">
								<span class="group-label">{group.label}</span>
								<span class="quantifier">{quantifier(group)}</span>
								{#if !group.enabledInSearch}
									<span class="off-badge">off in this search</span>
								{/if}
								{#if groupResult}
									<span class="badge tone-{GROUP_OUTCOME_TONE[groupResult.outcome]}"
										>{GROUP_OUTCOME_LABEL[groupResult.outcome]}</span
									>
									{#if groupResult.need > 0}
										<span class="meta">
											{groupResult.confident} of {groupResult.need}{groupResult.rowIndex === null
												? ''
												: ` · row ${groupResult.rowIndex + 1}`}
										</span>
									{/if}
								{/if}
							</div>
							<ul class="entries">
								<!-- Keys are unique per group (asserted upstream in rulesets.test.ts);
								     the same id may legitimately recur across sibling groups. -->
								{#each group.entries as entry (entry.id)}
									{@const kind = entryKind(group, entry)}
									{@const position = positions.get(positionKey(ruleset.id, group.id, entry.id)) ?? null}
									<li class="entry entry-{kind}" title={kindTitle(kind)}>
										<span class="glyph" aria-hidden="true">{GLYPH[kind]}</span>
										<span class="entry-name">{entry.name}</span>
										{#if position && capture}
											<span class="captured">{capturedAt(capture, position.site)}</span>
											<span class="badge tone-{POSITION_OUTCOME_TONE[position.outcome]}"
												>{positionOutcomeLabel(position.kind, position.outcome)}</span
											>
										{/if}
									</li>
								{/each}
							</ul>
						</div>
					{/each}

					{#if result && result.notInRules.length > 0}
						<p class="not-in-rules">Not in these rules: {notInRulesNames([result]).join(', ')}</p>
					{/if}
				</article>
			{/each}
		</div>
	</section>

	<!-- 5. Settings: which sources take part in the verdict. -->
	<section class="card settings-card">
		<h2 class="card-title">Settings</h2>
		<p class="meta">
			A source switched off keeps its rules on the page but takes no part in the verdict — its
			headline reads OFF. The choice is shared with the verdict overlay, so both always judge a
			mercenary by the same guides.
		</p>
		<ul class="source-toggles">
			{#each MERC_SOURCES as toggleSource (toggleSource.id)}
				<li>
					<label class="source-toggle">
						<input
							type="checkbox"
							checked={enabled.has(toggleSource.id)}
							onchange={(e) => setSourceEnabled(toggleSource.id, e.currentTarget.checked)}
						/>
						<span>{toggleSource.label}</span>
					</label>
				</li>
			{/each}
		</ul>
		{#if sourcesError}
			<p class="settings-error">{sourcesError}</p>
		{/if}
	</section>
</div>

<style>
	.settings-error {
		margin-top: 0.5rem;
		font-size: 0.8rem;
		color: var(--color-lab-red);
	}

	.merc-page {
		max-width: 1400px;
		margin: 0 auto;
		color: var(--color-lab-text);
	}

	.page-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 0.75rem;
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

	.legend {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 0.9rem;
		margin-left: 0.5rem;
	}

	.legend-item {
		white-space: nowrap;
	}

	/* The capture card's own legend rides the card head, so it stays smaller and
	   quieter than the page-level one under the intro. */
	.read-legend {
		font-size: 0.68rem;
		gap: 0.7rem;
		color: var(--color-lab-text-muted);
	}

	.card,
	.panel {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		border-radius: 8px;
		padding: 1rem;
	}

	.status-card,
	.capture-card,
	.verdict-card,
	.trade-card,
	.settings-card {
		margin-bottom: 1rem;
	}

	.trade-headline {
		font-size: 0.82rem;
		color: var(--color-lab-text);
		margin-top: 0.5rem;
	}

	.trade-settings {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 1rem;
		margin-top: 0.75rem;
	}

	.trade-floor {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
	}

	.settings-card {
		margin-top: 1rem;
		margin-bottom: 0;
	}

	.stub {
		font-size: 0.8rem;
		color: var(--color-lab-text-secondary);
	}

	.spacer {
		flex: 1 1 auto;
	}

	.error {
		font-size: 0.75rem;
		color: var(--color-lab-red);
		margin-top: 0.4rem;
	}

	.report {
		font-size: 0.7rem;
		line-height: 1.45;
		color: var(--color-lab-text-secondary);
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		border-radius: 4px;
		padding: 0.5rem;
		margin-top: 0.5rem;
		max-height: 240px;
		overflow: auto;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.report.error {
		color: var(--color-lab-red);
	}

	/* --- learned templates --- */

	.templates {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-top: 0.6rem;
	}

	.templates-head {
		font-size: 0.72rem;
		font-weight: 600;
		color: var(--color-lab-text);
	}

	.template-list {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
		list-style: none;
		padding-left: 0;
	}

	.pool {
		font-size: 0.68rem;
		color: var(--color-lab-text-muted);
	}

	.pool-ok {
		color: var(--color-lab-text-secondary);
	}

	.pooled-mark {
		font-size: 0.65rem;
		color: var(--color-lab-text-muted);
	}

	.template.pooled {
		border-style: dashed;
	}

	/* Visually hidden, still read aloud — the provenance must not be a
	   glyph-only fact. */
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}

	.template {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.7rem;
		color: var(--color-lab-text-secondary);
		border: 1px solid var(--color-lab-border);
		border-radius: 999px;
		padding: 1px 4px 1px 8px;
	}

	.forget {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--color-lab-text-muted);
		font-size: 0.7rem;
		line-height: 1;
		padding: 2px 4px;
		border-radius: 3px;
	}

	.forget:hover {
		color: var(--color-lab-red);
	}

	.forget:focus-visible {
		outline: 1px solid var(--color-lab-blue);
		outline-offset: 1px;
	}

	/* --- capture rows --- */

	.capture-header {
		display: flex;
		flex-wrap: wrap;
		gap: 0.9rem;
		font-size: 0.75rem;
		margin: 0.4rem 0 0.6rem;
	}

	.hdr-key {
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-lab-text-muted);
		margin-right: 0.25rem;
	}

	.rows-scroll {
		overflow-x: auto;
		/* Same reason as the matrix: a horizontal bar must not spawn a vertical one. */
		overflow-y: hidden;
	}

	.rows {
		border-collapse: collapse;
		width: 100%;
	}

	.rows th,
	.rows td {
		text-align: left;
		vertical-align: top;
		padding: 3px 8px;
		font-weight: 400;
		font-size: 0.75rem;
	}

	.rows thead th {
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-lab-text-muted);
		border-bottom: 1px solid var(--color-lab-border);
	}

	.num-col {
		width: 2rem;
		color: var(--color-lab-text-muted);
	}

	.supports {
		display: flex;
		flex-wrap: wrap;
		gap: 0.15rem 0.9rem;
	}

	.read {
		display: inline-flex;
		align-items: baseline;
		gap: 0.35rem;
	}

	.read-read .glyph {
		color: var(--color-lab-green);
	}

	.read-unsure .glyph {
		color: var(--color-lab-yellow);
	}

	.read-unread .glyph {
		color: var(--color-lab-red);
	}

	.read-unsure,
	.read-unread {
		color: var(--color-lab-text-secondary);
	}

	/* --- verdict --- */

	.headline-strip {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin: 0.4rem 0;
	}

	.headline {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.75rem;
		border: 1px solid var(--color-lab-border);
		border-radius: 6px;
		padding: 3px 8px;
	}

	.headline.selected {
		border-color: var(--color-lab-purple);
	}

	.headline-source {
		color: var(--color-lab-text);
	}

	.reasons {
		list-style: none;
		padding-left: 0.15rem;
		margin-top: 0.3rem;
		font-size: 0.72rem;
		line-height: 1.55;
		color: var(--color-lab-text-secondary);
	}

	.not-in-rules {
		font-size: 0.7rem;
		color: var(--color-lab-text-muted);
		margin-top: 0.5rem;
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

	.tone-unknown {
		color: var(--color-lab-yellow);
	}

	.tone-bonus {
		color: var(--color-lab-blue);
	}

	.tone-muted {
		color: var(--color-lab-text-muted);
	}

	.captured {
		margin-left: auto;
		font-size: 0.68rem;
		color: var(--color-lab-text-muted);
		text-align: right;
	}

	/* The source you are inside is the loudest thing on the page — it used to be a
	   muted banner under headings that shouted over it. */
	.panel-head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 0.6rem;
		margin-bottom: 0.75rem;
	}

	.panel-head h2 {
		font-size: 1.05rem;
		font-weight: 600;
		color: var(--color-lab-text);
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(340px, 100%), 1fr));
		gap: 0.75rem;
		align-items: start;
	}

	/* The matrix is the source's headline, and it is wider than one grid track. */
	.matrix-card {
		grid-column: 1 / -1;
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

	.guide-link,
	.search-link {
		font-size: 0.7rem;
		color: var(--color-lab-blue);
		text-decoration: none;
	}

	.guide-link:hover,
	.search-link:hover {
		text-decoration: underline;
	}

	.guide-link:focus-visible,
	.search-link:focus-visible {
		outline: 1px solid var(--color-lab-blue);
		outline-offset: 2px;
	}

	.visually-hidden {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}

	.guide-pending {
		font-size: 0.7rem;
		color: var(--color-lab-text-secondary);
		font-style: italic;
	}

	.league-badge {
		font-size: 0.65rem;
		color: var(--color-lab-yellow);
		border: 1px solid var(--color-lab-yellow);
		border-radius: 999px;
		padding: 1px 7px;
	}

	.meta {
		font-size: 0.7rem;
		color: var(--color-lab-text-muted);
	}

	.floor {
		font-size: 0.72rem;
		color: var(--color-lab-text-secondary);
	}

	.matrix-hint {
		font-size: 0.72rem;
		color: var(--color-lab-text-muted);
		margin: 0.15rem 0 0.5rem;
	}

	.matrix-scroll {
		overflow-x: auto;
		/* `overflow-x: auto` alone COMPUTES overflow-y to auto as well (the spec
		 * forbids visible on one axis with non-visible on the other), and on
		 * Windows classic scrollbars any horizontal bar's own height then spawns
		 * a spurious vertical bar — the double-scrollbar Sebastian hit. The
		 * matrix never needs to scroll vertically; its height is its content. */
		overflow-y: hidden;
	}

	.matrix {
		border-collapse: collapse;
		width: 100%;
	}

	.matrix th,
	.matrix td {
		text-align: left;
		vertical-align: top;
		padding: 2px 8px;
		font-weight: 400;
	}

	.rule-col {
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-lab-text-muted);
		min-width: 260px;
	}

	.tier-col {
		min-width: 92px;
		border-bottom: 1px solid var(--color-lab-border);
		padding-bottom: 0.35rem;
	}

	.tier-name {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--color-lab-purple);
		margin-right: 0.3rem;
	}

	.tier-col .meta,
	.tier-col .floor {
		display: block;
	}

	.verdict-row .tier-verdict {
		padding-bottom: 0.4rem;
		border-bottom: 1px solid var(--color-lab-border);
	}

	.group-row .group-cell {
		padding-top: 0.7rem;
	}

	.entry-row .name-cell {
		font-size: 0.75rem;
		line-height: 1.5;
		color: var(--color-lab-text);
	}

	.cell {
		line-height: 1.5;
	}

	/* The verdict tint sits UNDER the kind glyph rather than replacing it: the
	   cell still says what the rule is, and now also what this mercenary did with
	   it. Both facts are in the cell's title for readers the tint cannot reach.

	   The `.matrix .cell` qualifier is load-bearing: `.varies th, .varies td`
	   below is (0,1,1) and would otherwise win over a bare `.outcome-*` (0,1,0),
	   so a delta row — exactly the row a reader is comparing rungs on — would
	   show no outcome at all. The row's name cell keeps the varies tint, so the
	   delta cue survives the override. */
	.matrix .cell.outcome-pass {
		background: rgba(34, 197, 94, 0.12);
	}

	.matrix .cell.outcome-fail {
		background: rgba(239, 68, 68, 0.12);
	}

	.matrix .cell.outcome-unknown {
		background: rgba(234, 179, 8, 0.12);
	}

	.matrix .cell.outcome-bonus {
		background: rgba(59, 130, 246, 0.12);
	}

	/* The deltas are what a reader comparing rungs is hunting for; everything else
	   is the skeleton they share. */
	.varies th,
	.varies td {
		background: rgba(168, 85, 247, 0.08);
	}

	.group {
		margin-top: 0.6rem;
	}

	.group-head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 0.4rem;
		margin-bottom: 0.15rem;
	}

	.group-label {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--color-lab-text);
	}

	.off-badge {
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-lab-text-muted);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		padding: 0 4px;
	}

	.quantifier {
		font-size: 0.7rem;
		color: var(--color-lab-text-muted);
	}

	/* Rows, not pills: no border, no fill, no per-row padding — the glyph column is
	   the only structure, so a long list scans as a list. */
	.entries {
		list-style: none;
		padding-left: 0.15rem;
	}

	.entry {
		display: flex;
		align-items: baseline;
		gap: 0.4rem;
		font-size: 0.75rem;
		line-height: 1.5;
	}

	.glyph {
		display: inline-block;
		width: 0.9em;
		text-align: center;
		flex-shrink: 0;
	}

	.entry-name {
		color: var(--color-lab-text);
	}

	.entry-required .glyph {
		color: var(--color-lab-green);
	}

	.entry-forbidden .glyph {
		color: var(--color-lab-red);
	}

	.entry-bonus .glyph {
		color: var(--color-lab-blue);
	}

	/* Colour IS the dimming for a bonus row — stacking opacity on top of a coloured
	   glyph is what dropped the parked Pierce denials to ~1.5:1 in the pill design. */
	.entry-bonus .entry-name {
		color: var(--color-lab-text-secondary);
	}

	.entry-absent .glyph {
		color: var(--color-lab-text-muted);
	}

	/* --- settings --- */

	.source-toggles {
		list-style: none;
		padding-left: 0;
		margin-top: 0.5rem;
		display: flex;
		flex-wrap: wrap;
		gap: 0.9rem;
	}

	.source-toggle {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.78rem;
		color: var(--color-lab-text);
		cursor: pointer;
	}

	.source-toggle input {
		accent-color: var(--color-lab-green);
		cursor: pointer;
	}

	.source-toggle input:focus-visible {
		outline: 1px solid var(--color-lab-blue);
		outline-offset: 2px;
	}
</style>
