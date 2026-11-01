<script lang="ts">
	import { onMount, untrack } from 'svelte'
	import Button from '$lib/components/Button.svelte'
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte'
	import ItemIcon from '$lib/components/ItemIcon.svelte'
	import Toggle from '$lib/components/Toggle.svelte'
	import ShortcutCapture from '$lib/boss-loot/ShortcutCapture.svelte'
	import { persisted } from '$lib/prefs.svelte'
	import { groupNumber } from '$lib/stores/number-format.svelte'
	import { createBossLootPageController, type PagePorts } from '$lib/boss-loot/page-controller.svelte'
	import type { LootItem } from '$lib/boss-loot/contracts'
	import { ssot } from '$lib/stores/ssot.svelte'

	let { ports, iconFor = () => null }: { ports: PagePorts; iconFor?: (item: LootItem) => string | null } = $props()
	const tab = persisted('bossLootTab', 'results')
	const scope = persisted('bossLootScope', 'own')
	const sort = persisted('bossLootSort', 'rate')
	const page = untrack(() => createBossLootPageController(ports, { scope, sort }))
	const view = $derived(page.results)
	const history = $derived(page.runsView)
	const editor = $derived(page.editorView)
	const behavior = $derived(page.settingsView)
	onMount(() => { untrack(() => { void page.load() }) })
	$effect(() => {
		const clock = ssot.bossLoot?.nowMs
		if (clock !== undefined) untrack(() => { void page.reconcileNative(ssot.bossLoot!) })
	})
	$effect(() => {
		const selected = page.scope
		untrack(() => { void page.selectScope(selected) })
	})
	$effect(() => {
		const section = tab.value
		const profile = page.profile?.boss
		if (section === 'runs' && profile) untrack(() => { void page.loadRuns(1) })
	})
</script>

<div class="boss-loot-page">
	<header class="page-head">
		<h1>Boss loot</h1>
		<span class="caption">Drop rates per boss from logged kills.</span>
		<span class="spacer"></span>
		<SegmentedButtons value={page.scope} options={[{ value: 'own', label: 'Own' }, { value: 'collective', label: 'Collective' }]}
			title="Whose data" onselect={(value) => { scope.value = value === 'collective' ? 'collective' : 'own' }} />
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
	{#if tab.value === 'results' && page.status === 'loading'}
		<div class="status-line" role="status">Loading…</div>
	{:else if tab.value === 'results' && page.status === 'unreachable'}
		<div class="status-line warning" role="status">Server unreachable.</div>
	{/if}
	{#if page.error && (tab.value !== 'settings' || page.error !== page.settingsError)}<div class="status-line warning" role="alert">{page.error}</div>{/if}
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
	{#if tab.value === 'runs'}
		{#if page.runsStatus === 'loading'}
			<div class="status-line" role="status">Loading…</div>
		{:else if page.runsStatus === 'unreachable'}
			<div class="status-line warning" role="status">Server unreachable.</div>
		{/if}
		{#if page.runsError}<div class="status-line warning" role="alert">{page.runsError}</div>{/if}
		{#if page.runMutationError}<div class="status-line warning" role="alert">{page.runMutationError}</div>{/if}
		{#if history}
			<section class="panel" aria-label="Boss loot runs">
				<div role="table" aria-label="Runs">
					<div class="runrow run-head" role="row">
						<span role="columnheader">Run</span><span role="columnheader">Date</span><span role="columnheader">Unique</span><span role="columnheader">Additional</span><span role="columnheader" aria-label="Actions"></span>
					</div>
					{#each history.rows as row (row.id)}
						<div role="rowgroup">
							<div class="runrow" role="row" data-run={row.run}>
								<span class="mono" role="cell">{groupNumber(String(row.run))}</span>
								<span class="run-date" role="cell">{row.date}</span>
								<span class="chips" role="cell">
									{#each row.unique as item (item.id)}<span class="item"><ItemIcon src={iconFor(item)} alt="" size={22} />{item.name}</span>{/each}
								</span>
								<span class="chips" role="cell">
									{#each row.extra as item (item.id)}<span class="history-chip"><ItemIcon src={iconFor(item)} alt="" size={18} />{item.name}{#if item.dropQuantity > 1} ×{groupNumber(String(item.dropQuantity))}{/if}</span>
									{:else}<span class="secondary">—</span>{/each}
								</span>
								<span class="run-actions" role="cell">
									<Button disabled={page.runMutationPending} onclick={() => page.beginEdit(row.id)}>Edit</Button>
									<Button disabled={page.runMutationPending} title="Delete run {row.run}" onclick={() => { if (confirm(`Delete run ${row.run}?`)) void page.deleteRun(row.id) }}>Delete</Button>
								</span>
							</div>
							{#if editor?.id === row.id}
								<div role="row"><div role="cell" aria-colspan="5" class="run-editor" aria-label="Edit run {row.run}">
									{#each editor.groups as group (group.id)}
										<span class="table-heading">{group.label} pool · {group.rule}</span>
										<div class="chips">
											{#each group.items as item (item.id)}
												<button type="button" class="edit-chip" class:selected={item.selected} disabled={page.runMutationPending} aria-pressed={item.selected} onclick={() => page.toggleEditorItem(item.id)}><ItemIcon src={iconFor(item)} alt="" size={18} />{item.name}</button>
											{/each}
										</div>
									{/each}
									<span class="table-heading">Additional drops</span>
									<div class="chips">
										{#each editor.extra as item (item.id)}
											<button type="button" class="edit-chip" class:selected={item.selected} disabled={page.runMutationPending} aria-pressed={item.selected} onclick={() => page.toggleEditorItem(item.id)}><ItemIcon src={iconFor(item)} alt="" size={18} />{item.name}{#if item.dropQuantity > 1} ×{groupNumber(String(item.dropQuantity))}{/if}</button>
										{/each}
									</div>
									{#if !editor.valid}<span class="warning">Check guaranteed groups before saving.</span>{/if}
									<div class="editor-actions">
										<Button variant="save" disabled={page.runMutationPending || !editor.valid} onclick={() => page.saveEditor()}>Save run {row.run}</Button>
										<Button disabled={page.runMutationPending} onclick={() => page.cancelEdit()}>Cancel</Button>
									</div>
								</div></div>
							{/if}
						</div>
					{/each}
				</div>
				<nav class="pager" aria-label="Runs pages">
					<span class="secondary">{#if history.rows.length}Runs <span class="mono">{groupNumber(String(history.rows[0].run))}–{groupNumber(String(history.rows[history.rows.length - 1].run))}</span> of <span class="mono">{groupNumber(String(history.total))}</span>, newest first{:else}{history.range}{/if}</span>
					<div class="page-buttons">
						<button type="button" class="page-button" aria-label="Previous page" disabled={history.first || page.runMutationPending} onclick={() => page.loadRuns(history.page - 1)}>‹</button>
						{#each history.pages as number (number)}<button type="button" class="page-button" class:current={number === history.page} aria-current={number === history.page ? 'page' : undefined} disabled={page.runMutationPending} onclick={() => page.loadRuns(number)}>{number}</button>{/each}
						<button type="button" class="page-button" aria-label="Next page" disabled={history.last || page.runMutationPending} onclick={() => page.loadRuns(history.page + 1)}>›</button>
					</div>
				</nav>
			</section>
		{/if}
	{/if}
	{#if tab.value === 'settings'}
		{#if page.settingsStatus === 'loading'}<div class="status-line" role="status">Loading…</div>
		{:else if page.settingsStatus === 'unreachable'}<div class="status-line warning" role="status">Settings unavailable.</div>{/if}
		{#if page.settingsError}<div class="status-line warning" role="alert">{page.settingsError}</div>{/if}
		{#if behavior && page.settings}
			<section class="panel" aria-label="Boss loot settings">
				<div class="setting-row">
					<span class="setting-label"><span>Boss loot module</span><small>Capture shortcut and overlay</small></span>
					<fieldset class="setting-control" disabled={page.modulePending}><Toggle checked={page.enabled} label="Boss loot module" onchange={(enabled) => { void page.setEnabled(enabled) }} /></fieldset>
				</div>
				<div class="setting-row">
					<span class="setting-label"><span>Auto-save after</span><small>Any click on the overlay restarts it</small></span>
					<fieldset class="setting-control" disabled={page.settingsPending}>
						<SegmentedButtons value={String(page.settings.autoSaveSeconds)} options={behavior.timeouts} title="Auto-save after" onselect={(value) => { void page.setAutoSave(Number(value)) }} />
					</fieldset>
				</div>
				<div class="setting-row">
					<span class="setting-label"><span>Capture</span><small>Only while Path of Exile is in front; the click still reaches the game</small></span>
					<ShortcutCapture value={page.settings.captureShortcut} label="Capture" listening={page.listening === 'capture'} disabled={page.settingsPending}
						onlisten={() => page.listenShortcut('capture')} onkey={(event) => { void page.captureKey(event) }} onmouse={(event) => { void page.captureMouse(event) }} />
				</div>
				<div class="setting-row">
					<span class="setting-label"><span>Preview frame</span><small>Shows the capture area around the cursor</small></span>
					<ShortcutCapture value={page.settings.previewShortcut} label="Preview frame" keycaps listening={page.listening === 'preview'} disabled={page.settingsPending}
						onlisten={() => page.listenShortcut('preview')} onkey={(event) => { void page.captureKey(event) }} onmouse={(event) => { void page.captureMouse(event) }} />
				</div>
				<div class="setting-row exclusions">
					<span class="setting-label"><span>Not counted</span><small>Items your loot filter hides. Their rate leaves out your runs.</small></span>
					<fieldset class="setting-control not-counted-list" disabled={page.settingsPending}>
						{#each behavior.notCounted as item (item.id)}
							<label><input type="checkbox" checked={item.checked} onchange={(event) => {
								const checked = event.currentTarget.checked
								event.currentTarget.checked = item.checked
								void page.setNotCounted(item.id, checked)
							}} /><ItemIcon src={iconFor(item)} alt="" size={20} />{item.name}</label>
						{/each}
					</fieldset>
				</div>
				<div class="setting-row">
					<span>Where runs go</span>
					{#if page.deviceId !== null}<span class="storage-note">To the ProfitOfExile server, with this device's id <span class="device-id">{page.deviceId}</span>. Nothing is kept on this PC; a run made while the server is down is not saved.</span>
					{:else}<span class="storage-note">Device identity unavailable.</span>{/if}
				</div>
				{#if page.deviceError}<div class="status-line warning" role="alert">{page.deviceError}</div>{/if}
			</section>
		{/if}
	{/if}
</div>

<style>
	.boss-loot-page { display: flex; flex-direction: column; gap: 10px; font-size: .8125rem; line-height: normal; color: var(--color-lab-text); }
	.page-head { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; padding: 10px 16px; background: var(--color-lab-surface); border: 1px solid var(--color-lab-border); }
	.page-head h1 { margin: 0; font-size: 1rem; font-weight: 700; }
	.page-head :global(.btn) { height: 28px; padding: 0 12px; border-color: var(--color-lab-border); color: var(--color-lab-text); }
	.page-head :global(.btn:hover:not(:disabled)) { border-color: var(--color-lab-blue); color: var(--color-lab-text); }
	.page-head :global(.btn:focus-visible) { outline: 1px solid var(--color-lab-blue); outline-offset: 2px; }
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
	.wiki-col { width: 73px; }
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
	.runrow { display: grid; grid-template-columns: 44px 64px 200px minmax(0,1fr) 112px; align-items: center; gap: 10px; padding: 6px 12px; border-top: 1px solid var(--color-lab-border); }
	.run-head { border-top: 0; }
	.run-head, .table-heading { font-size: .6875rem; text-transform: uppercase; letter-spacing: .5px; color: var(--color-lab-text-secondary); font-weight: 600; }
	.run-date, .secondary { color: var(--color-lab-text-secondary); font-size: 12px; }
	.chips { display: flex; gap: 4px; align-items: center; flex-wrap: wrap; }
	.history-chip { display: inline-flex; align-items: center; gap: 4px; min-height: 20px; padding: 1px 6px 1px 2px; border-radius: 4px; background: rgba(255,255,255,.05); font-size: 11px; white-space: nowrap; }
	.run-actions { display: flex; gap: 4px; justify-content: flex-end; }
	.run-actions :global(.btn) { height: 24px; padding: 0 8px; border-color: var(--color-lab-border); color: var(--color-lab-text); }
	.run-editor { padding: 8px 12px 12px 74px; display: flex; flex-direction: column; gap: 8px; background: rgba(59,130,246,.05); }
	.edit-chip { display: inline-flex; align-items: center; gap: 4px; min-height: 24px; padding: 2px 6px; border: 1px solid var(--color-lab-border); border-radius: 4px; background: transparent; color: var(--color-lab-text); font: inherit; font-size: 11px; cursor: pointer; }
	.edit-chip.selected { border-color: var(--color-lab-blue); background: rgba(59,130,246,.14); }
	.edit-chip:not(.selected) :global(img) { opacity: .45; }
	.editor-actions { display: flex; gap: 6px; }
	.editor-actions :global(.btn) { height: 28px; padding: 0 12px; border-color: var(--color-lab-text-muted); color: var(--color-lab-text); }
	.editor-actions :global(.btn.save) { border-color: var(--color-lab-blue); background: var(--color-lab-blue); color: var(--color-lab-bg); font-weight: 600; }
	.editor-actions :global(.btn.save:hover:not(:disabled)) { border-color: var(--color-lab-blue); background: var(--color-lab-blue); color: var(--color-lab-bg); }
	.run-actions :global(.btn:hover:not(:disabled)), .editor-actions :global(.btn:not(.save):hover:not(:disabled)) { border-color: var(--color-lab-blue); color: var(--color-lab-text); }
	.run-actions :global(.btn:focus-visible), .editor-actions :global(.btn:focus-visible), .edit-chip:focus-visible, .page-button:focus-visible { outline: 1px solid var(--color-lab-blue); outline-offset: 2px; }
	.pager { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px; padding: 8px 12px; border-top: 1px solid var(--color-lab-border); }
	.page-buttons { display: flex; gap: 4px; align-items: center; }
	.pager .mono { color: var(--color-lab-text); }
	.page-button { min-width: 28px; height: 26px; padding: 0 8px; border-radius: 4px; border: 1px solid var(--color-lab-border); background: transparent; color: var(--color-lab-text-secondary); font: inherit; font-size: 12px; cursor: pointer; }
	.page-button:hover { color: var(--color-lab-text); }
	.page-button.current { background: rgba(99,102,241,.2); color: var(--color-lab-text); border-color: rgba(99,102,241,.5); }
	.page-button:disabled, .edit-chip:disabled { opacity: .35; cursor: default; }
	.setting-row { display: grid; grid-template-columns: 200px minmax(0,1fr); gap: 16px; align-items: center; padding: 10px 16px; border-top: 1px solid var(--color-lab-border); }
	.setting-row:first-child { border-top: 0; }
	.setting-row > :last-child { justify-self: start; }
	.setting-label { display: flex; flex-direction: column; gap: 2px; }
	.setting-control { border: 0; margin: 0; padding: 0; min-width: 0; }
	.setting-control:disabled { opacity: .6; }
	.setting-control :global(.toggle:focus-visible), .not-counted-list input:focus-visible { outline: 1px solid var(--color-lab-blue); outline-offset: 2px; }
	.exclusions { align-items: start; }
	.not-counted-list { display: grid; grid-template-columns: repeat(2,minmax(0,1fr)); gap: 6px 16px; }
	.not-counted-list label { display: inline-flex; align-items: center; gap: 8px; min-height: 20px; font-size: 12px; cursor: pointer; }
	.not-counted-list input { width: 14px; height: 14px; accent-color: var(--color-lab-blue); flex-shrink: 0; }
	.storage-note { font-size: 12px; color: var(--color-lab-text-secondary); }
	.device-id { font-family: Consolas, Monaco, monospace; color: var(--color-lab-text); }
</style>
