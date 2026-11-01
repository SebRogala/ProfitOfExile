<script lang="ts">
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte'
	import Button from '$lib/components/Button.svelte'
	import LootItem from './LootItem.svelte'
	import { buildOverlayView, dispatchOverlayAction, type OverlayInput } from './overlay-view'

	let { input, iconFor, ontouch, onaction }: {
		input: OverlayInput
		iconFor: (id: string) => string | null
		ontouch: () => void
		onaction: (action: string, item?: string) => void
	} = $props()
	const view = $derived(buildOverlayView(input))

	/** WidgetHost and native DOM clicks share one reset/action entry. */
	export function handleAction(action: string, element: HTMLElement): void {
		dispatchOverlayAction(action, ontouch, (name) => {
			if (element instanceof HTMLButtonElement && element.disabled) return
			onaction(name, element.dataset.item)
		})
	}
	function click(event: MouseEvent): void {
		const element = (event.target as HTMLElement).closest<HTMLElement>('[data-action]')
		if (element) handleAction(element.dataset.action!, element)
	}
	function scopeActions(node: HTMLElement): void {
		// Shared SegmentedButtons owns its markup; metadata keeps its real buttons
		// reachable through WidgetHost's forwarded-click path.
		node.querySelectorAll('button').forEach((button, index) => {
			button.dataset.action = index === 0 ? 'loot.own' : 'loot.collective'
		})
	}
</script>

<!-- Buttons supply keyboard activation; their click bubbles to this single dispatcher. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_click_events_have_key_events -->
<section class="confirmation" class:failed={input.outcome?.state === 'unreachable'}
	class:list={input.layout === 'list'} style:width="{view.width}px"
	aria-label="Boss loot confirmation" data-hot data-action="loot.touch" onclick={click}>
	<header>
		<div class="identity"><div class="title">{input.profile.name} · run {input.capture.run} <span class="profile-key">{input.profile.key}</span></div>
			<div class="zone">Zone <span class="mono">{input.capture.attribution.area}</span>, level {input.capture.attribution.level}</div>
		</div>
		<div class="controls">
			{#if view.badge}<span class="badge {view.tone}" role="status">{#if input.outcome?.state === 'saving'}<i class="spinner" aria-hidden="true"></i>{/if}{view.badge}</span>{/if}
			<div use:scopeActions><SegmentedButtons value={input.scope} options={[{ value: 'own', label: 'Own' }, { value: 'collective', label: 'Collective' }]} onselect={() => {}} title="Whose data" /></div>
		</div>
	</header>
	<div class="countdown"><i style:width="{view.barPercent}%"></i></div>
	{#if view.mergeMessage}<div class="message merge"><span>{view.mergeMessage}</span><button type="button" class="link" data-action="loot.force-new">Save as new run</button></div>{/if}
	{#if view.message}<div class="message"><span>{view.message}</span>{#if input.outcome?.state === 'unreachable'}<span data-action="loot.close"><Button>Close</Button></span>{/if}</div>{/if}
	{#if view.excludedMessage}<div class="message warning">{view.excludedMessage}</div>{/if}
	<div class="runs-note">{view.runsNote}{#if view.mixed}<div>{view.rateHeader}</div>{/if}</div>
	{#each view.groups as group (group.id)}
		<div class="section">
			<div class="group"><span>Guaranteed · {group.label} · {group.rule}</span><span class="status {group.tone}">{group.status}</span></div>
			{#if group.ambiguity}<div class="ask"><span class="warning">?</span><span><b>{group.ambiguity.base}</b> was read. Several uniques share this base: which one dropped?</span></div>{/if}
			<div class="guaranteed-grid" class:rows={input.layout === 'list'} role={group.radio ? 'radiogroup' : 'group'} aria-label={group.label}>
				{#each group.items as item (item.id)}<LootItem {item} layout={input.layout} icon={iconFor(item.id)} locked={!view.open} />{/each}
			</div>
		</div>
	{/each}
	<div class="section">
		<div class="group"><span>Additional drops</span><span class="status">{view.extraMessage}</span></div>
		<div class="additional-grid" class:rows={input.layout === 'list'}>{#each view.extra as item (item.id)}<LootItem {item} layout={input.layout} icon={iconFor(item.id)} locked={!view.open} />{/each}</div>
	</div>
	<footer class="section">
		<span class="unknown">{view.unknownLine}</span>
		{#if view.open}<div class="buttons"><span data-action="loot.discard"><Button>Discard</Button></span><span class="save" data-action="loot.save"><Button disabled={!view.canSave}>Save now</Button></span></div>{/if}
	</footer>
</section>

<style>
	.confirmation { --loot-selected: rgba(59, 130, 246, .12); --loot-warning: rgba(234, 179, 8, .16); --loot-tag: rgba(255, 255, 255, .08); width: 400px; max-width: 100%; background: var(--color-lab-bg); border: 1px solid var(--color-lab-border); border-radius: 6px; color: var(--color-lab-text); font: 12px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; box-shadow: 0 6px 24px rgba(0, 0, 0, .55); }
	.failed { border-color: var(--color-lab-red); }
	header { display: flex; align-items: flex-start; justify-content: space-between; gap: 8px; padding: 10px 12px 8px; border-bottom: 1px solid var(--color-lab-border); }
	.identity { min-width: 0; }
	.title { font-size: 13px; font-weight: 700; }
	.profile-key, .badge { display: inline-flex; align-items: center; gap: 4px; padding: 0 6px; border-radius: 8px; font-size: 11px; line-height: 16px; }
	.profile-key { background: var(--loot-tag); color: var(--color-lab-text-secondary); }
	.zone { margin-top: 2px; color: var(--color-lab-text-secondary); font-size: 11px; overflow-wrap: anywhere; }
	.mono { font-family: 'Consolas', 'Monaco', monospace; }
	.controls { display: flex; flex-direction: column; align-items: flex-end; gap: 6px; flex-shrink: 0; }
	.badge { white-space: nowrap; background: rgba(59, 130, 246, .16); }
	.badge.warning { background: var(--loot-warning); }
	.badge.success { background: rgba(34, 197, 94, .14); }
	.badge.error { background: rgba(239, 68, 68, .22); color: var(--color-lab-text); }
	.warning { color: var(--color-lab-yellow); }
	.success { color: var(--color-lab-green); }
	.error { color: var(--color-lab-red); }
	.countdown { height: 3px; background: var(--color-lab-border); }
	.countdown i { display: block; height: 100%; background: var(--color-lab-blue); }
	.message { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--color-lab-border); }
	.merge { background: var(--loot-selected); }
	.link { padding: 0; border: 0; background: transparent; color: var(--color-lab-blue); text-decoration: underline; font: inherit; cursor: pointer; }
	.runs-note { padding: 6px 12px 0; font-size: 11px; color: var(--color-lab-text-secondary); }
	.section { padding: 8px 12px 10px; }
	.section + .section { border-top: 1px solid var(--color-lab-border); }
	.group { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 6px; color: var(--color-lab-text-secondary); font-size: 11px; text-transform: uppercase; letter-spacing: .5px; }
	.status { text-transform: none; letter-spacing: 0; }
	.ask { display: flex; align-items: center; gap: 6px; margin-bottom: 6px; padding: 5px 8px; border-radius: 4px; background: var(--loot-warning); font-size: 12px; }
	.guaranteed-grid, .additional-grid { display: grid; gap: 6px; }
	.guaranteed-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
	.additional-grid { grid-template-columns: repeat(4, minmax(0, 1fr)); }
	.rows { display: flex; flex-direction: column; gap: 2px; }
	footer, .buttons { display: flex; align-items: center; justify-content: space-between; gap: 6px; }
	.unknown { color: var(--color-lab-text-secondary); font-size: 11px; }
	.confirmation :global(.btn) { height: 28px; padding: 0 12px; border-color: var(--color-lab-text-muted); color: var(--color-lab-text); font: inherit; }
	.save :global(.btn) { background: var(--color-lab-blue); border-color: var(--color-lab-blue); color: var(--color-lab-bg); font-weight: 600; }
	.confirmation :global(button:focus-visible) { outline: 1px solid var(--color-lab-blue); outline-offset: -2px; }
	.spinner { width: 10px; height: 10px; border: 2px solid var(--color-lab-border); border-top-color: var(--color-lab-blue); border-radius: 50%; animation: spin 1s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
