<script lang="ts">
	/**
	 * The 25 x 3 room value table, read-only or editable (POE-259).
	 *
	 * ONE component for both presets, because they are the same table: Default
	 * shows what the formula produced and Custom shows the same cells with an
	 * input over each. Two components would be two layouts to keep in step, and
	 * the whole point of the picker is that the player is looking at the same
	 * board either way.
	 *
	 * It decides no value. Rust prices every cell (`temple_value_table`,
	 * ADR-022 §1) and `$lib/temple/values` says what a cell's letter means and
	 * whether what was typed into it is a chaos amount; this file lays them out
	 * and reports the accepted edits.
	 *
	 * **The refusal is per cell and stays on the cell.** Rust refuses a table
	 * with one line for all 75 of them, which is no use to somebody who has
	 * just mistyped one. So a value this build would not store never leaves the
	 * component: the cell is marked, the reason sits under it, and `onedit`
	 * does not fire.
	 */
	import {
		VALUE_MARK_LABEL,
		formatChaos,
		parseCell,
		type ValueRow
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

<!-- The box scrolls (25 rows do not fit), and a scroll container that cannot
     take focus cannot be scrolled from the keyboard at all. Under Default it
     holds nothing focusable, so without this a keyboard-only user can see the
     first eight rows and no more. Same trade as `compass/LabGraph.svelte`. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="value-table" role="region" aria-label="Room values" tabindex="0">
	<table>
		<thead>
			<tr>
				<th class="room">Room line</th>
				<th class="grade">Grade</th>
				<th>Tier 1</th>
				<th>Tier 2</th>
				<th>Tier 3</th>
			</tr>
		</thead>
		<tbody>
			{#each rows as row (row.key)}
				<tr>
					<td class="room">{row.name}</td>
					<td class="grade">{row.grade}</td>
					{#each row.cells as cell (cell.tier)}
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

<style>
	/* 25 rows is more than fits beside the rest of the settings card, so the
	   table scrolls inside its own box and the header stays put. */
	.value-table {
		max-height: 320px;
		overflow: auto;
		width: 100%;
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

	.empty {
		padding: 0.5rem;
		font-size: 0.7rem;
		color: var(--color-lab-text-muted);
	}
</style>
