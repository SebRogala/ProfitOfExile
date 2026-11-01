import type { BossProfile, BossStats, ModuleSettings, ModuleSettingsPort, ProfilesPort, RunsPort, StatsPort, StatsScope } from './contracts'
import { buildResultsView, runsCsv, type ResultsSort } from './page-view'
import type { PersistedString } from '$lib/prefs.svelte'

export interface PagePorts {
	profiles: ProfilesPort
	stats: StatsPort
	moduleSettings: ModuleSettingsPort
	runs: RunsPort
	/** WI-5/9 supplies the native destination dialog and writer; cancellation resolves without writing. */
	saveCsv(profile: BossProfile, csv: string): Promise<void>
}

export function createBossLootPageController(ports: PagePorts, prefs?: { scope: PersistedString; sort: PersistedString }) {
	let profiles = $state.raw<BossProfile[]>([])
	let profile = $state.raw<BossProfile | null>(null)
	let settings = $state.raw<ModuleSettings | null>(null)
	let stats = $state.raw<BossStats | null>(null)
	let scope = $state<StatsScope>('own')
	let sort = $state<ResultsSort>('rate')
	let status = $state<'loading' | 'ready' | 'unreachable'>('loading')
	let error = $state('')
	let exporting = $state(false)
	let generation = 0
	const currentScope = () => prefs ? prefs.scope.value === 'collective' ? 'collective' : 'own' : scope
	const currentSort = () => prefs ? prefs.sort.value === 'name' ? 'name' : 'rate' : sort
	const results = $derived(profile && stats && settings && status === 'ready'
		? buildResultsView({ profile, stats, scope: currentScope(), sort: currentSort(), notCounted: settings.notCounted }) : null)
	async function refreshStats() {
		const request = ++generation
		stats = null
		status = 'loading'
		error = ''
		if (!profile) { status = 'ready'; return }
		try {
			const response = await ports.stats.load(profile, currentScope())
			if (request !== generation) return
			stats = response
			status = 'ready'
		} catch (cause) {
			if (request !== generation) return
			status = 'unreachable'
			error = String(cause)
		}
	}
	async function load() {
		try {
			const [loadedProfiles, loadedSettings] = await Promise.all([ports.profiles.load(), ports.moduleSettings.load()])
			profiles = loadedProfiles
			profile = profiles[0] ?? null
			settings = loadedSettings
			await refreshStats()
		} catch (cause) { status = 'unreachable'; error = String(cause) }
	}
	async function selectScope(next: StatsScope) {
		scope = next
		if (prefs) prefs.scope.value = next
		await refreshStats()
	}
	function selectSort(next: ResultsSort) {
		sort = next
		if (prefs) prefs.sort.value = next
	}
	async function exportCsv() {
		if (!profile || exporting) return
		const target = profile
		exporting = true
		error = ''
		try {
			const rows = []
			let page = 1
			while (true) {
				const response = await ports.runs.list(target, page, 10)
				rows.push(...response.items)
				if (rows.length >= response.total) break
				if (!response.items.length) throw new Error('Incomplete run history; CSV was not exported.')
				page++
			}
			await ports.saveCsv(target, runsCsv(target, rows))
		} catch (cause) { error = String(cause) }
		finally { exporting = false }
	}
	return {
		get profiles() { return profiles }, get profile() { return profile }, get settings() { return settings },
		get scope() { return currentScope() }, get sort() { return currentSort() },
		get results() { return results }, get status() { return status }, get error() { return error },
		get exporting() { return exporting }, load, selectScope, selectSort, exportCsv
	}
}
