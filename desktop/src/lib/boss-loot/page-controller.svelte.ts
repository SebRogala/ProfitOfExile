import type { BossProfile, BossStats, DevicePort, ModuleSettings, ModuleSettingsPort, ProfilesPort, Quantities, RunsPage, RunsPort, StatsPort, StatsScope, StoredRun } from './contracts'
import { buildResultsView, buildRunEditor, buildRunsView, buildSettingsView, keyboardShortcut, mouseShortcut, runsCsv, type ResultsSort, type ShortcutKeyInput, type ShortcutMouseInput } from './page-view'
import type { PersistedString } from '$lib/prefs.svelte'
import { ssot, setModuleEnabled } from '$lib/stores/ssot.svelte'

export interface PagePorts {
	profiles: ProfilesPort
	stats: StatsPort
	moduleSettings: ModuleSettingsPort
	device: DevicePort
	runs: RunsPort
	/** WI-5/9 supplies the native destination dialog and writer; cancellation resolves without writing. */
	saveCsv(profile: BossProfile, csv: string): Promise<void>
}

export function createBossLootPageController(ports: PagePorts, prefs?: { scope: PersistedString; sort: PersistedString }) {
	let profiles = $state.raw<BossProfile[]>([])
	let profile = $state.raw<BossProfile | null>(null)
	let settings = $state.raw<ModuleSettings | null>(null)
	let settingsStatus = $state<'loading' | 'ready' | 'unreachable'>('loading')
	let settingsError = $state('')
	let settingsPending = $state(false)
	let modulePending = $state(false)
	let listening = $state<'capture' | 'preview' | null>(null)
	let deviceId = $state<string | null>(null)
	let deviceError = $state('')
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
	const settingsView = $derived(settings ? buildSettingsView(profile, settings) : null)
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
		await Promise.all([
			(async () => {
				try { profiles = await ports.profiles.load(); profile = profiles[0] ?? null }
				catch (cause) { error = String(cause) }
			})(),
			(async () => {
				settingsStatus = 'loading'
				try { settings = await ports.moduleSettings.load(); settingsStatus = 'ready' }
				catch (cause) { settingsStatus = 'unreachable'; settingsError = String(cause); error = String(cause) }
			})(),
			(async () => {
				try { deviceId = (await ports.device.load()).id }
				catch (cause) { deviceError = String(cause) }
			})()
		])
		if (profile && settings) await refreshStats()
		else status = 'unreachable'
	}
	async function setEnabled(enabled: boolean) {
		if (modulePending) return
		modulePending = true
		try { await setModuleEnabled('boss_loot', enabled) }
		finally { modulePending = false }
	}
	type BehaviorPatch = Partial<Pick<ModuleSettings, 'autoSaveSeconds' | 'captureShortcut' | 'previewShortcut' | 'notCounted'>>
	async function saveBehavior(patch: BehaviorPatch) {
		if (!settings || settingsPending) return
		settingsPending = true
		settingsError = ''
		try {
			// The wire echo includes enabled; the page's enabled reader/writer remains the module SSOT.
			settings = await ports.moduleSettings.save({ ...settings, ...patch, enabled: ssot.modules.boss_loot ?? false })
			if (patch.notCounted) editor = null
		} catch (cause) { settingsError = String(cause) }
		finally { settingsPending = false }
	}
	async function setAutoSave(seconds: number) {
		if (!settings) return
		if (!settings.autoSaveOptions.includes(seconds)) { settingsError = 'Unsupported auto-save timeout'; return }
		await saveBehavior({ autoSaveSeconds: seconds })
	}
	async function setNotCounted(id: string, checked: boolean) {
		if (!settings || !profile) return
		if (!profile.extra.some((item) => item.id === id)) { settingsError = 'Item is not an additional drop'; return }
		const notCounted = settings.notCounted.filter((key) => key !== id)
		if (checked) notCounted.push(id)
		await saveBehavior({ notCounted })
	}
	function listenShortcut(target: 'capture' | 'preview') {
		if (settings && !settingsPending) listening = target
	}
	function cancelShortcut() { listening = null }
	async function saveShortcut(binding: string) {
		if (!listening) return
		const field = listening === 'capture' ? 'captureShortcut' : 'previewShortcut'
		listening = null
		await saveBehavior({ [field]: binding })
	}
	async function captureKey(input: ShortcutKeyInput) {
		if (input.key === 'Escape') { cancelShortcut(); return }
		const binding = keyboardShortcut(input)
		if (binding) await saveShortcut(binding)
	}
	async function captureMouse(input: ShortcutMouseInput) {
		const binding = mouseShortcut(input)
		if (binding) await saveShortcut(binding)
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
		get enabled() { return ssot.modules.boss_loot ?? false }, get modulePending() { return modulePending }, setEnabled,
		get settingsView() { return settingsView }, get settingsStatus() { return settingsStatus },
		get settingsError() { return settingsError }, get settingsPending() { return settingsPending },
		get deviceId() { return deviceId }, get deviceError() { return deviceError },
		get listening() { return listening }, listenShortcut, cancelShortcut, captureKey, captureMouse, setAutoSave, setNotCounted,
		get scope() { return currentScope() }, get sort() { return currentSort() },
		get results() { return results }, get status() { return status }, get error() { return error },
		get runsView() { return runsView }, get runsStatus() { return runsStatus }, get runsError() { return runsError },
		get editorView() { return editorView }, get runMutationPending() { return runMutationPending },
		get runMutationError() { return runMutationError }, loadRuns, beginEdit, cancelEdit, toggleEditorItem, saveEditor, deleteRun,
		get exporting() { return exporting }, load, selectScope, selectSort, exportCsv
	}
}
