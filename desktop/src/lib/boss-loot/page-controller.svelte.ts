import type { BossProfile, BossStats, ModuleSettings, ModuleSettingsPort, ProfilesPort, Quantities, RunsPage, RunsPort, StatsPort, StatsScope, StoredRun } from './contracts'
import { buildResultsView, buildRunEditor, buildRunsView, runsCsv, type ResultsSort } from './page-view'
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
	let runs = $state.raw<RunsPage | null>(null)
	let runsStatus = $state<'loading' | 'ready' | 'unreachable'>('loading')
	let runsError = $state('')
	let runMutationError = $state('')
	let runMutationPending = $state(false)
	let editor = $state.raw<{ run: StoredRun; changes: Quantities } | null>(null)
	let runsGeneration = 0
	let generation = 0
	const currentScope = () => prefs ? prefs.scope.value === 'collective' ? 'collective' : 'own' : scope
	const currentSort = () => prefs ? prefs.sort.value === 'name' ? 'name' : 'rate' : sort
	const results = $derived(profile && stats && settings && status === 'ready'
		? buildResultsView({ profile, stats, scope: currentScope(), sort: currentSort(), notCounted: settings.notCounted }) : null)
	const runsView = $derived(profile && runs && runsStatus === 'ready' ? buildRunsView(profile, runs) : null)
	const editorView = $derived(profile && editor && settings
		? buildRunEditor(profile, editor.run, settings.notCounted, editor.changes) : null)
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
	async function readAllRuns(target: BossProfile): Promise<StoredRun[]> {
		const rows: StoredRun[] = []
		let page = 1
		while (true) {
			const response = await ports.runs.list(target, page, 10)
			rows.push(...response.items)
			if (rows.length >= response.total) return rows
			if (!response.items.length) throw new Error('Incomplete run history; no complete map was read.')
			page++
		}
	}
	async function loadRuns(page = 1) {
		const request = ++runsGeneration
		editor = null
		runs = null
		runsStatus = 'loading'
		runsError = ''
		runMutationError = ''
		if (!profile) return
		try {
			const response = await ports.runs.list(profile, Math.max(1, page), 10)
			if (request !== runsGeneration) return
			const last = Math.max(1, Math.ceil(response.total / response.pageSize))
			if (response.page > last) { await loadRuns(last); return }
			runs = response
			runsStatus = 'ready'
		} catch (cause) {
			if (request !== runsGeneration) return
			runsStatus = 'unreachable'
			runsError = String(cause)
		}
	}
	function beginEdit(id: string) {
		if (runMutationPending) return
		const run = runs?.items.find((run) => run.id === id)
		if (!run) return
		runMutationError = ''
		editor = { run: structuredClone(run), changes: {} }
	}
	function cancelEdit() {
		if (runMutationPending) return
		editor = null
		runMutationError = ''
	}
	function toggleEditorItem(id: string) {
		if (!profile || !editor || !editorView || runMutationPending || !editorView.editableKeys.includes(id)) return
		const changes = { ...editor.changes }
		const group = profile.groups.find((group) => group.items.some((item) => item.id === id))
		const current = editorView.request.items.quantities
		if (group?.max === 1) {
			// An explicit pick-one change replaces any concurrent pick in this group on reapplication.
			for (const item of group.items) if (editorView.editableKeys.includes(item.id)) changes[item.id] = item.id === id ? 1 : 0
		} else changes[id] = Object.hasOwn(current, id) && current[id] > 0 ? 0 : 1
		editor = { ...editor, changes }
		runMutationError = ''
	}
	async function saveEditor() {
		if (!profile || !settings || !editor || !editorView || runMutationPending) return
		const target = profile
		const snapshot = editor
		const requestGeneration = runsGeneration
		runMutationError = ''
		if (!editorView.valid) { runMutationError = 'Check guaranteed groups before saving.'; return }
		runMutationPending = true
		try {
			let saved: StoredRun
			try { saved = await ports.runs.edit(editorView.request) }
			catch (cause) {
				// RunsPort has no typed rejection code. A newer owned row version is the retry condition.
				let latest: StoredRun | undefined
				try { latest = (await readAllRuns(target)).find((run) => run.id === snapshot.run.id) }
				catch { throw cause }
				if (!latest || latest.version === snapshot.run.version) throw cause
				const reapplied = buildRunEditor(target, latest, settings.notCounted, snapshot.changes)
				if (!reapplied.valid) throw new Error('Run changed: check guaranteed groups before saving.')
				saved = await ports.runs.edit(reapplied.request)
			}
			if (requestGeneration === runsGeneration && runs) {
				runs = { ...runs, items: runs.items.map((run) => run.id === saved.id ? saved : run) }
				editor = null
			}
			await refreshStats()
		} catch (cause) {
			if (requestGeneration === runsGeneration) runMutationError = String(cause)
		} finally { runMutationPending = false }
	}
	async function deleteRun(id: string) {
		if (runMutationPending) return
		const run = runs?.items.find((run) => run.id === id)
		if (!run || !runs) return
		const page = runs.page
		const requestGeneration = runsGeneration
		runMutationPending = true
		runMutationError = ''
		try {
			await ports.runs.delete({ id, expectedVersion: run.version })
			if (requestGeneration === runsGeneration) await loadRuns(page)
			await refreshStats()
		} catch (cause) {
			if (requestGeneration === runsGeneration) runMutationError = String(cause)
		} finally { runMutationPending = false }
	}
	async function exportCsv() {
		if (!profile || exporting) return
		const target = profile
		exporting = true
		error = ''
		try {
			const rows = await readAllRuns(target)
			await ports.saveCsv(target, runsCsv(target, rows))
		} catch (cause) { error = String(cause) }
		finally { exporting = false }
	}
	return {
		get profiles() { return profiles }, get profile() { return profile }, get settings() { return settings },
		get scope() { return currentScope() }, get sort() { return currentSort() },
		get results() { return results }, get status() { return status }, get error() { return error },
		get runsView() { return runsView }, get runsStatus() { return runsStatus }, get runsError() { return runsError },
		get editorView() { return editorView }, get runMutationPending() { return runMutationPending },
		get runMutationError() { return runMutationError }, loadRuns, beginEdit, cancelEdit, toggleEditorItem, saveEditor, deleteRun,
		get exporting() { return exporting }, load, selectScope, selectSort, exportCsv
	}
}
