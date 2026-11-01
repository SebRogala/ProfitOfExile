<script lang="ts">
	import { onMount, untrack } from 'svelte'
	import Button from '$lib/components/Button.svelte'
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte'
	import ItemIcon from '$lib/components/ItemIcon.svelte'
	import { persisted } from '$lib/prefs.svelte'
	import { groupNumber } from '$lib/stores/number-format.svelte'
	import { createBossLootPageController, type PagePorts } from '$lib/boss-loot/page-controller.svelte'
	import type { LootItem } from '$lib/boss-loot/contracts'

	let { ports, iconFor = () => null }: { ports: PagePorts; iconFor?: (item: LootItem) => string | null } = $props()
	const tab = persisted('bossLootTab', 'results')
	const scope = persisted('bossLootScope', 'own')
	const sort = persisted('bossLootSort', 'rate')
	const page = untrack(() => createBossLootPageController(ports, { scope, sort }))
	const view = $derived(page.results)
	onMount(() => { untrack(() => { void page.load() }) })
	$effect(() => {
		const selected = page.scope
		untrack(() => { void page.selectScope(selected) })
	})
</script>

<div class="boss-loot-page">
	<header class="page-head">
		<h1>Boss loot</h1>
		<span class="caption">Drop rates per boss from logged kills.</span>
		<span class="spacer"></span>
		<SegmentedButtons value={page.scope} options={[{ value: 'own', label: 'Own' }, { value: 'collective', label: 'Collective' }]}
			title="Whose data" onselect={(value) => page.selectScope(value === 'collective' ? 'collective' : 'own')} />
		<Button disabled={!page.profile || page.exporting} onclick={() => page.exportCsv()}>Export CSV</Button>
	</header>
	<div class="page-picks">
		<div class="boss-pills" role="group" aria-label="Boss">
			{#if page.profile}
				<button type="button" class="pill" aria-pressed="true">{page.profile.name}
					{#if view}<span class="mono">{groupNumber(view.runs.toString())}</span>{/if}
				</button>
			{/if}
		</div>
		<SegmentedButtons value={tab.value} options={[{ value: 'results', label: 'Results' }, { value: 'runs', label: 'Runs' }, { value: 'settings', label: 'Settings' }]}
			title="Section" onselect={(value) => { tab.value = value }} />
	</div>
	{#if page.status === 'loading'}
		<div class="status-line" role="status">Loading…</div>
	{:else if page.status === 'unreachable'}
		<div class="status-line warning" role="status">Server unreachable.</div>
	{/if}
	{#if page.error}<div class="status-line warning" role="alert">{page.error}</div>{/if}
	{#if tab.value === 'results' && view && page.profile}
		<section class="panel" aria-label="Boss loot results">
			<div class="boss-summary">
				<b>{page.profile.fullName ?? page.profile.name}</b>
				<span>Zone <span class="zone">{page.profile.area}</span> · level {page.profile.level}</span>
				<span>{groupNumber(view.runs.toString())} runs · {view.deviceNote}</span>
			</div>
			{#if view.message}<div class="status-line">{view.message}</div>{/if}
			<table>
				<colgroup><col /><col class="drops-col" /><col class="rate-col" /><col class="wiki-col" /></colgroup>
				<thead><tr>
					<th aria-sort={page.sort === 'name' ? 'ascending' : 'none'}>
						<button type="button" class="sort" class:active={page.sort === 'name'} onclick={() => page.selectSort('name')}>Item {page.sort === 'name' ? '↑' : ''}</button>
					</th>
					<th class="right">Drops</th>
					<th aria-sort={page.sort === 'rate' ? 'descending' : 'none'}>
						<button type="button" class="sort" class:active={page.sort === 'rate'} onclick={() => page.selectSort('rate')}>Rate {page.sort === 'rate' ? '↓' : ''}</button>
						<span class="rate-header">{view.rateHeader}</span>
					</th>
					<th class="right">Wiki</th>
				</tr></thead>
				{#each view.groups as group (group.id)}
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
		</section>
	{/if}
</div>

<style>
	.boss-loot-page { display: flex; flex-direction: column; gap: 10px; font-size: .8125rem; color: var(--color-lab-text); }
	.page-head { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; padding: 10px 16px; background: var(--color-lab-surface); border: 1px solid var(--color-lab-border); }
	.page-head h1 { margin: 0; font-size: 1rem; font-weight: 700; }
	.caption, .boss-summary > span { color: var(--color-lab-text-secondary); font-size: 12px; }
	.spacer { flex: 1; }
	.page-picks, .boss-pills { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
	.page-picks { justify-content: space-between; gap: 8px; }
	.pill { display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 14px; border: 1px solid rgba(99,102,241,.5); background: rgba(99,102,241,.2); color: var(--color-lab-text); font: inherit; font-size: 12px; cursor: pointer; }
	.panel { background: var(--color-lab-surface); border: 1px solid var(--color-lab-border); }
	.boss-summary { display: flex; flex-wrap: wrap; align-items: baseline; gap: 16px; padding: 10px 12px; }
	.zone { font-family: Consolas, Monaco, monospace; }
	.mono { font-family: Consolas, Monaco, monospace; font-weight: 600; }
	table { width: 100%; table-layout: fixed; border-collapse: collapse; }
	.drops-col { width: 74px; }
	.rate-col { width: 180px; }
	.wiki-col { width: 78px; }
	th { font-size: .6875rem; text-transform: uppercase; letter-spacing: .5px; color: var(--color-lab-text-secondary); font-weight: 600; text-align: left; }
	td, th { border-top: 1px solid var(--color-lab-border); padding: 5px; }
	td:first-child, th:first-child { padding-left: 12px; }
	td:last-child, th:last-child { padding-right: 12px; }
	thead th { padding-top: 6px; padding-bottom: 6px; }
	.right { text-align: right; }
	.item { display: flex; align-items: center; gap: 8px; min-width: 0; }
	small { display: block; font-size: 11px; color: var(--color-lab-text-secondary); }
	.bar { position: relative; height: 14px; background: rgba(255,255,255,.04); border-radius: 2px; display: flex; align-items: center; }
	.bar i { position: absolute; inset: 0 auto 0 0; background: rgba(59,130,246,.35); border-radius: 2px; }
	.bar b { position: relative; padding-left: 6px; font-family: Consolas, Monaco, monospace; font-size: 12px; font-weight: 600; }
	.group th { padding: 8px 12px 4px; }
	.group-note { float: right; font-size: 11px; text-transform: none; letter-spacing: 0; font-weight: 400; }
	.sort { all: unset; cursor: pointer; display: inline-flex; gap: 4px; border-radius: 3px; padding: 1px 2px; }
	.sort:hover, .sort.active { color: var(--color-lab-text); }
	.sort:focus-visible, .pill:focus-visible { outline: 1px solid var(--color-lab-blue); outline-offset: 2px; }
	.rate-header { text-transform: none; letter-spacing: 0; font-weight: 400; }
	.wiki { color: var(--color-lab-text-secondary); }
	.not-counted { display: inline-block; border-radius: 10px; padding: 1px 6px; font-size: 11px; color: var(--color-lab-yellow); background: rgba(234,179,8,.15); }
	.status-line { padding: 4px 12px; color: var(--color-lab-text-secondary); font-size: 12px; }
	.warning { color: var(--color-lab-yellow); }
</style>
