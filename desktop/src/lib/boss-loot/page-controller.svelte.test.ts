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
