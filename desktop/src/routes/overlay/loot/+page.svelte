<script lang="ts">
	import { untrack } from 'svelte'
	import { invoke } from '@tauri-apps/api/core'
	import WidgetHost from '$lib/overlay/widgets/WidgetHost.svelte'
	import CaptureFrame from '$lib/boss-loot/CaptureFrame.svelte'
	import LootConfirmation from '$lib/boss-loot/LootConfirmation.svelte'
	import { createNativeCapture, createNativeSubmit, createNativeProfiles, createNativeStats, mapNativeOverlay, shortcutLabel } from '$lib/boss-loot/native-adapters'
	import { bossLootIcon } from '$lib/boss-loot/art'
	import { ssot } from '$lib/stores/ssot.svelte'
	import type { BossProfile, BossStats, StatsScope } from '$lib/boss-loot/contracts'

	const captures = createNativeCapture()
	const submit = createNativeSubmit()
	const profilesOwner = createNativeProfiles()
	const statsOwner = createNativeStats()
	let profiles = $state.raw<BossProfile[]>([])
	let stats = $state.raw<BossStats | null>(null)
	let scope = $state<StatsScope>('own')
	let confirmation = $state<LootConfirmation | undefined>(undefined)
	let statsGeneration = 0
	let actions = Promise.resolve()
	const native = $derived(ssot.bossLoot)
	const profile = $derived(native?.pending?.profile ?? profiles[0])
	const statsKey = $derived(profile ? `${profile.boss}:${profile.key}:${profile.profileVersion}:${scope}:${native?.pending?.id ?? ''}:${native?.pending?.phase ?? ''}` : '')
	const input = $derived(native ? mapNativeOverlay(native, stats, scope) : null)
	const frame = $derived(native?.frame)
	const previewShortcut = $derived(native ? shortcutLabel(native.settings.previewShortcut) : '')

	function report(cause: unknown): void {
		const msg = `[loot] ${String(cause)}`
		console.error(msg)
		void invoke('app_log_from_frontend', { msg }).catch((error) => console.error('[loot] log failed:', error))
	}
	$effect(() => {
		let disposed = false
		untrack(() => {
			void profilesOwner.load().then((next) => { if (!disposed) profiles = next }).catch(report)
		})
		return () => { disposed = true }
	})
	$effect(() => {
		const key = statsKey
		const request = ++statsGeneration
		if (!key) return
		untrack(() => {
			const target = profile!
			stats = null
			void statsOwner.load({ boss: target.boss, profileKey: target.key, profileVersion: target.profileVersion }, scope)
				.then((next) => { if (request === statsGeneration) stats = next })
				.catch((cause) => { if (request === statsGeneration) report(cause) })
		})
		return () => { if (request === statsGeneration) statsGeneration++ }
	})
	function enqueue(work: () => Promise<void>): void {
		// Touch and the nested intent retain the clicked capture identity and native ordering.
		actions = actions.then(work).catch(report)
	}
	function touch(): void {
		const id = native?.pending?.id
		if (id !== undefined) enqueue(() => captures.action(id, { type: 'touch' }))
	}
	function action(name: string, item?: string): void {
		const pending = native?.pending
		if (!pending) return
		const id = pending.id
		if (name === 'loot.own' || name === 'loot.collective') {
			enqueue(async () => { scope = name === 'loot.own' ? 'own' : 'collective' }); return
		}
		if (name === 'loot.close' || name === 'loot.discard') { enqueue(() => captures.action(id, { type: 'discard' })); return }
		if (name === 'loot.save') { enqueue(() => submit.submit(id)); return }
		if (name === 'loot.force-new') { enqueue(() => captures.action(id, { type: 'forceNew' })); return }
		if (!item || !input || !['loot.toggle', 'loot.increment', 'loot.decrement'].includes(name)) return
		const selection = input.capture
		const quantity = Object.values(selection.picks).some((keys) => keys.includes(item)) ? 1
			: selection.extra[item] ?? selection.categories[item] ?? 0
		const next = name === 'loot.toggle' ? quantity > 0 ? 0 : 1 : Math.max(0, quantity + (name === 'loot.increment' ? 1 : -1))
		enqueue(() => captures.action(id, { type: 'select', key: item, quantity: next }))
	}
</script>

<WidgetHost module="loot" onAction={(name, element) => confirmation?.handleAction(name, element)}>
	{#snippet content(spec, configMode)}
		{#if spec.id === 'loot.confirmation'}
			{#if input}
				<LootConfirmation bind:this={confirmation} {input} iconFor={bossLootIcon} ontouch={touch} onaction={action} />
			{:else if configMode}<div class="placeholder">Loot confirmation</div>{/if}
		{/if}
	{/snippet}
	{#snippet anchored(spec, host)}
		{#if spec.id === 'loot.capture' && frame && host.scaleFactor > 0}
			<CaptureFrame mode={frame.phase === 'reading' ? 'capture' : frame.phase === 'refused' ? 'refused' : 'preview'}
				rect={{ x: frame.rect[0] / host.scaleFactor, y: frame.rect[1] / host.scaleFactor,
					width: frame.rect[2] / host.scaleFactor, height: frame.rect[3] / host.scaleFactor }}
				boss={native?.pending?.profile.name ?? profiles[0]?.name ?? ''} run={native?.pending?.run || stats?.nextRun || 0}
				area={frame.attribution?.area ?? 'unknown'} level={frame.attribution?.level ?? 'unknown'}
				trackedBosses={profiles.map((profile) => profile.name)} {previewShortcut} />
		{/if}
	{/snippet}
</WidgetHost>

<style>
	:global(html), :global(body) { margin: 0; padding: 0; background: transparent !important; overflow: hidden; }
	.placeholder { padding: 12px; color: var(--color-lab-text-secondary); font: 12px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; }
</style>
