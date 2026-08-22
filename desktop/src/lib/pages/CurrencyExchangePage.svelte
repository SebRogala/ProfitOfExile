<script lang="ts">
	/**
	 * Currency Exchange — the ranked arbitrage plays the server computes each
	 * hour (POE-175/188), as a table with two layers of filters over it.
	 *
	 * Deliberately thin. Every derivation the header, the filters and the cells
	 * need lives in `$lib/exchange/view` and `$lib/exchange/filters`, which are
	 * pure and unit-tested; this file owns the persisted picks, the fetch that
	 * fills the four loose state variables, and the markup. A `.svelte` file has
	 * no unit-test harness here, so anything with a rule in it belongs on the
	 * other side of those imports.
	 *
	 * Everything on screen is the LAST SETTLED feed hour, which is why the status
	 * line leads with how old that hour is rather than footnoting it: the feed
	 * publishes 40-60 minutes after an hour closes, so a reader who takes these
	 * numbers for the live book is reading something up to two hours stale.
	 *
	 * It does not open its own Mercure connection: `LabPage.svelte` owns the one
	 * connection for the whole app (ADR-014 keeps every page mounted) and
	 * re-emits the Currency Exchange payloads as a Tauri `currency-exchange-updated`
	 * event, which this page listens for and answers with a jittered refetch.
	 *
	 * A failed fetch never blanks the table: the last plays stay on screen and
	 * the header says how old they are. The alternative — an empty page on a
	 * dropped connection — throws away the only data the user has while the
	 * server is the thing that is down.
	 */
	import { listen } from '@tauri-apps/api/event';
	import { fetchCurrencyExchangePlays, getApiBase, type CurrencyExchangeResponse } from '$lib/api';
	import {
		DENSITY_OPTIONS,
		HORIZON_OPTIONS,
		MODE_OPTIONS,
		SORT_OPTIONS,
		anyConvertStep,
		dataAgeParts,
		deriveState,
		formatChaos,
		formatGain,
		formatRoiPct,
		formatVolume,
		hoursProgress,
		moneyColumns,
		parseDensity,
		parseHorizon,
		parseMode,
		parseSort,
		parseUnit,
		refetchDelay,
		sortPlays,
		worthwhileScale
	} from '$lib/exchange/view';
	import {
		applyGates,
		applyNumericFilters,
		applyRules,
		parseGates,
		resetGateInputs,
		itemUniverse,
		matchesSearch,
		movedGates,
		parseCategoryRules,
		parseItemRules,
		serializeCategoryRules,
		serializeItemRules,
		type CategoryRuleState,
		type Gates
	} from '$lib/exchange/filters';
	import { EXCHANGE_TOOLTIPS } from '$lib/tooltips';
	import { persisted, type PersistedString } from '$lib/prefs.svelte';
	import ExchangeFilterBar from '$lib/components/ExchangeFilterBar.svelte';
	import ExchangeRoute from '$lib/components/ExchangeRoute.svelte';
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte';
	import Tooltip from '$lib/components/Tooltip.svelte';

	/**
	 * Every pick the reader makes survives a restart (ADR-013). All strings,
	 * validated on read by the pure parsers rather than on write: a preference
	 * written by an older build, or a half-typed number still in an input, has to
	 * degrade to a usable default instead of throwing on first render.
	 */
	const mode = persisted('currencyExchangeMode', 'all');
	const horizon = persisted('currencyExchangeHorizon', 'recent');
	const sortPref = persisted('currencyExchangeSort', 'expected');
	const densityPref = persisted('currencyExchangeDensity', 'comfortable');
	const unitPref = persisted('currencyExchangeUnit', 'chaos');
	/**
	 * The investment bounds, under keys that carry SCALE in the name (POE-192).
	 *
	 * Deliberately not the `currencyExchangeInvestMin`/`Max` the reader may still
	 * have on disk. The bounds used to be compared against ONE exchange's cost and
	 * are now compared against what the worthwhile run ties up — a figure tens or
	 * hundreds of times larger — so a 500c ceiling typed against the old meaning
	 * would silently empty the new table, with nothing on screen to say why. New
	 * keys start the reader from unset; the stale pair is simply never read again.
	 */
	const investMinPref = persisted('currencyExchangeScaleInvestMin', '');
	const investMaxPref = persisted('currencyExchangeScaleInvestMax', '');
	const categoryRulesPref = persisted('currencyExchangeCategoryRules', '{}');
	const itemRulesPref = persisted('currencyExchangeItemRules', '[]');

	/**
	 * The six quality gates (POE-191, plus POE-196's trash-price knob), one
	 * preference each.
	 *
	 * Every one defaults to '' and NOT to the number it stands for, because ''
	 * already means that number: `parseGate` reads an unset knob as its default,
	 * so an empty preference and a preference holding that default filter
	 * identically. The empty one has the property the literal lacks — a default
	 * this build changes reaches the reader who never touched the knob, instead of
	 * leaving them pinned to a number an older build wrote into their settings
	 * file. POE-193 is that case: the defaults went to 0 and every reader who had
	 * not typed a level got the whole served table, with no migration.
	 *
	 * `minItemPrice` is the reason that property still earns its keep: it is the
	 * one knob whose default is not 0 (0.5 chaos, ADR-017's sanctioned exception),
	 * so '' has to keep meaning "whatever this build says" rather than a number
	 * frozen at install time. Blanking that box restores the shipped floor; only
	 * an explicit 0 turns it off.
	 *
	 * `minRoiPct` keeps its original key: it is the same knob the reader has been
	 * setting since POE-186, moved into the group rather than replaced, and
	 * renaming it would silently drop the floor of anyone who had one.
	 */
	const gatePrefs: Record<keyof Gates, PersistedString> = {
		minItemPrice: persisted('currencyExchangeGateMinItemPrice', ''),
		minRoiChaos: persisted('currencyExchangeGateMinRoiChaos', ''),
		minTurnover: persisted('currencyExchangeGateMinTurnover', ''),
		maxTickPct: persisted('currencyExchangeGateMaxTickPct', ''),
		minEdgeTickRatio: persisted('currencyExchangeGateMinEdgeTickRatio', ''),
		minRoiPct: persisted('currencyExchangeMinRoiPct', '')
	};

	/**
	 * The base the legs' relative icon paths hang off. `$derived` rather than a
	 * const read at init: this page is mounted for the life of the app (ADR-014),
	 * so on a cold start it renders before the Rust status carrying `server_url`
	 * arrives, and a value captured then would pin every icon to the fallback
	 * base for the whole session. One call feeds every tile on screen.
	 */
	const apiBase = $derived(getApiBase());

	let result = $state<CurrencyExchangeResponse | null>(null);
	let lastFetchedAt = $state<Date | null>(null);
	let lastError = $state<string | null>(null);
	let loading = $state(false);

	/**
	 * The clock the relative strings are measured against. Ticked rather than
	 * read inline: "as of 14:35 (3 min ago)" is derived once and would otherwise
	 * stay frozen at its fetch-time wording until the next Mercure publish, which
	 * on a quiet hour is an hour away.
	 */
	let now = $state(new Date());
	const NOW_TICK_MS = 30_000;

	const viewState = $derived(deriveState({ result, lastFetchedAt, lastError, now }));
	const age = $derived(dataAgeParts(result, now));

	// ------------------------------------------------------------ the picks --

	const density = $derived(parseDensity(densityPref.value));
	const dense = $derived(density === 'dense');
	const unit = $derived(parseUnit(unitPref.value));
	const categoryRules = $derived(parseCategoryRules(categoryRulesPref.value));
	const itemRules = $derived(parseItemRules(itemRulesPref.value));

	/**
	 * The gate knobs, raw and parsed. Both go to the filter bar: the boxes show
	 * the raw strings (so a half-typed number is not fought while it is typed) and
	 * the badge measures the parsed ones against `gateDefaults`. Parsed once here
	 * rather than once here and once in the bar, so the filter and the badge
	 * cannot disagree about whether a knob is at its default.
	 */
	const gateInputs = $derived({
		minItemPrice: gatePrefs.minItemPrice.value,
		minRoiChaos: gatePrefs.minRoiChaos.value,
		minTurnover: gatePrefs.minTurnover.value,
		maxTickPct: gatePrefs.maxTickPct.value,
		minEdgeTickRatio: gatePrefs.minEdgeTickRatio.value,
		minRoiPct: gatePrefs.minRoiPct.value
	});
	const gates = $derived(parseGates(gateInputs));

	/**
	 * The filter bar's search box, as typed. Plain `$state` and deliberately NOT
	 * `persisted()`, the one pick on this page that is not: everything else here
	 * is a setup the reader built and expects back, and a search is a moment —
	 * restoring one would open the app on a table narrowed by a word they typed
	 * days ago, with the reason for it sitting in a box they are not looking at.
	 */
	let search = $state('');

	// ----------------------------------------------------------- the filters --

	/** The response's own list — what the counter counts against and what the
	 *  picker's universe is built from, never the filtered one. */
	const allPlays = $derived(result?.plays ?? []);
	const items = $derived(itemUniverse(allPlays));
	const hoursWindow = $derived(result?.hours ?? 0);

	/**
	 * Rules, then gates, then numbers, then the search, then order. The two rule
	 * layers narrow by identity and the numeric bounds by size, so running them
	 * the other way round would cost the same rows at more comparisons; the search
	 * runs last of the four because it narrows what the persisted setup has
	 * already left on screen rather than joining it. The sort is last of all
	 * because it is the only step whose answer depends on how many rows survived.
	 *
	 * The gate step is held in its own `$derived` rather than folded into the
	 * chain, because the counter has to be able to say how many rows the GATES
	 * took: since POE-191 they are the one filter that hides rows the reader never
	 * set, and a lump "hidden by filters" over a bar with nothing visibly on it
	 * reads as a broken table rather than as a knob to turn. POE-196 makes that
	 * figure non-zero on a fresh install — the trash-price knob ships armed — so
	 * the split is now what tells the reader the missing sub-chaos rows are a
	 * default they can undo rather than rows the server never sent.
	 *
	 * `rows` is what the counter counts, so the shown figure is the post-search
	 * one: a query that hides a play is one of the reasons it is not on the table.
	 */
	const afterRules = $derived(applyRules(allPlays, categoryRules, itemRules));
	const afterGates = $derived(applyGates(afterRules, gates));
	const rows = $derived(
		sortPlays(
			applyNumericFilters(afterGates, {
				investMin: investMinPref.value,
				investMax: investMaxPref.value,
				unit,
				divineChaosRate: result?.divineChaosRate ?? 0
			}).filter((play) => matchesSearch(play, search)),
			parseSort(sortPref.value)
		)
	);

	/**
	 * Whether the route's convert slot is drawn, for the whole table at once.
	 *
	 * Taken over `rows` — what is actually rendered — and not over `allPlays`: a
	 * reader who has filtered down to direct plays is looking at a column of empty
	 * dashed tiles, and the response still holding a 1-hop they cannot see is no
	 * reason to keep spending the width on it. Both the header spans and every
	 * `ExchangeRoute` read this one value, which is what keeps the two geometries
	 * mirrored; the rule against deciding it per row lives with `anyConvertStep`.
	 */
	const showConvert = $derived(anyConvertStep(rows));

	/**
	 * The counter's attribution. Gates get their own figure; the rules, the
	 * bounds and the search share the other, because those three are all things
	 * the reader can see they set on a bar that is always open — a gate's controls
	 * are behind a collapsed row, so its rows are the ones that need pointing at.
	 */
	const counts = $derived({
		shown: rows.length,
		total: allPlays.length,
		hiddenByGates: afterRules.length - afterGates.length,
		hiddenByFilters: allPlays.length - rows.length - (afterRules.length - afterGates.length)
	});

	/**
	 * What the empty table says when the GATES are what emptied it.
	 *
	 * Two wordings, because "lower your gates" is wrong advice for the one gate
	 * the reader did not set. When nothing is off its shipped default, the only
	 * knob that can have emptied the table is POE-196's item-price floor — a
	 * reachable state, not a theoretical one: a fresh install on a thin hour whose
	 * served plays are all sub-floor gets exactly this. Naming the floor and the
	 * one number that disables it is the difference between a dead end and a
	 * control.
	 *
	 * `movedGates` is the same verdict the filter bar badges with, so the message
	 * and the badge cannot disagree about whether the reader has touched anything.
	 */
	const gatesEmptyMessage = $derived(
		movedGates(gates).length === 0 && gates.minItemPrice > 0
			? `No play served this hour costs the ${gates.minItemPrice}c minimum item price to enter — that floor is the one filter this app arms for you. Type 0 into Min item price in the filter bar’s Gates row to see the cheaper plays.`
			: 'No plays clear your gates right now — lower them in the filter bar’s Gates row.'
	);

	/**
	 * A pill's new state. Neutral is stored as an absent key rather than a
	 * `'neutral'` value — `parseCategoryRules` reads it back that way, and a
	 * stored neutral would be a second spelling of the same fact.
	 */
	function setCategoryRule(category: string, state: CategoryRuleState | undefined) {
		const next = { ...categoryRules };
		if (state === undefined) delete next[category];
		else next[category] = state;
		categoryRulesPref.value = serializeCategoryRules(next);
	}

	/** An item's new state; `undefined` drops the rule. One rule per id. */
	function setItemRule(item: { id: string; name: string }, state: CategoryRuleState | undefined) {
		const next = itemRules.filter((rule) => rule.id !== item.id);
		if (state !== undefined) next.push({ id: item.id, name: item.name, state });
		itemRulesPref.value = serializeItemRules(next);
	}

	/** One gate knob, as typed. Validation is `parseGate`'s, on read. */
	function setGate(knob: keyof Gates, value: string) {
		gatePrefs[knob].value = value;
	}

	/**
	 * Every knob back to its default. What that is spelled as is `resetGateInputs`'
	 * business, not this function's: the strings it hands back go straight into
	 * the preferences, and the reason they are blanks rather than numbers lives
	 * beside the parser that reads them. That lands five gates OFF and
	 * `minItemPrice` back at its shipped floor (POE-196) — the shipped state, not
	 * a blank one — which makes this also the only way BACK to the trash-price
	 * floor for a reader who typed 0 into it.
	 */
	function resetGates() {
		const inputs = resetGateInputs();
		for (const knob of Object.keys(gatePrefs) as (keyof Gates)[]) {
			gatePrefs[knob].value = inputs[knob];
		}
	}

	/**
	 * Clear resets the persisted setup that hides rows — the two rule layers and
	 * the two investment bounds — and nothing else. Sort, mode, horizon and
	 * density change what the reader is looking at, not how much of it, so
	 * clearing a filter that emptied the table must not also throw away the way
	 * they had it set up. The search is left alone too, for the opposite reason:
	 * it is not part of the setup, it is visibly in its own box, and that box has
	 * its own × for the reader who wants it gone.
	 *
	 * The GATES are left alone for a third reason (POE-191): five of the six ship
	 * off, so a gate that is running is one the reader deliberately armed — a
	 * standing choice rather than the question Clear is here to undo. The sixth,
	 * `minItemPrice`, is the shipped trash-price floor and is not the reader's
	 * question either. The Gates row has its own Defaults, which empties the boxes
	 * and is the way back to the shipped state; sweeping them up here would make
	 * Clear a second control undoing it.
	 */
	function clearFilters() {
		categoryRulesPref.value = serializeCategoryRules({});
		itemRulesPref.value = serializeItemRules([]);
		investMinPref.value = '';
		investMaxPref.value = '';
	}

	// ------------------------------------------------------------- the fetch --

	/**
	 * Which `load()` call owns the state. Two fetches are routinely in flight at
	 * once — a cold start fires the defaults and then the restored picks a tick
	 * later, and a Mercure refetch can be overtaken by a mode switch — and the
	 * requests are not guaranteed to answer in the order they were sent. Without
	 * the counter the slower one wins and the table shows one mode's plays under
	 * the other mode's button. Same pattern as `LabPage.svelte`'s
	 * `bestPlaysGeneration`.
	 *
	 * A plain `let`, not `$state`: nothing renders it, and making it reactive
	 * would re-run the effects that call `load()`.
	 */
	let loadGeneration = 0;

	async function load() {
		const generation = ++loadGeneration;
		loading = true;
		try {
			const response = await fetchCurrencyExchangePlays(
				parseMode(mode.value),
				parseHorizon(horizon.value)
			);
			if (generation !== loadGeneration) return;
			result = response;
			lastFetchedAt = new Date();
			lastError = null;
		} catch (e: any) {
			if (generation !== loadGeneration) return;
			// Keep `result`: the header downgrades to `stale` and the table stays.
			lastError = String(e?.message ?? e);
		} finally {
			// A superseded fetch leaves `loading` to the call that overtook it.
			if (generation === loadGeneration) loading = false;
		}
	}

	// Fetch on mount and on every mode or horizon change — one effect for all
	// three, because both picks are parameters of the same request.
	$effect(() => {
		// The explicit reads are the dependencies: reactivity must not rest on
		// `load()` happening to read them before its first await.
		void mode.value;
		void horizon.value;
		load();
	});

	// Re-derive the relative strings on a wall clock the data does not move.
	$effect(() => {
		const timer = setInterval(() => {
			now = new Date();
		}, NOW_TICK_MS);
		return () => clearInterval(timer);
	});

	/**
	 * Mercure fan-out from LabPage. The refetch is debounced AND jittered
	 * (`refetchDelay`) because every client receives the same publish within
	 * milliseconds — see the constants' comment in `$lib/exchange/view`.
	 */
	let refetchTimer: ReturnType<typeof setTimeout> | null = null;

	$effect(() => {
		let cancelled = false;
		const promise = listen('currency-exchange-updated', () => {
			if (cancelled) return;
			if (refetchTimer) clearTimeout(refetchTimer);
			refetchTimer = setTimeout(() => {
				refetchTimer = null;
				load();
			}, refetchDelay());
		});
		return () => {
			cancelled = true;
			if (refetchTimer) {
				clearTimeout(refetchTimer);
				refetchTimer = null;
			}
			// Expected to reject outside a Tauri context (the browser dev server).
			promise.then((unlisten) => unlisten()).catch(() => {});
		};
	});
</script>

{#snippet warning()}
	<svg
		class="warn-icon"
		width="11"
		height="11"
		viewBox="0 0 12 12"
		fill="none"
		stroke="currentColor"
		stroke-width="1.3"
		stroke-linecap="round"
		stroke-linejoin="round"
		aria-hidden="true"
	>
		<path d="M6 1.2 L11.2 10.6 H0.8 Z M6 4.6 V7.4 M6 8.9 V9.1" />
	</svg>
{/snippet}

<div class="exchange-page" aria-busy={loading}>
	<div class="page-head">
		<h1>Currency Exchange</h1>
		{#if result}
			<span class="league">{result.league}</span>
		{/if}
		<div class="spacer"></div>

		<span class="control-label">Sort</span>
		<SegmentedButtons
			value={parseSort(sortPref.value)}
			options={SORT_OPTIONS}
			onselect={(v) => (sortPref.value = parseSort(v))}
			title="Rank by the simulated outcome the server ranks on, by the hour's best-case chaos the ROI column shows — which orders the number each row prints, one posting of the market it enters on, so a market that posts a thousand at a time ranks above one that posts a single item at ten times the price — or by how long the market needs to absorb the play's worthwhile scale, shortest wait first."
		/>

		<div class="divider"></div>

		<SegmentedButtons
			value={parseMode(mode.value)}
			options={MODE_OPTIONS}
			onselect={(v) => (mode.value = parseMode(v))}
			title="Which plays to show: every one, single-swap only, or two-swap only."
		/>

		<div class="divider"></div>

		<SegmentedButtons
			value={parseHorizon(horizon.value)}
			options={HORIZON_OPTIONS}
			onselect={(v) => (horizon.value = parseHorizon(v))}
			title="How many hours of history ranked the list. The Hours column counts against this window."
		/>

		<div class="divider"></div>

		<span class="control-label">Density</span>
		<SegmentedButtons
			value={density}
			options={DENSITY_OPTIONS}
			onselect={(v) => (densityPref.value = parseDensity(v))}
			title="Dense drops the sub-lines and shrinks the icons; their content is in the column tooltips."
		/>
	</div>

	{#if result}
		<ExchangeFilterBar
			categories={result.categories}
			{categoryRules}
			{itemRules}
			{items}
			{gateInputs}
			{gates}
			investMin={investMinPref.value}
			investMax={investMaxPref.value}
			{unit}
			divineChaosRate={result.divineChaosRate}
			{search}
			{counts}
			{apiBase}
			oncategoryrule={setCategoryRule}
			onitemrule={setItemRule}
			ongate={setGate}
			ongatedefaults={resetGates}
			oninvestmin={(v) => (investMinPref.value = v)}
			oninvestmax={(v) => (investMaxPref.value = v)}
			onunit={(v) => (unitPref.value = v)}
			onsearch={(v) => (search = v)}
			onclear={clearFilters}
		/>
	{/if}

	<div class="status-line">
		{#if viewState.kind === 'loading'}
			Loading…
		{:else if viewState.kind === 'warming'}
			Waiting for the first Currency Exchange hour…
		{:else if viewState.kind === 'ready' || viewState.kind === 'stale'}
			<!-- The badge and the best-case caveat describe the rows, and the rows
			     keep rendering in the stale state — which is exactly when the
			     prices are oldest, so the two must not disappear with the refetch. -->
			{#if viewState.kind === 'stale'}
				<span class="warn">stale since {viewState.staleSince} — server unreachable</span>
				<span class="dot">·</span>
			{/if}
			{#if age}
				<Tooltip text={EXCHANGE_TOOLTIPS['Data age']} position="below">
					<span class="age">{age.label}{age.ago === '' ? '' : ` (${age.ago})`}</span>
				</Tooltip>
				<span class="dot">·</span>
			{/if}
			<span>window: {hoursWindow} hours</span>
			<span class="dot">·</span>
			<span>
				prices are the newest hour’s cheapest buy and dearest sell, so the ROI columns are a best
				case, not a quote — Exp. ROI is what the play would have paid across the last day, and
				every money figure on a row counts one posting of the market you enter on, whatever currency
				that market prices in, and one item where it posted no quantity pair at all — the Scale
				column shows the worthwhile run wherever there is one and a dash where there is not — and the route is
				priced at that best case throughout except its Get end, which is the spend plus Exp. ROI,
				so the last step’s total and Get differ by the gap between those two columns; both
				identities are in chaos, and a divine-entry route prints them at the divine rate
			</span>
		{:else}
			<span class="warn">Couldn't reach the server</span>
		{/if}
	</div>

	{#if rows.length > 0}
		<div class="table-wrap">
			<table class:dense>
				<thead>
					<tr>
						<th class="col-rank num">#</th>
						<th class="col-mode">
							<Tooltip text={EXCHANGE_TOOLTIPS.Mode}>Mode</Tooltip>
						</th>
						<th class="col-route">
							<!-- The label spans mirror `ExchangeRoute`'s slot geometry exactly
							     (see the contract in that file's header comment): change a
							     width there and the labels here drift off the tiles. The
							     convert label and its arrow gap are behind the SAME
							     `showConvert` the routes below read, so the collapsed form
							     stays mirrored too. -->
							<Tooltip text={EXCHANGE_TOOLTIPS.Route}>
								<div class="route-head" class:dense>
									<span class="slot-spend">Spend</span>
									<span class="gap"></span>
									<span class="slot-buy">{dense ? 'Buy' : 'Step 1 — buy'}</span>
									<span class="gap"></span>
									<span class="slot-sell">{dense ? 'Sell' : 'Step 2 — sell'}</span>
									{#if showConvert}
										<span class="gap"></span>
										<span class="slot-convert">{dense ? 'Convert' : 'Step 3 — convert'}</span>
									{/if}
									<span class="gap"></span>
									<span class="slot-get">Get</span>
								</div>
							</Tooltip>
						</th>
						<th class="col-money num">
							<Tooltip text={EXCHANGE_TOOLTIPS.Investment}>Investment</Tooltip>
						</th>
						<th class="col-money num">
							<Tooltip text={EXCHANGE_TOOLTIPS.ROI}>ROI</Tooltip>
						</th>
						<th class="col-expected num">
							<Tooltip text={EXCHANGE_TOOLTIPS['Exp. ROI']}>Exp. ROI</Tooltip>
						</th>
						<th class="col-pct num">
							<Tooltip text={EXCHANGE_TOOLTIPS['ROI%']}>ROI%</Tooltip>
						</th>
						<th class="col-trend num reserved">
							<Tooltip text={EXCHANGE_TOOLTIPS.Trend}>Trend</Tooltip>
						</th>
						<th class="col-depth num">
							<Tooltip text={EXCHANGE_TOOLTIPS.Depth}>Depth</Tooltip>
						</th>
						<th class="col-scale num">
							<Tooltip text={EXCHANGE_TOOLTIPS.Scale}>Scale</Tooltip>
						</th>
						<th class="col-hours num">
							<Tooltip text={EXCHANGE_TOOLTIPS.Hours}>Hours</Tooltip>
						</th>
					</tr>
				</thead>
				<tbody>
					{#each rows as play, i (play.key)}
						{@const progress = hoursProgress(play.hoursSeen, hoursWindow)}
						{@const scale = worthwhileScale(play)}
						{@const money = moneyColumns(play)}
						<!-- The thin-hour reading is the play's, not one step's — the ROUND
						     TRIP returned no spread worth taking in the newest hour — but a
						     row-wide ring turned solid red below some rank, where nearly
						     every play is flagged. `ExchangeRoute` now marks the traded
						     item's own tiles (buy + sell) instead, the same red-border
						     language the golden suspect mark uses on a doubtful leg. Nothing
						     on the row itself carries the flag any more, so there is no class
						     to bind here; the row is never hidden, dimmed or reordered for
						     it — Exp. ROI still measures the whole day, and the ranking is
						     untouched. -->
						<tr>
							<td class="num mono rank">{i + 1}</td>

							<td>
								<span class="pill" class:hop={play.mode === '1-hop'}>{play.mode}</span>
							</td>

							<td class="route-cell">
								<ExchangeRoute
									{play}
									{density}
									{apiBase}
									{showConvert}
									divineChaosRate={result?.divineChaosRate ?? 0}
								/>
							</td>

							<!-- ONE SCALE PER ROW. All three money columns come from
							     `moneyColumns`, which is `displayScale` — one posting of the
							     buy market, whatever currency it prices in (owner rulings,
							     2026-08-22) — and the route slots read the same decision, so
							     Spend/Get/keep and these three are never about different
							     trips. No "each" sub-line: what one exchange costs is not a
							     second reading the row owes, and the Scale column says how
							     many exchanges the run is. -->
							<td class="num">
								<div class="mono value">{formatChaos(money.investment)}c</div>
							</td>

							<td class="num">
								<div class="mono gain" class:flat={money.roi <= 0}>
									{formatGain(money.roi)}c
								</div>
							</td>

							<!-- What the row is measured to pay at the size it displays — the
							     same chaos the Get slot's "keep ≈" line carries, off the same
							     `moneyColumns` call — and the only money cell on the row that
							     can print a MINUS. It is one POSTING's gain on every row; the
							     RUN's gain is what the Scale column's "→ +Xc" prints instead,
							     and the two are deliberately different numbers. A run only
							     exists for a positive expectation, so the minus is always a row
							     with no run: the simulation is free to measure a loss
							     and the server serves it anyway (ADR-016), so red is a reading
							     here and not an error state. The ranking is still the
							     PER-EXCHANGE expectation this scales, which is why the Exp. ROI
							     sort keeps the served order rather than re-reading the column.
							     NOTHING measured is not a wash. A recipe with no simulable entry
							     hour carries a 0 mean over 0 entries, and printing that as "0c"
							     would tell the reader the play was replayed and broke even. It
							     takes the Scale column's dash instead — the same "this row has
							     no answer to that question" the rest of the table spells that
							     way. Below that, low coverage DIMS the number rather than
							     replacing it: "measured over too few hours" is a caveat on a
							     real mean, not a different reading. The sub-line names the
							     caveat and the title carries it into dense, where every
							     sub-line is gone. -->
							<td class="num">
								{#if play.simEntries === 0}
									<span
										class="mono reserved"
										title="Not simulable: no hour of the last day could be replayed for this play, so there is no expectation to report."
									>—</span
									>
								{:else}
									<div
										class="mono gain"
										class:flat={money.expectedRoi === 0}
										class:loss={money.expectedRoi < 0}
										class:thin={play.lowCoverage}
										title={play.lowCoverage
											? `Low coverage: only ${play.simEntries} of the last day's hours could be simulated, so this mean is thin.`
											: null}
									>
										{formatGain(money.expectedRoi)}c
									</div>
									{#if !dense}
										<div class="mono sub">
											n={play.simEntries}{#if play.lowCoverage}<span class="amber"> · low</span>{/if}
										</div>
									{/if}
								{/if}
							</td>

							<td class="num">
								<div class="cell-line">
									{#if play.suspect}
										<Tooltip text={EXCHANGE_TOOLTIPS.Suspect}>{@render warning()}</Tooltip>
									{/if}
									<span class="cap">Net</span>
									<span class="mono value">{formatRoiPct(play.roiPct)}</span>
								</div>
								{#if !dense}
									<div class="cell-line">
										<span class="cap">Raw</span>
										<span class="mono sub">{formatRoiPct(play.roiPctRaw)}</span>
									</div>
								{/if}
							</td>

							<td class="num">
								<span class="mono reserved">—</span>
							</td>

							<!-- Raw in both densities. Absorption is the Scale column's
							     sub-line now: the reader types no size for a depth to be
							     "over", and the honest reading of a thin market is how many
							     hours the worthwhile run needs, not a warning triangle.
							     The thin-hour label lands HERE and not by the mode chip: the
							     flag is a liquidity reading about the hour these prices came
							     from, and this is the liquidity column — a play marked low
							     liquidity is usually one whose depth figure is already tiny,
							     and the two belong under one another. Comfortable has the
							     sub-line for free (every other cell on the row carries one,
							     so the height is already paid for); dense drops every
							     sub-line in the table, so there the buy/sell tile borders in
							     `ExchangeRoute` carry the mark and the same copy rides on
							     this cell's title. -->
							<td
								class="num"
								title={dense && play.lowLiquidity ? EXCHANGE_TOOLTIPS['Low liquidity'] : null}
							>
								<span class="mono value">{formatVolume(play.depth)}/h</span>
								{#if play.lowLiquidity && !dense}
									<div class="sub">
										<Tooltip text={EXCHANGE_TOOLTIPS['Low liquidity']}>
											<span class="low-liq">low liquidity</span>
										</Tooltip>
									</div>
								{/if}
							</td>

							<!-- What the play is worth repeating to, and what that costs in
							     bankroll and in waiting. Dense keeps only the ×N → gain —
							     the run's cost and wait are traded away with every other
							     sub-line, so the Fastest sort ranks by a number dense does
							     not print; comfortable is where the wait is read.

							     THIS CELL IS ALWAYS THE RUN, and since the owner rulings of
							     2026-08-22 it is the ONLY cell on the row that ever is. The
							     cells above count one posting of the buy market on every row,
							     divine entries included, so this cell is the run's whole home:
							     the ×N, what it ties up — the exact figure the filter bar's
							     Run-cost bound compares against, through `runInvestment` — and
							     how long the market needs
							     (`docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` §1, SCALE, and §5).
							     Reaching for `play.expectedRoi` or `play.investment` here
							     would print a per-exchange figure under a run-sized ×N, which
							     is the bug that document exists to close. -->
							<td class="num">
								{#if scale === null}
									<span class="mono reserved">—</span>
								{:else}
									<div class="mono value">×{formatChaos(scale.flips)} → {formatGain(scale.gain)}c</div>
									{#if !dense}
										<div class="sub">
											{formatChaos(scale.investment)}c in
											{#if scale.hours === null}
												<span class="reserved">· — h</span>
											{:else if scale.hours <= 1}
												<span class="fast">· &le;1 h</span>
											{:else}
												<span class="amber">· ~{scale.hours} h</span>
											{/if}
										</div>
									{/if}
								{/if}
							</td>

							<td class="num">
								<div class="mono value">{play.hoursSeen} / {hoursWindow}</div>
								{#if !dense}
									<div class="bar-row">
										<span class="bar">
											<span
												class="bar-fill"
												class:full={progress === 1}
												style="width: {progress * 100}%"
											></span>
										</span>
									</div>
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<!-- THE LEGEND — one strip, every mark the table draws.
		     It replaces the single suspect footnote this block used to hold rather
		     than sitting beside it: two homes for "what the gold border means"
		     would have drifted the moment either was edited. Each entry is a
		     SAMPLE of the real mark (the same `.pill`, `.gain`, `.thin`,
		     `.reserved` classes the rows use, so a colour changed in one place
		     changes both) plus two to four words, with the explanation on hover
		     from `EXCHANGE_TOOLTIPS` — the same source the column headers read, so
		     the legend cannot contradict a column about its own mark.
		     Both densities: the strip shrinks a step in dense but keeps every
		     entry, because dense is where the marks carry MORE of the meaning —
		     every sub-line that would have spelled it out is gone. -->
		<div class="legend" class:dense>
			<span class="legend-label">Marks</span>

			<Tooltip text={EXCHANGE_TOOLTIPS.Mode}>
				<span class="key"><span class="pill hop">1-hop</span>three trades</span>
			</Tooltip>

			<!-- Both homes of the one fact: the golden tile on the doubtful STEP and
			     the triangle on the row's ROI%. The footnote this legend replaced
			     taught only the triangle, and a reader who met the tile first had
			     nothing on screen to name it. -->
			<Tooltip text={EXCHANGE_TOOLTIPS.Suspect}>
				<span class="key">
					<span class="sw sw-suspect"></span>{@render warning()}suspect price
				</span>
			</Tooltip>

			<Tooltip text={EXCHANGE_TOOLTIPS['Low liquidity']}>
				<span class="key"><span class="sw sw-low-liq"></span>low liquidity</span>
			</Tooltip>

			<!-- Only while the column is on screen: with every rendered play direct,
			     the convert slot is gone and a legend entry for its dashed tile would
			     explain a mark the reader cannot find. -->
			{#if showConvert}
				<Tooltip text={EXCHANGE_TOOLTIPS['Step not used']}>
					<span class="key"><span class="sw sw-empty"></span>step not used</span>
				</Tooltip>
			{/if}

			<Tooltip text={EXCHANGE_TOOLTIPS['Expected gain']}>
				<span class="key"><span class="mono gain">+102c</span>expected gain</span>
			</Tooltip>

			<Tooltip text={EXCHANGE_TOOLTIPS['Measured loss']}>
				<span class="key"><span class="mono gain loss">&minus;4c</span>measured loss</span>
			</Tooltip>

			<Tooltip text={EXCHANGE_TOOLTIPS['Low coverage']}>
				<span class="key">
					<span class="mono gain thin">4c</span><span class="amber">· low</span>thin coverage
				</span>
			</Tooltip>

			<Tooltip text={EXCHANGE_TOOLTIPS['No reading']}>
				<span class="key"><span class="mono reserved">&mdash;</span>no reading</span>
			</Tooltip>

			<Tooltip text={EXCHANGE_TOOLTIPS['ROI%']}>
				<span class="key">
					<span class="cap">Net</span><span class="cap">Raw</span>after / before undercut
				</span>
			</Tooltip>

			<Tooltip text={EXCHANGE_TOOLTIPS.Scale}>
				<span class="key"><span class="mono fast">&le;1 h</span>absorbed this hour</span>
			</Tooltip>

			<Tooltip text={EXCHANGE_TOOLTIPS['Full window']}>
				<span class="key">
					<span class="bar"><span class="bar-fill full" style="width: 100%"></span></span>held
					every hour
				</span>
			</Tooltip>
		</div>
	{:else if viewState.kind === 'ready'}
		<!-- The gates are named separately here for the reason the counter splits
		     them: they hide rows on a bar the reader may never have opened, so
		     "your filters" alone would point at controls that are all visibly
		     off. `gatesEmptyMessage` splits that branch again, because since
		     POE-196 the gate that emptied the table may be one the reader never
		     set. -->

		<div class="empty">
			{allPlays.length > 0
				? search.trim() !== ''
					? 'No plays match your search, gates and filters right now.'
					: counts.hiddenByGates > 0 && counts.hiddenByFilters === 0
						? gatesEmptyMessage
						: counts.hiddenByFilters > 0 && counts.hiddenByGates === 0
							? 'No plays pass your filters right now.'
							: 'No plays pass your gates and filters right now.'
				: 'No plays ranked for this mode and horizon.'}
		</div>
	{/if}
</div>

<style>
	.exchange-page {
		display: flex;
		flex-direction: column;
		gap: 10px;
		height: 100%;
		color: var(--color-lab-text);
	}

	.page-head {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 10px 16px;
	}

	.page-head h1 {
		font-size: 1rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}

	.league {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
	}

	.spacer {
		flex: 1;
	}

	.control-label {
		font-size: 0.625rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-lab-text-secondary);
		white-space: nowrap;
	}

	.divider {
		width: 1px;
		height: 20px;
		background: var(--color-lab-border);
		margin: 0 2px;
	}

	.status-line {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		padding: 0 2px;
	}

	/* The one thing on this line in primary text: how stale the whole table is
	   is the fact a reader has to take with them, not a footnote. */
	.status-line .age {
		color: var(--color-lab-text);
		font-weight: 600;
	}

	.status-line .warn {
		color: var(--color-lab-yellow);
	}

	.dot {
		opacity: 0.5;
	}

	.table-wrap {
		flex: 1;
		overflow-y: auto;
		/* The route's five fixed slots outrun a narrow window by design — the
		   geometry is what makes a column readable down the page. */
		overflow-x: auto;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8125rem;
	}

	thead {
		position: sticky;
		top: 0;
		z-index: 1;
		background: var(--color-lab-surface);
	}

	th {
		text-align: left;
		padding: 8px;
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
		border-bottom: 1px solid var(--color-lab-border);
		font-weight: 600;
		vertical-align: bottom;
		white-space: nowrap;
	}

	table.dense th {
		padding: 5px 8px;
		font-size: 0.625rem;
	}

	/* A reserved column reads as reserved before it is hovered — Trend carries no
	   number yet, and a full-weight header would promise one. */
	th.reserved {
		color: #6b7280;
	}

	.col-rank {
		width: 40px;
	}
	.col-mode {
		width: 74px;
	}
	.col-money {
		width: 136px;
	}
	/* Wider than the ROI column beside it: the same chaos figure can carry a
	   minus, and it sits over an "n=24 · low" sub-line the money columns have
	   nothing like. */
	.col-expected {
		width: 148px;
	}
	.col-pct {
		width: 132px;
	}
	.col-trend {
		width: 62px;
	}
	.col-depth {
		width: 96px;
	}
	/* Wider than the Fill column it replaced: the cell carries "×34 → +102c" over
	   "1,360c in · ~3 h", not a bare duration. The width Depth gives up is the
	   width it no longer needs — its amber sub-line went with the quantity. */
	.col-scale {
		width: 168px;
	}
	.col-hours {
		width: 92px;
	}

	table.dense .col-mode {
		width: 68px;
	}
	table.dense .col-money {
		width: 116px;
	}
	table.dense .col-expected {
		width: 116px;
	}
	table.dense .col-pct {
		width: 112px;
	}
	table.dense .col-trend {
		width: 58px;
	}
	table.dense .col-depth {
		width: 82px;
	}
	table.dense .col-scale {
		width: 120px;
	}
	table.dense .col-hours {
		width: 62px;
	}

	/* The Route header is the one tooltip trigger wrapping a BLOCK: `Tooltip`'s
	   own wrapper is an inline span, and the slot geometry below only lines up
	   with the tiles when the flex row is the full width of the cell. */
	.col-route :global(.tooltip-wrap) {
		display: block;
	}

	/* Mirrors `ExchangeRoute`'s slot widths, arrows and gap. Both densities:
	   comfortable 120 / 208 / 176 / 176 / 168 with 22px arrow gaps and a 7px
	   gap, dense 80 / 220 / 140 / 140 / 80 with 18px arrow gaps and a 6px gap;
	   collapsed, comfortable 120 / 208 / 176 / 168 and dense 80 / 220 / 140 / 80,
	   which is the same set less the convert slot and the arrow gap before it.
	   The numbers are DERIVED there, off the longest string each slot can hold —
	   change one there, copy it here, and never the other way round. */
	.route-head {
		display: flex;
		align-items: center;
		gap: 7px;
	}

	.route-head.dense {
		gap: 6px;
	}

	.route-head span {
		flex-shrink: 0;
	}

	.route-head .gap {
		width: 22px;
	}
	/* Spend is narrower than Get, and deliberately: only Get carries the profit
	   line the comfortable end geometry is sized around. */
	.route-head .slot-spend {
		width: 120px;
	}
	.route-head .slot-get {
		width: 168px;
	}
	/* The step slots are the ones that moved when the lines became `≈` step
	   totals: 196 → 208 and 164 → 176. At their worst case it is the RATE that
	   binds them rather than the item name above it, which is the derivation
	   `ExchangeRoute`'s CSS carries. The ends did not move. */
	.route-head .slot-buy {
		width: 208px;
	}
	.route-head .slot-sell,
	.route-head .slot-convert {
		width: 176px;
	}

	.route-head.dense .gap {
		width: 18px;
	}
	/* Dense drops the unit word from both ends, so both hold a bare number and
	   take the one width again. */
	.route-head.dense .slot-spend,
	.route-head.dense .slot-get {
		width: 80px;
	}
	.route-head.dense .slot-buy {
		width: 220px;
	}
	.route-head.dense .slot-sell,
	.route-head.dense .slot-convert {
		width: 140px;
	}

	td {
		padding: 8px;
		color: var(--color-lab-text);
		border-bottom: 1px solid var(--color-lab-border);
		vertical-align: middle;
	}

	table.dense td {
		padding: 4px 8px;
	}

	th.num,
	td.num {
		text-align: right;
		white-space: nowrap;
	}

	.mono {
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 600;
	}

	td .value {
		font-size: 0.8125rem;
		line-height: 1.25;
	}

	table.dense td .value {
		font-size: 0.75rem;
	}

	td .sub {
		font-size: 0.6875rem;
		color: #6b7280;
		line-height: 1.25;
		white-space: nowrap;
	}

	.rank {
		color: var(--color-lab-text-secondary);
	}

	/* Trend holds its slot with a dash rather than an empty cell, and so does a
	   Scale whose flip count or whose wait cannot be read: an empty cell reads as
	   a value that went missing, a dash as a question this row has no answer to. */
	.reserved {
		color: #4b5563;
	}

	/* A run the market absorbs inside the hour is the only one the ROI was
	   computed against a book that will still be there — the colour is that
	   verdict, not a speed. */
	.fast {
		color: var(--color-lab-green);
	}

	.gain {
		font-size: 0.875rem;
		font-weight: 700;
		color: var(--color-lab-green);
		line-height: 1.25;
	}

	table.dense .gain {
		font-size: 0.75rem;
	}

	/* A round trip that returns exactly what it cost is not a gain, and green
	   would put it in the same class as the plays that are. */
	.gain.flat {
		color: #6b7280;
	}

	/* Exp. ROI's own case, and the only one on this table: a measured LOSS.
	   Grey would read as "no result" next to the flat rows, which is the one
	   thing this number is not — the simulation ran and it came out negative.
	   The palette's red (tokens.css), same one the app spends on RISKY. */
	.gain.loss {
		color: var(--color-lab-red);
	}

	/* Too few simulated hours to trust the mean. Dimmed rather than recoloured,
	   so the sign still reads: the verdict on the number is unchanged, the
	   confidence in it is what dropped. Matches how a suspect play stays in the
	   table with a marker instead of being hidden. */
	.thin {
		opacity: 0.55;
	}

	.cell-line {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 5px;
		line-height: 1.3;
	}

	.cap {
		font-size: 0.5625rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: #6b7280;
	}

	.amber {
		color: var(--color-lab-yellow);
	}

	.warn-icon {
		color: var(--color-lab-yellow);
		flex-shrink: 0;
	}

	.bar-row {
		display: flex;
		justify-content: flex-end;
		margin-top: 3px;
	}

	.bar {
		display: block;
		width: 56px;
		height: 4px;
		background: var(--color-lab-border);
		border-radius: 2px;
		overflow: hidden;
	}

	.bar-fill {
		display: block;
		height: 4px;
		background: #4b5563;
	}

	/* Only a play that held every hour of the window gets the green bar — the
	   colour is the persistence verdict, not a progress indicator. */
	.bar-fill.full {
		background: var(--color-lab-green);
	}

	tr:hover {
		background: rgba(255, 255, 255, 0.02);
	}

	/* The play's newest hour printed no exploitable spread. The mark used to be a
	   red inset ring around the whole row (box-shadows, so the route cell's fixed
	   slot geometry never shifted on a marked row) but below some rank nearly
	   every row was flagged, and the viewport turned solid red frames. The mark
	   now lives on the traded item's tiles in `ExchangeRoute` — the same red-
	   border language the golden suspect mark already uses — so there is nothing
	   to draw at the row level here.

	   The words, under the depth figure. Muted rather than full-strength red: the
	   tile border is what catches the eye and the label is what names it, so a
	   second loud red would only compete with the row's own numbers. */
	.low-liq {
		color: var(--color-lab-red);
		opacity: 0.8;
	}

	.pill {
		display: inline-block;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		border-radius: 3px;
		border: 1px solid currentcolor;
		padding: 0 5px;
		white-space: nowrap;
		color: var(--color-lab-text-secondary);
	}

	table.dense .pill {
		font-size: 0.5625rem;
		padding: 0 4px;
	}

	/* 1-hop is three trades against two markets — a different risk from market
	   making, so it is told apart at a glance rather than by reading the word. */
	.pill.hop {
		color: var(--color-lab-purple);
	}

	/* ---------------------------------------------------------- the legend -- */

	.legend {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		column-gap: 14px;
		row-gap: 5px;
		padding: 0 2px;
		font-size: 0.6875rem;
		color: #6b7280;
	}

	.legend.dense {
		column-gap: 11px;
		font-size: 0.625rem;
	}

	.legend-label {
		font-size: 0.5625rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: #4b5563;
	}

	.key {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		white-space: nowrap;
	}

	/* Every sample below is the row's OWN class, sized down — the legend must not
	   hold a second definition of a colour the table already owns, or the two
	   drift and the strip starts lying about the marks it names. Only the
	   dimensions are local. */
	.legend .gain {
		font-size: 0.6875rem;
	}

	.legend .bar {
		width: 22px;
	}

	.sw {
		display: inline-block;
		width: 13px;
		height: 13px;
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		background: var(--color-lab-bg);
		flex-shrink: 0;
	}

	/* The route tile's own mark. */
	.sw-suspect {
		border-color: var(--color-lab-yellow);
	}

	/* The route tile's own mark, same shape as the golden one above it now that
	   both live on a step tile rather than one being a row outline. */
	.sw-low-liq {
		border-color: var(--color-lab-red);
	}

	.sw-empty {
		background: transparent;
		border-style: dashed;
		/* The empty tile's own dash colour, from `ExchangeRoute`. */
		border-color: #23262f;
	}

	.empty {
		text-align: center;
		padding: 40px;
		color: var(--color-lab-text-secondary);
	}
</style>
