<script lang="ts">
	import ItemIcon from '$lib/components/ItemIcon.svelte'
	import { groupNumber } from '$lib/stores/number-format.svelte'
	import type { BossProfile, LootItem } from './contracts'
	import type { buildResultsView, ResultsSort } from './page-view'

	let { profile, view, sort, onSort, iconFor = () => null }: {
		profile: Pick<BossProfile, 'groups' | 'extra' | 'categories'>
		view: ReturnType<typeof buildResultsView>
		sort: ResultsSort
		onSort: (sort: ResultsSort) => void
		iconFor?: (item: LootItem) => string | null
	} = $props()
	const pools = $derived(view.groups.slice(0, profile.groups.length))
	const additional = $derived(view.groups.slice(profile.groups.length))
</script>

{#snippet resultsTable(groups: typeof view.groups)}
	<table>
		<colgroup><col /><col class="drops-col" /><col class="rate-col" /><col class="wiki-col" /></colgroup>
		<thead><tr>
			<th aria-sort={sort === 'name' ? 'ascending' : 'none'}>
				<button type="button" class="sort" class:active={sort === 'name'} onclick={() => onSort('name')}>Item {sort === 'name' ? '↑' : ''}</button>
			</th>
			<th class="right">Drops</th>
			<th aria-sort={sort === 'rate' ? 'descending' : 'none'}>
				<button type="button" class="sort" class:active={sort === 'rate'} onclick={() => onSort('rate')}>Rate {sort === 'rate' ? '↓' : ''}</button>
				<span class="rate-header">{view.rateHeader}</span>
			</th>
			<th class="right">Wiki</th>
		</tr></thead>
		{#each groups as group (group.id)}
			<tbody>
				<tr class="group"><th colspan="4"><span>{group.title}</span><span class="group-note">{group.note}</span></th></tr>
				{#each group.items as item (item.id)}
					<tr>
						<td><span class="item"><ItemIcon src={iconFor(item)} alt="" size={26} /><span>{item.name}<small>{item.base}</small></span></span></td>
						<td class="right mono">{item.drops === null ? '—' : groupNumber(String(item.drops))}</td>
						<td>
							{#if item.disabled}<span class="not-counted">not counted</span>
							{:else}<div class="bar" title={item.provenance}><i style:width="{item.bar}%"></i><b>{item.rate}</b></div>
								{#if view.mixed}<small class="provenance">{item.provenance}</small>{/if}
							{/if}
						</td>
						<td class="right wiki">{item.wikiRate}</td>
					</tr>
				{/each}
			</tbody>
		{/each}
	</table>
{/snippet}

<!-- Both alternatives share one row/header renderer; CSS also hides the inactive controls from focus and accessibility. -->
<div class="results-container">
	<div class="results-stacked">{@render resultsTable(view.groups)}</div>
	<div class="results-columns">
		{@render resultsTable(pools)}
		{@render resultsTable(additional)}
	</div>
</div>

<style>
	.results-container { container: boss-results / inline-size; }
	.results-columns { display: none; }
	table { width: 100%; table-layout: fixed; border-collapse: collapse; }
	.drops-col { width: 74px; }
	.rate-col { width: 180px; }
	.wiki-col { width: 73px; }
	th { font-size: .6875rem; text-transform: uppercase; letter-spacing: .5px; color: var(--color-lab-text-secondary); font-weight: 600; text-align: left; }
	td, th { border-top: 1px solid var(--color-lab-border); padding: 5px; }
	td:first-child, th:first-child { padding-left: 12px; }
	td:last-child, th:last-child { padding-right: 12px; }
	thead th { padding-top: 6px; padding-bottom: 6px; }
	.right { text-align: right; }
	.mono { font-family: Consolas, Monaco, monospace; font-weight: 600; }
	.item { display: flex; align-items: center; gap: 8px; min-width: 0; }
	small { display: block; font-size: 11px; color: var(--color-lab-text-secondary); }
	.bar { position: relative; height: 14px; background: rgba(255,255,255,.04); border-radius: 2px; display: flex; align-items: center; }
	.bar i { position: absolute; inset: 0 auto 0 0; background: rgba(59,130,246,.35); border-radius: 2px; }
	.bar b { position: relative; padding-left: 6px; font-family: Consolas, Monaco, monospace; font-size: 12px; font-weight: 600; }
	.group th { padding: 8px 12px 4px; }
	.group-note { float: right; font-size: 11px; text-transform: none; letter-spacing: 0; font-weight: 400; }
	.sort { all: unset; cursor: pointer; display: inline-flex; gap: 4px; border-radius: 3px; padding: 1px 2px; }
	.sort:hover, .sort.active { color: var(--color-lab-text); }
	.sort:focus-visible { outline: 1px solid var(--color-lab-blue); outline-offset: 2px; }
	.rate-header { text-transform: none; letter-spacing: 0; font-weight: 400; }
	.wiki { color: var(--color-lab-text-secondary); }
	.not-counted { display: inline-block; border-radius: 10px; padding: 1px 6px; font-size: 11px; color: var(--color-lab-yellow); background: rgba(234,179,8,.15); }
	@container boss-results (min-width: 600px) {
		.results-stacked { display: none; }
		.results-columns { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; align-items: start; }
		.results-columns .drops-col { width: 16%; }
		.results-columns .rate-col { width: 25%; }
		.results-columns .wiki-col { width: 19%; }
		.results-columns td:last-child, .results-columns th:last-child { padding-right: 8px; }
		.results-columns .wiki { white-space: nowrap; }
		.results-columns .item > span { flex: 1; min-width: min-content; }
		.results-columns .item :global(img), .results-columns .item :global(.item-icon-fallback) { flex-shrink: 0; }
		.results-columns .group-note { float: none; display: block; margin-top: 2px; }
		.results-columns .rate-header { display: block; }
	}
	@container boss-results (600px <= width < 900px) {
		.results-columns .drops-col { width: 14%; }
		.results-columns .rate-col { width: 23%; }
		.results-columns .wiki-col { width: 17%; }
		.results-columns td, .results-columns th { padding-left: 3px; padding-right: 3px; }
		.results-columns td:first-child, .results-columns th:first-child { padding-left: 6px; }
		.results-columns td:last-child, .results-columns th:last-child { padding-right: 6px; }
		.results-columns thead th { vertical-align: top; letter-spacing: 0; }
		.results-columns .item { gap: 4px; }
	}
</style>
