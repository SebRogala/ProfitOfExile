<script lang="ts">
	import WidgetHost from '$lib/overlay/widgets/WidgetHost.svelte'
	import CaptureFrame from '$lib/boss-loot/CaptureFrame.svelte'
	import LootConfirmation from '$lib/boss-loot/LootConfirmation.svelte'
	import fixture from '$lib/boss-loot/__fixtures__/boss-loot.json'
	import {
		createFixtureCapture, createFixtureModuleSettings, createFixtureOverlaySettings,
		createFixtureProfiles, createFixtureRuns, createFixtureStats, createFixtureSubmit
	} from '$lib/boss-loot/fixture-adapters'
	import { buildOverlayView, changeSelection, type OverlayInput } from '$lib/boss-loot/overlay-view'
	import type { OverlaySettings, StatsScope, SubmitRun } from '$lib/boss-loot/contracts'
	import { getApiBase } from '$lib/api'

	// Owner-specific fixture seams are replaced by native adapters in WI-9.
	const profiles = createFixtureProfiles(fixture.profiles)
	const captures = createFixtureCapture(fixture.capture)
	const stats = createFixtureStats(fixture.stats)
	const settings = createFixtureModuleSettings(fixture.moduleSettings)
	const overlays = createFixtureOverlaySettings(fixture.overlaySettings)
	const runs = createFixtureRuns(fixture.runs)
	let input = $state<OverlayInput | null>(null)
	let overlaySettings = $state<OverlaySettings | null>(null)
	let previewShortcut = $state('')
	let confirmation: LootConfirmation | undefined
	let closeTimer: ReturnType<typeof setTimeout> | undefined
	let scopeGeneration = 0
	let disposed = false
	const apiBase = $derived(getApiBase())

	// Native crop/mode remain view props until WI-9 supplies the capture owner.
	let frame = $state<{
		mode: 'preview' | 'capture' | 'refused'
		rect: { x: number; y: number; width: number; height: number }
		boss: string; run: number; area: string; level: number; trackedBosses: string[]
	} | null>(null)

	$effect(() => {
		disposed = false
		async function load(): Promise<void> {
			try {
				const [[profile], capture, moduleSettings, appearance] = await Promise.all([
					profiles.load(), captures.load(), settings.load(), overlays.load()
				])
				if (!profile || !capture) return
				const own = await stats.load(profile, 'own')
				if (disposed) return
				overlaySettings = appearance
				previewShortcut = moduleSettings.previewShortcut
				input = { profile, capture, stats: own, scope: 'own', layout: appearance.layout,
					notCounted: moduleSettings.notCounted, autoSaveSeconds: moduleSettings.autoSaveSeconds,
					seconds: moduleSettings.autoSaveSeconds }
			} catch (error) { console.error('[loot] fixture owners could not load:', error) }
		}
		void load()
		const timer = setInterval(() => {
			if (!input || !buildOverlayView(input).canSave || input.autoSaveSeconds === 0) return
			input.seconds = Math.max(0, input.seconds - 1)
			if (input.seconds === 0) void save()
		}, 1000)
		return () => { disposed = true; clearInterval(timer); clearTimeout(closeTimer) }
	})

	function touch(): void {
		if (input && !input.outcome) input.seconds = input.autoSaveSeconds
	}
	async function selectScope(scope: StatsScope): Promise<void> {
		if (!input) return
		const generation = ++scopeGeneration
		try {
			const next = await stats.load(input.profile, scope)
			if (input && !disposed && generation === scopeGeneration) { input.scope = scope; input.stats = next }
		} catch (error) { console.error('[loot] stats owner failed:', error) }
	}
	async function save(): Promise<void> {
		if (!input || !buildOverlayView(input).canSave) return
		const pending = $state.snapshot(input)
		input.outcome = { state: 'saving' }
		const quantities = {
			...Object.fromEntries(Object.values(pending.capture.picks).flat().map((key) => [key, 1])),
			...pending.capture.extra, ...pending.capture.categories
		}
		const counted = [...pending.profile.groups.flatMap((group) => group.items), ...pending.profile.extra, ...pending.profile.categories]
			.map((item) => item.id).filter((id) => !pending.notCounted.includes(id))
		const request: SubmitRun = {
			boss: pending.profile.boss, profileKey: pending.profile.profileKey, profileVersion: pending.profile.profileVersion,
			league: 'fixture-only', seed: pending.capture.seed, mode: 'new',
			items: { quantities: Object.fromEntries(Object.entries(quantities).filter(([id, quantity]) => counted.includes(id) && quantity > 0)), counted }
		}
		try {
			const page = await runs.list(pending.profile, 1, 1)
			const row = { ...page.items[0], id: `fixture-catarina-${pending.capture.run}`, run: pending.capture.run, items: request.items }
			const submit = createFixtureSubmit(fixture.submit, 'saved', row)
			await submit.submit(request, (outcome) => { if (input && !disposed) input.outcome = outcome })
			closeTimer = setTimeout(() => { input = null }, 2000)
		} catch (error) {
			console.error('[loot] fixture submit owner failed:', error)
			if (input) input.outcome = { state: 'unreachable' }
		}
	}
	function action(name: string, id?: string): void {
		if (!input) return
		if (name === 'loot.own' || name === 'loot.collective') { void selectScope(name === 'loot.own' ? 'own' : 'collective'); return }
		if (name === 'loot.close' || name === 'loot.discard') { input = null; return }
		if (name === 'loot.save') { void save(); return }
		if (input.outcome) return
		if (name === 'loot.force-new') { input.previousRun = undefined; return }
		if (!id || !['loot.toggle', 'loot.increment', 'loot.decrement'].includes(name)) return
		const selection = changeSelection(input.profile, $state.snapshot(input.capture), input.notCounted,
			name === 'loot.toggle' ? { type: 'toggle', id } : { type: 'quantity', id, delta: name === 'loot.increment' ? 1 : -1 })
		input.capture = { ...input.capture, ...selection }
	}
	function iconFor(id: string): string | null {
		if (id.startsWith('category:')) return null
		const category = id.startsWith('Metadata/') ? 'currency-exchange' : 'items'
		return `${apiBase}/api/icon/${category}/${encodeURIComponent(id)}`
	}
</script>

<WidgetHost module="loot" onAction={(name, element) => confirmation?.handleAction(name, element)}>
	{#snippet content(spec, configMode)}
		{#if spec.id === 'loot.confirmation'}
			{#if input && overlaySettings?.show}
				<LootConfirmation bind:this={confirmation} {input} {iconFor} ontouch={touch} onaction={action} />
			{:else if configMode}<div class="placeholder">Loot confirmation</div>{/if}
		{/if}
	{/snippet}
	{#snippet anchored(spec, host)}
		{#if spec.id === 'loot.capture' && frame && host.scaleFactor > 0}
			<CaptureFrame {...frame} rect={{ x: frame.rect.x / host.scaleFactor, y: frame.rect.y / host.scaleFactor,
				width: frame.rect.width / host.scaleFactor, height: frame.rect.height / host.scaleFactor }} {previewShortcut} />
		{/if}
	{/snippet}
</WidgetHost>

<style>
	:global(html), :global(body) { margin: 0; padding: 0; background: transparent !important; overflow: hidden; }
	.placeholder { padding: 12px; color: var(--color-lab-text-secondary); font: 12px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; }
</style>
