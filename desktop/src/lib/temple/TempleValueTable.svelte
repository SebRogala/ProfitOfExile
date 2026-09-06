<script lang="ts">
	/**
	 * The 25 x 3 room value table, read-only or editable (POE-259), sortable and
	 * collapsible (POE-263).
	 *
	 * ONE component for both presets, because they are the same table: Default
	 * shows what the formula produced and Custom shows the same cells with an
	 * input over each. Two components would be two layouts to keep in step, and
	 * the whole point of the picker is that the player is looking at the same
	 * board either way.
	 *
	 * It decides no value. Rust prices every cell (`temple_value_table`,
	 * ADR-022 §1) and `$lib/temple/values` says what a cell's letter means,
	 * whether what was typed into it is a chaos amount, and in what ORDER the
	 * rows read; this file lays them out and reports the accepted edits.
	 * Sorting reorders and never re-prices — the value shown is the value
	 * ranked (epic lock L4).
	 *
	 * **The refusal is per cell and stays on the cell.** Rust refuses a table
	 * with one line for all 75 of them, which is no use to somebody who has
	 * just mistyped one. So a value this build would not store never leaves the
	 * component: the cell is marked, the reason sits under it, and `onedit`
	 * does not fire. The marks are keyed `line:tier` and therefore survive a
	 * re-sort — a mark keyed on a row's POSITION would slide onto a different
	 * room the moment a header was clicked.
	 */
	import { persisted } from '$lib/prefs.svelte';
	import {
		DEFAULT_TIERS_MODE,
		DEFAULT_VALUE_SORT,
		VALUE_MARK_LABEL,
		formatChaos,
		hiddenOverrideTitle,
		nextValueSort,
		orderRows,
		parseCell,
		parseTiersMode,
		parseValueSort,
		serializeValueSort,
		sortValueRows,
		type ValueRow,
		type ValueSortKey
	} from './values';

	let {
		rows,
		editable = false,
		onedit
	}: {
		rows: ValueRow[];
		/** Custom's cells take input; Default's are text. */
		editable?: boolean;
		/** An ACCEPTED edit: a chaos amount, or null for "use the formula for
		 *  this tier". Never fires for a refused one. */
		onedit?: (key: string, tier: number, chaos: number | null) => void;
	} = $props();

	/** The two view picks, persisted per ADR-013. Both are the reader's, not the
	 *  valuation's: neither changes a number, so neither belongs in the typed
	 *  `temple_custom` settings block beside the rates. */
	const sortPref = persisted('templeValueTableSort', serializeValueSort(DEFAULT_VALUE_SORT));
	const tiersPref = persisted('templeValueTableTiers', DEFAULT_TIERS_MODE);

	const sort = $derived(parseValueSort(sortPref.value));
	const tiersMode = $derived(parseTiersMode(tiersPref.value));
	const showAllTiers = $derived(tiersMode === 'all');

	/** Why each refused cell was refused, keyed `line:tier`. Local because the
	 *  refusal is: nothing left the component, so nothing else can know. */
	let refused = $state<Record<string, string>>({});
	/** Which mode the marks in `refused` were made under; null until the effect
	 *  below has run once, which is before any of them can exist. */
	let markedUnder = $state<boolean | null>(null);

	/**
	 * A refusal belongs to the table it was typed into, and dies with it.
	 *
	 * Default's cells are text, so a mark made in Custom has nothing left to
	 * point at; and coming BACK to Custom it would sit red over a value the
	 * player never re-typed, blaming a cell that is fine.
	 *
	 * Keyed on `editable` and deliberately NOT on `rows`: the SSOT snapshot is
	 * whole-replaced every three seconds, so the row array is a fresh identity
	 * on every poll and a rows-keyed reset would wipe the mark within three
	 * seconds of the player making it — while they are still reading it.
	 */
	$effect(() => {
		if (markedUnder === editable) return;
		markedUnder = editable;
		refused = {};
	});

	/** The row order on screen, as keys. Held rather than re-derived on every
	 *  poll — see the effect below. */
	let orderKeys = $state<string[]>([]);
	/** Which sort `orderKeys` was made under. Plain, not `$state`: it is only
	 *  ever read inside the effect that writes it. */
	let orderedUnder = '';
	/**
	 * The cell a Custom input currently holds focus in, or null.
	 *
	 * Plain and deliberately NOT `$state`: the effect below must not re-run
	 * when focus moves, or clicking into a cell would itself re-sort the table.
	 */
	let editingCell: string | null = null;

	/**
	 * A focused cell that just left the screen cannot fire its own `onblur`.
	 *
	 * Toggling `editable` removes every input (Default has none) and toggling
	 * `showAllTiers` removes tiers 1 and 2's, so a cell held focused when either
	 * flips is gone without ever blurring — and a stale `editingCell` is what
	 * the re-sort effect above reads to decide whether to hold the order still.
	 * Left uncleared, the table would freeze on the last sort forever after a
	 * layout toggle taken mid-edit. Deliberately its OWN effect and not folded
	 * into the `markedUnder` one above: that one clears refusal marks and must
	 * stay keyed on `editable` alone, not on the tiers toggle too.
	 */
	$effect(() => {
		void editable;
		void showAllTiers;
		editingCell = null;
	});

	/**
	 * The order the table reads in, recomputed on a sort change or on new rows.
	 *
	 * **The one thing it will not do is re-sort under a typing player.** An
	 * edit changes a total, and the SSOT poll hands over a fresh array every
	 * three seconds, so a table that re-sorted on every arrival would slide the
	 * row out from under the cursor — the number moves as it is typed. While a
	 * cell has focus the held key order is kept and only the CELLS refresh
	 * (`orderRows`); the order catches up on the next arrival after focus
	 * leaves, which is the one that carries the accepted edit. Clicking a
	 * header still re-sorts immediately: that is the player asking for it.
	 */
	$effect(() => {
		const under = serializeValueSort(sort);
		const fresh = rows;
		if (editingCell !== null && under === orderedUnder) return;
		orderedUnder = under;
		orderKeys = sortValueRows(fresh, sort.key, sort.direction).map((row) => row.key);
	});

	/** Before the effect has run once there is no held order yet, so the first
	 *  paint sorts directly rather than flashing the arrival order. */
	const ordered = $derived(
		orderKeys.length === 0
			? sortValueRows(rows, sort.key, sort.direction)
			: orderRows(rows, orderKeys)
	);

	function pickSort(key: ValueSortKey): void {
		sortPref.value = serializeValueSort(nextValueSort(sort, key));
	}

	function ariaSort(key: ValueSortKey): 'ascending' | 'descending' | 'none' {
		if (sort.key !== key) return 'none';
		return sort.direction === 'asc' ? 'ascending' : 'descending';
	}

	/** The arrow beside a header. `↕` on an unsorted column so the header reads
	 *  as sortable before it is clicked; the direction is also on `aria-sort`,
	 *  so the glyph is never the only carrier. */
	function sortArrow(key: ValueSortKey): string {
		if (sort.key !== key) return '↕';
		return sort.direction === 'asc' ? '▲' : '▼';
	}

	/** The cells a row shows: all three, or tier 3 alone. */
	function shownCells(row: ValueRow) {
		return showAllTiers ? row.cells : row.cells.filter((cell) => cell.tier === 3);
	}

	function cellId(key: string, tier: number): string {
		return `${key}:${tier}`;
	}

	function edit(key: string, tier: number, raw: string): void {
		const parsed = parseCell(raw);
		const id = cellId(key, tier);
		if (parsed.kind === 'invalid') {
			refused = { ...refused, [id]: parsed.reason };
			return;
		}
		const { [id]: _dropped, ...rest } = refused;
		refused = rest;
		onedit?.(key, tier, parsed.kind === 'clear' ? null : parsed.chaos);
	}
</script>

<div class="value-block">
	<label class="tiers-toggle">
		<input
			type="checkbox"
			checked={showAllTiers}
			onchange={(e) => (tiersPref.value = e.currentTarget.checked ? 'all' : 'tier3')}
		/>
		<span>Tiers 1–2</span>
		<span class="hint">
			A tier-1 or tier-2 room is a fraction of its line's tier 3 unless you priced it by hand.
			Collapsed, a hand-priced one is marked on the tier-3 cell.
		</span>
	</label>

	<!-- The box scrolls (25 rows do not fit), and a scroll container that cannot
	     take focus cannot be scrolled from the keyboard at all. Under Default it
	     holds nothing focusable, so without this a keyboard-only user can see the
	     first eight rows and no more. Same trade as `compass/LabGraph.svelte`. -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div class="value-table" role="region" aria-label="Room values" tabindex="0">
		<table>
			<thead>
				<tr>
					<th class="room" aria-sort={ariaSort('room')}>
						<button type="button" class="sort" onclick={() => pickSort('room')}>
							<span>Room line</span><span class="arrow" aria-hidden="true">{sortArrow('room')}</span>
						</button>
					</th>
					<th class="grade" aria-sort={ariaSort('grade')}>
						<button type="button" class="sort" onclick={() => pickSort('grade')}>
							<span>Grade</span><span class="arrow" aria-hidden="true">{sortArrow('grade')}</span>
						</button>
					</th>
					{#if showAllTiers}
						<th>Tier 1</th>
						<th>Tier 2</th>
					{/if}
					<th aria-sort={ariaSort('tier3')}>
						<button type="button" class="sort" onclick={() => pickSort('tier3')}>
							<span>Tier 3</span><span class="arrow" aria-hidden="true">{sortArrow('tier3')}</span>
						</button>
					</th>
				</tr>
			</thead>
			<tbody>
				{#each ordered as row (row.key)}
					<tr>
						<td class="room">{row.name}</td>
						<td class="grade">{row.grade}</td>
						{#each shownCells(row) as cell (cell.tier)}
							<td class="cell">
								{#if editable}
									<input
										type="text"
										inputmode="decimal"
										class="cell-input"
										class:refused={refused[cellId(row.key, cell.tier)] !== undefined}
										value={cell.override ?? ''}
										placeholder={formatChaos(cell.total)}
										aria-label="{row.name} tier {cell.tier} chaos value"
										aria-invalid={refused[cellId(row.key, cell.tier)] !== undefined}
										title="Empty keeps the formula for this tier. Currently {formatChaos(
											cell.total
										)} c — {cell.title}"
										onfocus={() => (editingCell = cellId(row.key, cell.tier))}
										onblur={() => {
											if (editingCell === cellId(row.key, cell.tier)) editingCell = null;
										}}
										onchange={(e) => edit(row.key, cell.tier, e.currentTarget.value)}
									/>
									<!-- In the editable arm with the input it is about: Default has no
									     box a refused value could have been typed into, so a mark left
									     over from Custom would be red text against a number nobody can
									     edit. -->
									{#if refused[cellId(row.key, cell.tier)]}
										<span class="refusal">{refused[cellId(row.key, cell.tier)]}</span>
									{/if}
								{:else}
									<span class="chaos">{formatChaos(cell.total)}</span>
								{/if}
								<span
									class="mark"
									title="{VALUE_MARK_LABEL[cell.mark]} — {cell.title}"
									aria-label={VALUE_MARK_LABEL[cell.mark]}>{cell.mark}</span
								>
								<!-- Nothing stored is ever silent: with tiers 1 and 2 off screen,
								     a number the player typed into one of them is disclosed here
								     rather than being simply absent. Expanded, the numbers are on
								     screen and the mark would be noise. -->
								{#if cell.tier === 3 && !showAllTiers && hiddenOverrideTitle(row) !== null}
									<span
										class="hidden-override"
										title={hiddenOverrideTitle(row)}
										aria-label="priced by hand: {hiddenOverrideTitle(row)}">•</span
									>
								{/if}
							</td>
						{/each}
					</tr>
				{/each}
			</tbody>
		</table>
		{#if rows.length === 0}
			<p class="empty">No values yet — the table is read from Rust when the page opens.</p>
		{/if}
	</div>
</div>

<style>
	/* The toggle and the table, each as wide as it needs and no wider. */
	.value-block {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.3rem;
		max-width: 100%;
	}

	.tiers-toggle {
		display: flex;
		align-items: baseline;
		gap: 0.35rem;
		font-size: 0.72rem;
		color: var(--color-lab-text);
		cursor: pointer;
	}

	.tiers-toggle .hint {
		font-size: 0.7rem;
		color: var(--color-lab-text-muted);
	}

	/* 25 rows is more than fits beside the rest of the settings card, so the
	   table scrolls inside its own box and the header stays put. The box is
	   sized to its CONTENT (POE-263) rather than to the card: collapsed to
	   three columns it is half the width, and what the card gains is the space
	   the constant columns were spending. */
	.value-table {
		max-height: 320px;
		overflow: auto;
		width: max-content;
		max-width: 100%;
		border: 1px solid var(--color-lab-border);
		border-radius: 4px;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.72rem;
	}

	thead th {
		position: sticky;
		top: 0;
		z-index: 1;
		text-align: right;
		font-weight: 600;
		padding: 4px 6px;
		color: var(--color-lab-text-secondary);
		background: var(--color-lab-surface);
		border-bottom: 1px solid var(--color-lab-border);
	}

	th.room,
	th.grade {
		text-align: left;
	}

	/* A real button, so the sort is reachable by keyboard and announced as a
	   control; it inherits the header's own type so the row still reads as a
	   header rather than as a toolbar. */
	.sort {
		display: inline-flex;
		align-items: baseline;
		gap: 0.25rem;
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		color: inherit;
		cursor: pointer;
	}

	.sort:hover {
		color: var(--color-lab-text);
	}

	.sort .arrow {
		font-family: ui-monospace, monospace;
		color: var(--color-lab-text-muted);
	}

	td {
		padding: 2px 6px;
		border-bottom: 1px solid var(--color-lab-border);
	}

	td.room {
		color: var(--color-lab-text);
		white-space: nowrap;
	}

	td.grade {
		color: var(--color-lab-text-muted);
	}

	td.cell {
		text-align: right;
		white-space: nowrap;
	}

	.chaos {
		color: var(--color-lab-text);
	}

	.cell-input {
		width: 5rem;
		text-align: right;
		background: var(--color-lab-bg);
		color: var(--color-lab-text);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		padding: 1px 4px;
		font-family: inherit;
		font-size: inherit;
	}

	/* A refused cell is marked by its border AND by the reason printed beside
	   it — a colour alone is a mark somebody cannot see. */
	.cell-input.refused {
		border-color: var(--color-lab-red);
	}

	.refusal {
		margin-left: 0.3rem;
		color: var(--color-lab-red);
	}

	/* The provenance letter. Muted and monospaced so a column of them reads as
	   one thing rather than competing with the numbers. */
	.mark {
		margin-left: 0.3rem;
		font-family: ui-monospace, monospace;
		color: var(--color-lab-text-muted);
		cursor: help;
	}

	/* The hidden-tier override mark, in the same muted monospace as the
	   provenance letter beside it — it is the same kind of fact about the
	   cell, not a warning. */
	.hidden-override {
		margin-left: 0.2rem;
		font-family: ui-monospace, monospace;
		color: var(--color-lab-text-muted);
		cursor: help;
	}

	.empty {
		padding: 0.5rem;
		font-size: 0.7rem;
		color: var(--color-lab-text-muted);
	}
</style>
