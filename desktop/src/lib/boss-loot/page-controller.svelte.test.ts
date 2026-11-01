import { describe, expect, it } from 'vitest'
import source from './__fixtures__/boss-loot.json'
import { createFixtureProfiles, createFixtureModuleSettings, createFixtureStats, createFixtureRuns } from './fixture-adapters'
import { createBossLootPageController, type PagePorts } from './page-controller.svelte'
import type { BossStats } from './contracts'

function ports(): PagePorts {
	return { profiles: createFixtureProfiles(source.profiles), stats: createFixtureStats(source.stats),
		moduleSettings: createFixtureModuleSettings(source.moduleSettings), runs: createFixtureRuns(source.runs),
		saveCsv: async () => {} }
}

function deferred<T>() {
	let resolve!: (value: T) => void
	let reject!: (error: unknown) => void
	const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
	return { promise, resolve, reject }
}

describe('Results controller', () => {
	it('keeps the table empty while statistics load', async () => {
		const pending = deferred<BossStats>()
		const deps = ports()
		deps.stats = { load: () => pending.promise }
		const page = createBossLootPageController(deps)
		const loading = page.load()
		await Promise.resolve(); await Promise.resolve()
		expect(page.results).toBeNull()
		expect(page.status).toBe('loading')
		pending.resolve(await createFixtureStats(source.stats).load((await deps.profiles.load())[0], 'own'))
		await loading
		expect(page.results?.summary).toBe('78 runs · this device')
	})
	it('keeps the table empty on an unreachable statistics read', async () => {
		const deps = ports()
		deps.stats = { load: async () => { throw new Error('offline') } }
		const page = createBossLootPageController(deps)
		await page.load()
		expect(page.results).toBeNull()
		expect(page.status).toBe('unreachable')
		expect(page.error).toContain('offline')
	})
	it('rejects a stale Own response after switching to Collective', async () => {
		const deps = ports()
		const own = deferred<BossStats>()
		const profile = (await deps.profiles.load())[0]
		deps.stats = { load: (_profile, scope) => scope === 'own' ? own.promise : Promise.resolve({ runs: 3, devices: 2, nextRun: 4, items: {} }) }
		const page = createBossLootPageController(deps)
		const first = page.load()
		await Promise.resolve(); await Promise.resolve(); await Promise.resolve()
		await page.selectScope('collective')
		own.resolve(await createFixtureStats(source.stats).load(profile, 'own'))
		await first
		expect(page.results?.summary).toBe('3 runs · 2 devices')
	})
	it('exports every owned run beyond the visible ten', async () => {
		const deps = ports()
		let written = ''
		deps.saveCsv = async (_profile, csv) => { written = csv }
		const page = createBossLootPageController(deps)
		await page.load()
		await page.exportCsv()
		expect(written.split('\r\n')).toHaveLength(79)
		expect(written.split('\r\n')[1]).toMatch(/^78,,/)
		expect(written.split('\r\n')[78]).toMatch(/^1,,/)
	})
	it('does not write a partial CSV after a failed later page read', async () => {
		const deps = ports()
		const original = deps.runs.list
		let written = ''
		deps.runs.list = (profile, page, size) => page === 2 ? Promise.reject(new Error('offline')) : original(profile, page, size)
		deps.saveCsv = async (_profile, csv) => { written = csv }
		const page = createBossLootPageController(deps)
		await page.load()
		await page.exportCsv()
		expect(written).toBe('')
		expect(page.error).toContain('offline')
	})
})

async function runsHarness(notCounted: string[] = []) {
	const deps = ports()
	const [profile] = await deps.profiles.load()
	const original = deps.runs.list
	const rows = new Map<string, import('./contracts').StoredRun>()
	for (let page = 1; page <= 8; page++) for (const row of (await original(profile, page, 10)).items) rows.set(row.id, row)
	const writes: import('./contracts').EditRun[] = []
	const deletions: import('./contracts').DeleteRun[] = []
	let editError = ''
	let deleteError = ''
	const moduleSettings = await deps.moduleSettings.load()
	deps.moduleSettings.load = async () => ({ ...moduleSettings, notCounted })
	deps.runs = {
		list: async (_profile, page, pageSize) => ({ page, pageSize, total: rows.size,
			items: structuredClone([...rows.values()].sort((a, b) => b.run - a.run).slice((page - 1) * pageSize, page * pageSize)) }),
		edit: async (request) => {
			writes.push(structuredClone(request))
			if (editError) throw new Error(editError)
			const current = rows.get(request.id)!
			if (current.version !== request.expectedVersion) throw new Error('Version conflict')
			const saved = { ...current, version: current.version + 1, items: structuredClone(request.items) }
			rows.set(saved.id, saved)
			return structuredClone(saved)
		},
		delete: async (request) => {
			deletions.push(structuredClone(request))
			if (deleteError) throw new Error(deleteError)
			rows.delete(request.id)
		}
	}
	const page = createBossLootPageController(deps)
	await page.load()
	return { page, deps, profile, rows, writes, deletions, setEditError: (error: string) => { editError = error },
		setDeleteError: (error: string) => { deleteError = error } }
}

describe('Runs controller', () => {
	it('keeps the Runs table empty while its page loads', async () => {
		const h = await runsHarness()
		const pending = deferred<import('./contracts').RunsPage>()
		h.deps.runs.list = () => pending.promise
		const loading = h.page.loadRuns(1)
		expect(h.page.runsView).toBeNull()
		expect(h.page.runsStatus).toBe('loading')
		pending.resolve({ total: 0, page: 1, pageSize: 10, items: [] })
		await loading
		expect(h.page.runsView?.range).toBe('No runs logged for this boss yet.')
	})
	it('keeps the Runs table empty after an unreachable page read', async () => {
		const h = await runsHarness()
		h.deps.runs.list = async () => { throw new Error('runs offline') }
		await h.page.loadRuns(1)
		expect(h.page.runsView).toBeNull()
		expect(h.page.runsError).toContain('runs offline')
	})
	it('closes the editor when a different page is requested', async () => {
		const h = await runsHarness()
		await h.page.loadRuns(8)
		h.page.beginEdit('fixture-catarina-8')
		const loading = h.page.loadRuns(1)
		expect(h.page.editorView).toBeNull()
		await loading
		expect(h.page.runsView?.range).toBe('Runs 78–69 of 78, newest first')
	})
	it('cancels an edit without submitting a map', async () => {
		const h = await runsHarness()
		await h.page.loadRuns(8)
		h.page.beginEdit('fixture-catarina-8')
		h.page.toggleEditorItem('Cinderswallow Urn')
		h.page.cancelEdit()
		expect(h.writes).toEqual([])
		expect(h.page.runsView?.rows[0].unique.map((item) => item.id)).toEqual(['Cane of Kulemak'])
	})
	it('preserves the full hidden Eye history while correcting a unique', async () => {
		const eye = 'Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly'
		const h = await runsHarness([eye])
		await h.page.loadRuns(8)
		const counted = [...h.rows.get('fixture-catarina-8')!.items.counted]
		h.page.beginEdit('fixture-catarina-8')
		expect(h.page.editorView?.extra.map((item) => item.id)).not.toContain(eye)
		h.page.toggleEditorItem('Cinderswallow Urn')
		await h.page.saveEditor()
		expect(h.writes[0].items).toEqual({ quantities: { 'Cinderswallow Urn': 1, [eye]: 1 }, counted })
	})
	it('changes only the explicitly toggled additional key', async () => {
		const h = await runsHarness()
		await h.page.loadRuns(8)
		h.page.beginEdit('fixture-catarina-8')
		h.page.toggleEditorItem('Metadata/Items/Currency/CurrencyRerollRareVeiled')
		await h.page.saveEditor()
		expect(h.writes[0].items.quantities).toEqual({ 'Cane of Kulemak': 1,
			'Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly': 1, 'Metadata/Items/Currency/CurrencyRerollRareVeiled': 1 })
	})
	it('preserves newly stored hidden quantities when a stale edit is reapplied', async () => {
		const eye = 'Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly'
		const h = await runsHarness([eye])
		await h.page.loadRuns(8)
		h.page.beginEdit('fixture-catarina-8')
		h.page.toggleEditorItem('Cinderswallow Urn')
		const latest = h.rows.get('fixture-catarina-8')!
		latest.version = 2
		latest.items.quantities[eye] = 4
		latest.items.quantities['historical-unknown'] = 3
		await h.page.saveEditor()
		expect(h.writes.at(-1)).toMatchObject({ expectedVersion: 2, items: {
			quantities: { 'Cinderswallow Urn': 1, [eye]: 4, 'historical-unknown': 3 }, counted: latest.items.counted } })
		expect(h.writes.at(-1)!.items.quantities).not.toHaveProperty('Cane of Kulemak')
	})
	it('retains an intervening visible change that the editor did not touch', async () => {
		const h = await runsHarness()
		await h.page.loadRuns(8)
		h.page.beginEdit('fixture-catarina-8')
		h.page.toggleEditorItem('Metadata/Items/Currency/CurrencyRerollRareVeiled')
		const latest = h.rows.get('fixture-catarina-8')!
		latest.version = 2
		delete latest.items.quantities['Cane of Kulemak']
		latest.items.quantities['Spinehail'] = 1
		await h.page.saveEditor()
		expect(h.writes.at(-1)!.items.quantities).toEqual({ Spinehail: 1,
			'Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly': 1, 'Metadata/Items/Currency/CurrencyRerollRareVeiled': 1 })
	})
	it('preserves the server-confirmed row when a save is rejected', async () => {
		const h = await runsHarness()
		await h.page.loadRuns(8)
		h.page.beginEdit('fixture-catarina-8')
		h.page.toggleEditorItem('Cinderswallow Urn')
		h.setEditError('edit offline')
		await h.page.saveEditor()
		expect(h.page.runsView?.rows[0].unique.map((item) => item.id)).toEqual(['Cane of Kulemak'])
		expect(h.page.runMutationError).toContain('edit offline')
		expect(h.page.editorView?.groups[0].items.find((item) => item.id === 'Cinderswallow Urn')?.selected).toBe(true)
	})
	it('shows the server-confirmed unique after a successful save', async () => {
		const h = await runsHarness()
		await h.page.loadRuns(8)
		h.page.beginEdit('fixture-catarina-8')
		h.page.toggleEditorItem('Cinderswallow Urn')
		await h.page.saveEditor()
		expect(h.page.runsView?.rows[0].unique.map((item) => item.id)).toEqual(['Cinderswallow Urn'])
		expect(h.page.editorView).toBeNull()
	})
	it('preserves the server-confirmed row when delete is rejected', async () => {
		const h = await runsHarness()
		await h.page.loadRuns(8)
		h.setDeleteError('delete offline')
		await h.page.deleteRun('fixture-catarina-8')
		expect(h.page.runsView?.rows.map((row) => row.run)).toEqual([8, 7, 6, 5, 4, 3, 2, 1])
		expect(h.page.runMutationError).toContain('delete offline')
	})
	it('refills the page from confirmed history after deleting a run', async () => {
		const h = await runsHarness()
		await h.page.loadRuns(8)
		await h.page.deleteRun('fixture-catarina-8')
		expect(h.page.runsView?.rows.map((row) => row.run)).toEqual([7, 6, 5, 4, 3, 2, 1])
		expect(h.page.runsView?.range).toBe('Runs 7–1 of 77, newest first')
	})
	it('rejects an obsolete page response', async () => {
		const h = await runsHarness()
		const pending = deferred<import('./contracts').RunsPage>()
		const list = h.deps.runs.list
		h.deps.runs.list = (profile, page, size) => page === 1 ? pending.promise : list(profile, page, size)
		const first = h.page.loadRuns(1)
		await h.page.loadRuns(8)
		pending.resolve(await list(h.profile, 1, 10))
		await first
		expect(h.page.runsView?.range).toBe('Runs 8–1 of 78, newest first')
	})
	it('does not submit hidden item toggles', async () => {
		const eye = 'Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly'
		const h = await runsHarness([eye])
		await h.page.loadRuns(8)
		h.page.beginEdit('fixture-catarina-8')
		h.page.toggleEditorItem(eye)
		await h.page.saveEditor()
		expect(h.writes[0].items.quantities[eye]).toBe(1)
	})
	it('keeps the Black Star fixture quantity of three while editing another item', async () => {
		const deps = ports()
		const profile = (await createFixtureProfiles(source.testProfiles).load()).find((profile) => profile.boss === 'blackstar')!
		const run: import('./contracts').StoredRun = { ...profile, id: 'blackstar-run', league: 'fixture-only', run: 1, date: null, seed: null,
			version: 1, items: { counted: profile.extra.map((item) => item.id), quantities: {
				'The Eternal Struggle': 1, 'Grand Eldritch Ember': 3, 'Eldritch Chaos Orb': 1 } } }
		let submitted: import('./contracts').EditRun | undefined
		deps.profiles.load = async () => [profile]
		deps.stats.load = async () => ({ runs: 1, devices: 1, nextRun: 2, items: {} })
		deps.moduleSettings.load = async () => ({ ...source.moduleSettings, notCounted: ['Grand Eldritch Ember'] })
		deps.runs.list = async () => ({ total: 1, page: 1, pageSize: 10, items: [run] })
		deps.runs.edit = async (request) => { submitted = request; return { ...run, version: 2, items: request.items } }
		const page = createBossLootPageController(deps)
		await page.load(); await page.loadRuns(1)
		page.beginEdit(run.id)
		page.toggleEditorItem('Polaric Devastation')
		await page.saveEditor()
		expect(submitted?.items.quantities).toEqual({ 'The Eternal Struggle': 1, 'Grand Eldritch Ember': 3,
			'Eldritch Chaos Orb': 1, 'Polaric Devastation': 1 })
	})
})

	describe('Runs mutation boundaries', () => {
		it('returns to the previous page after deleting the final row on a page', async () => {
			const h = await runsHarness()
			for (let run = 2; run <= 8; run++) h.rows.delete(`fixture-catarina-${run}`)
			await h.page.loadRuns(8)
			await h.page.deleteRun('fixture-catarina-1')
			expect(h.page.runsView?.page).toBe(7)
			expect(h.page.runsView?.rows.map((row) => row.run)).toEqual([18, 17, 16, 15, 14, 13, 12, 11, 10, 9])
		})
		it('preserves the displayed row when the retry of a stale correction is rejected', async () => {
			const h = await runsHarness()
			await h.page.loadRuns(8)
			h.page.beginEdit('fixture-catarina-8')
			h.page.toggleEditorItem('Cinderswallow Urn')
			h.rows.get('fixture-catarina-8')!.version = 2
			const edit = h.deps.runs.edit
			h.deps.runs.edit = async (request) => {
				if (request.expectedVersion === 2) throw new Error('changed again')
				return edit(request)
			}
			await h.page.saveEditor()
			expect(h.page.runsView?.rows[0].unique.map((item) => item.id)).toEqual(['Cane of Kulemak'])
			expect(h.page.runMutationError).toContain('changed again')
		})
		it('loads owned Runs while the separate statistics source is unreachable', async () => {
			const h = await runsHarness()
			h.deps.stats.load = async () => { throw new Error('stats offline') }
			await h.page.selectScope('collective')
			await h.page.loadRuns(8)
			expect(h.page.runsView?.range).toBe('Runs 8–1 of 78, newest first')
		})
	})
