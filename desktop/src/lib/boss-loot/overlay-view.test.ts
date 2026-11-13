import { describe, expect, it, vi } from 'vitest'
import source from './__fixtures__/boss-loot.json'
import { createFixtureCapture, createFixtureModuleSettings, createFixtureProfiles, createFixtureStats } from './fixture-adapters'
import type { BossStats, LootLayout, StatsScope, SubmitOutcome } from './contracts'
import { buildOverlayView, changeSelection, createOverlayClock, type OverlayInput } from './overlay-view'

const manualCategory = { id: 'category:test:1:manual', name: 'Manual category', base: 'any equipment', wikiRate: '—' }

async function input(layout: LootLayout = 'grid', scope: StatsScope = 'own'): Promise<OverlayInput & { stats: BossStats }> {
	const [profile] = await createFixtureProfiles(source.profiles).load()
	return {
		profile, capture: (await createFixtureCapture(source.capture).load())!,
		stats: await createFixtureStats(source.stats).load(profile, scope),
		notCounted: (await createFixtureModuleSettings(source.moduleSettings).load()).notCounted,
		layout, scope, autoSaveSeconds: 10, seconds: 7
	}
}

async function testProfile(boss: string): Promise<OverlayInput> {
	const profile = (await createFixtureProfiles(source.testProfiles).load()).find((p) => p.boss === boss)!
	return {
		...await input(), profile, capture: {
			...structuredClone(profile.capture!), boss, run: 1, seed: null,
			attribution: { area: profile.area, level: 83, profileKey: profile.profileKey },
			readLines: [], unknownLines: []
		}, stats: { runs: 0, devices: 0, nextRun: 1, items: {} }
	}
}

for (const layout of ['grid', 'list'] as const) {
	describe(layout, () => {
		it('preserves uniform fixture provenance', async () => {
			const view = buildOverlayView(await input(layout))
			expect(view.groups[0].items[0]).toMatchObject({ rate: '31%', provenance: '24/78', listProvenance: '' })
			expect(view.rateHeader).toBe('= drops / 78 runs')
		})
		for (const scope of ['own', 'collective'] as const) {
			it(`shows each eligible denominator for mixed ${scope} data`, async () => {
				const state = await input(layout, scope)
				state.stats.items['Cinderswallow Urn'] = { drops: 2, eligibleRuns: 5 }
				const view = buildOverlayView(state)
				expect(view.rateHeader).toBe('= drops / eligible runs')
				expect(view.groups[0].items[0]).toMatchObject({ rate: '40%', provenance: '2/5 eligible runs', listProvenance: '2/5 eligible runs' })
			})
			it(`shows zero eligible runs without a wiki substitute for ${scope}`, async () => {
				const state = await input(layout, scope)
				state.stats.items['Cinderswallow Urn'] = { drops: 0, eligibleRuns: 0 }
				expect(buildOverlayView(state).groups[0].items[0]).toMatchObject({ rate: '—', provenance: '0/0 eligible runs' })
			})
			it(`preserves measured zero drops for ${scope}`, async () => {
				const state = await input(layout, scope)
				state.stats.items['Cinderswallow Urn'] = { drops: 0, eligibleRuns: 78 }
				expect(buildOverlayView(state).groups[0].items[0]).toMatchObject({ rate: scope === 'own' ? '0%' : '—', provenance: '0/78' })
			})
		}
		it('uses wiki estimates only with zero boss runs', async () => {
			const state = await input(layout)
			state.stats = { runs: 0, devices: 0, nextRun: 1, items: {} }
			const view = buildOverlayView(state)
			expect(view.runsNote).toBe('No runs logged for this boss yet: rates show the wiki estimate.')
			expect(view.groups[0].items[0]).toMatchObject({ rate: '', provenance: 'wiki 30%' })
		})
		it('disables current not-counted items despite historical drops', async () => {
			const view = buildOverlayView(await input(layout))
			expect(view.extra.find((item) => item.id === 'Communion Support')).toMatchObject({ disabled: true, rate: '—', provenance: '', selected: false, title: 'Not counted: change on the Boss loot page' })
		})
		it('pauses shared-base ambiguity without selecting a candidate', async () => {
			const state = await testProfile('maven')
			state.layout = layout
			const view = buildOverlayView(state)
			expect(view.groups[0].items.filter((item) => item.ambiguous).map((item) => item.selected)).toEqual([false, false, false])
			expect(view.badge).toBe('Paused: which Cloth Belt?')
			expect(view.barPercent).toBe(0)
		})
		it('keeps two group statuses independent', async () => {
			const state = await testProfile('fear')
			state.layout = layout
			state.capture.picks[state.profile.groups[1].id] = []
			expect(buildOverlayView(state).groups.map((group) => group.status)).toEqual(['1 picked', 'pick 1'])
		})
		it('omits guaranteed sections for the quantity profile', async () => {
			const state = await testProfile('blackstar')
			state.layout = layout
			const view = buildOverlayView(state)
			expect(view.groups).toEqual([])
			expect(view.extraMessage).toBe('5 items')
		})
	})
}

it('shows unmatched OCR lines', async () => {
	expect(buildOverlayView(await input()).unknownLine).toBe('Not in table: Spidersilk Robe')
})
it('omits the unmatched line when there are no unknowns', async () => {
	const state = await input()
	state.capture.unknownLines = []
	expect(buildOverlayView(state).unknownLine).toBe('')
})
it('empties the countdown bar below the group minimum', async () => {
	const state = await input()
	state.capture.picks.unique = []
	expect(buildOverlayView(state)).toMatchObject({ badge: 'Paused: check guaranteed', barPercent: 0, canSave: false })
})
it('reports an overfull group', async () => {
	const state = await input()
	state.capture.picks.unique = ['Cinderswallow Urn', 'Spinehail']
	expect(buildOverlayView(state).groups[0].status).toBe('2 picked: max 1')
})
it('renders no countdown when auto-save is Off', async () => {
	const state = await input()
	state.autoSaveSeconds = 0
	expect(buildOverlayView(state)).toMatchObject({ badge: '', barPercent: 0, canSave: true })
})
it('projects the current countdown fraction', async () => {
	expect(buildOverlayView(await input())).toMatchObject({ badge: 'Saving in 7 s', barPercent: 70 })
})
it('shows a stepper only for a selected quantity item', async () => {
	const state = await testProfile('blackstar')
	const view = buildOverlayView(state)
	expect(view.extra.filter((item) => item.showQuantity).map((item) => item.quantity)).toEqual([1, 3])
})
it('omits removed Catarina entries from the overlay', async () => {
	expect(buildOverlayView(await input()).extra.map((item) => item.name)).toEqual([
		"Zorath's Eye of the Endless", 'Veiled Exalted Orb', 'Communion Support', "Nook's Crown"
	])
})
it('always shows the category stepper at zero', async () => {
	const state = await input()
	state.profile.categories = [manualCategory]
	expect(buildOverlayView(state).extra.at(-1)).toMatchObject({ category: true, showQuantity: true, quantity: 0 })
})
it.each([
	[{ state: 'saving' }, 'Saving', ''],
	[{ state: 'unreachable' }, 'Not saved', 'Server unreachable. This run was not recorded.'],
	[{ state: 'conflict' }, 'Paused: check guaranteed', '']
] as const)('projects %j submission copy', async (outcome, badge, message) => {
	expect(buildOverlayView({ ...await input(), outcome })).toMatchObject({ badge, message, canSave: false, barPercent: 0 })
})
it('uses the server-confirmed run number on success', async () => {
	const outcome: SubmitOutcome = { state: 'saved', run: { boss: 'catarina', profileKey: 'Normal', profileVersion: 1, id: 'row', league: 'fixture-only', run: 80, version: 1, seed: null, date: null, items: { quantities: {}, counted: [] } }, droppedPositiveKeys: [] }
	expect(buildOverlayView({ ...await input(), outcome })).toMatchObject({ badge: 'Saved', message: 'Saved as run 80.', canSave: false })
})
it('names excluded merge positives without changing selected items', async () => {
	const state = await input()
	delete state.capture.extra[state.profile.extra[0].id]
	state.outcome = { state: 'saved', run: { ...state.profile, id: 'row', league: 'fixture-only', run: 79, version: 2, seed: null, date: null, items: { quantities: { 'Cinderswallow Urn': 1 }, counted: ['Cinderswallow Urn'] } }, droppedPositiveKeys: [state.profile.extra[0].id] }
	const view = buildOverlayView(state)
	expect(view.excludedMessage).toBe("Not added (not counted in the original run): Zorath's Eye of the Endless.")
	expect(view.extra.find((item) => item.id === state.profile.extra[0].id)?.selected).toBe(false)
	expect(view.groups[0].items.filter((item) => item.selected).map((item) => item.id)).toEqual(['Cinderswallow Urn'])
})
it('offers force-new for a pending same-seed merge', async () => {
	expect(buildOverlayView({ ...await input(), previousRun: 79 }).mergeMessage).toBe('Same seed as run 79: added to it.')
})
it('moves a max-one selection to the newly picked item', async () => {
	const state = await input()
	expect(changeSelection(state.profile, state.capture, [], { type: 'toggle', id: 'Spinehail' }).picks.unique).toEqual(['Spinehail'])
})
it('clears a picked guaranteed item', async () => {
	const state = await input()
	expect(changeSelection(state.profile, state.capture, [], { type: 'toggle', id: 'Cinderswallow Urn' }).picks.unique).toEqual([])
})
it('retains the OCR tag after the read item is deselected', async () => {
	const state = await input()
	const readSelection = structuredClone(state.capture)
	state.capture.picks.unique = []
	expect(buildOverlayView({ ...state, readSelection }).groups[0].items[0].tag).toBe('read')
})
it('does not label a manual replacement as OCR read', async () => {
	const state = await input()
	const readSelection = structuredClone(state.capture)
	state.capture.picks.unique = ['Spinehail']
	expect(buildOverlayView({ ...state, readSelection }).groups[0].items[1].tag).toBe('')
})
it('resolves ambiguity only when a candidate is chosen', async () => {
	const state = await testProfile('maven')
	expect(changeSelection(state.profile, state.capture, [], { type: 'toggle', id: state.capture.ambiguous!.ids[0] }).ambiguous).toBe(null)
})
it('rejects a disabled item selection', async () => {
	const state = await input()
	expect(changeSelection(state.profile, state.capture, state.notCounted, { type: 'toggle', id: 'Communion Support' }).extra['Communion Support']).toBeUndefined()
})
it('floors decremented quantities at zero', async () => {
	const state = await input()
	const id = manualCategory.id
	state.profile.categories = [manualCategory]
	expect(changeSelection(state.profile, state.capture, [], { type: 'quantity', id, delta: -1 }).categories[id]).toBe(0)
})
it('ignores an unknown item action', async () => {
	const state = await input()
	expect(changeSelection(state.profile, state.capture, [], { type: 'toggle', id: 'constructor' })).toEqual(state.capture)
})

describe('native overlay authority', () => {
	it('keeps a native rejected confirmation paused even when the visible selection is valid', async () => {
		const state = await input()
		state.native = { phase: 'rejected', deadlineMs: null, error: 'Merge target changed; save as new run or discard', run: 79, droppedPositiveKeys: [] }
		const view = buildOverlayView(state)
		expect(view.canSave).toBe(false)
		expect(view.barPercent).toBe(0)
		expect(view.message).toContain('Merge target changed')
	})
	it('keeps a valid native confirmation with no deadline paused', async () => {
		const state = await input()
		state.native = { phase: 'open', deadlineMs: null, error: 'Check guaranteed groups', run: 79, droppedPositiveKeys: [] }
		expect(buildOverlayView(state).badge).toBe('Paused: check guaranteed')
	})
})

describe('grouped overlay counts', () => {
	it.each(['auto', 'space', 'comma', 'none'] as const)('matches the page formatter for %s', async (format) => {
		const { ssot } = await import('$lib/stores/ssot.svelte')
		const { groupNumber } = await import('$lib/stores/number-format.svelte')
		const prior = ssot.bossLoot
		ssot.bossLoot = { ...prior!, numberFormat: format }
		try {
			const state = await input()
			state.stats.runs = 12000
			state.stats.items['Cinderswallow Urn'] = { drops: 1234, eligibleRuns: 10000 }
			const view = buildOverlayView(state)
			expect(view.groups[0].items[0].provenance).toBe(`${groupNumber('1234')}/${groupNumber('10000')} eligible runs`)
			expect(view.runsNote).toBe(`Chances from ${groupNumber('12000')} runs`)
		} finally { ssot.bossLoot = prior }
	})
})

it('keeps native unreachable feedback visible when statistics have never loaded', async () => {
	const state: OverlayInput = await input()
	state.stats = null
	state.outcome = { state: 'unreachable' }
	const view = buildOverlayView(state)
	expect(view.message).toBe('Server unreachable. This run was not recorded.')
	expect(view.groups[0].items[0].rate).toBe('—')
	expect(view.groups[0].items[0].provenance).toBe('')
	expect(view.runsNote).toBe('')
})

describe('native countdown display clock', () => {
	it('ticks whole seconds and the bar between native snapshots', async () => {
		const state = await input()
		state.native = { phase: 'open', deadlineMs: 15000, error: null, run: 79, droppedPositiveKeys: [] }
		vi.useFakeTimers()
		let view = buildOverlayView(state)
		const clock = createOverlayClock((nowMs) => { view = buildOverlayView(state, nowMs) })
		clock.update(state, 5000)
		try {
			expect(view).toMatchObject({ badge: 'Saving in 10 s', barPercent: 100 })
			for (let seconds = 9; seconds >= 0; seconds--) {
				vi.advanceTimersByTime(1000)
				expect(view).toMatchObject({ badge: `Saving in ${seconds} s`, barPercent: seconds * 10 })
			}
			vi.advanceTimersByTime(2000)
			expect(view).toMatchObject({ badge: 'Saving in 0 s', barPercent: 0, canSave: true })
		} finally { clock.destroy(); vi.useRealTimers() }
	})
	it('stops delivering display ticks when its running owner is destroyed', async () => {
		const state = await input()
		state.native = { phase: 'open', deadlineMs: 15000, error: null, run: 79, droppedPositiveKeys: [] }
		vi.useFakeTimers()
		const render = vi.fn()
		const clock = createOverlayClock(render)
		clock.update(state, 5000)
		vi.advanceTimersByTime(1000)
		expect(render).toHaveBeenLastCalledWith(6000)
		clock.destroy()
		render.mockClear()
		try {
			vi.advanceTimersByTime(3000)
			expect(render).not.toHaveBeenCalled()
			expect(vi.getTimerCount()).toBe(0)
		} finally { vi.useRealTimers() }
	})
	it('uses elapsed time when a display callback is delayed', async () => {
		const state = await input()
		state.native = { phase: 'open', deadlineMs: 15000, error: null, run: 79, droppedPositiveKeys: [] }
		vi.useFakeTimers()
		const clock = vi.spyOn(performance, 'now').mockReturnValue(100)
		let view = buildOverlayView(state)
		const owner = createOverlayClock((nowMs) => { view = buildOverlayView(state, nowMs) })
		owner.update(state, 5000)
		try {
			clock.mockReturnValue(3700)
			vi.advanceTimersByTime(1000)
			expect(view).toMatchObject({ badge: 'Saving in 7 s', barPercent: 70 })
		} finally { owner.destroy(); clock.mockRestore(); vi.useRealTimers() }
	})
	it.each(['saving', 'saved', 'unreachable', 'rejected', 'closed'])('stops counting in native phase %s', async (phase) => {
		const state = await input()
		state.native = { phase, deadlineMs: 15000, error: null, run: 79, droppedPositiveKeys: [] }
		expect(buildOverlayView(state, 5000)).toMatchObject({ counting: false, barPercent: 0 })
	})
	it('stops counting when the native deadline is paused', async () => {
		const state = await input()
		state.native = { phase: 'open', deadlineMs: null, error: 'Check guaranteed groups', run: 79, droppedPositiveKeys: [] }
		expect(buildOverlayView(state, 5000)).toMatchObject({ counting: false, badge: 'Paused: check guaranteed', barPercent: 0 })
	})
	it('preserves fixture seconds when a local clock is supplied', async () => {
		expect(buildOverlayView(await input(), 5000)).toMatchObject({ badge: 'Saving in 7 s', barPercent: 70 })
	})
})

describe('overlay clock owner lifecycle', () => {
	it('preserves second boundaries across an off-second SSOT replacement', async () => {
		let state: OverlayInput = await input()
		state.native = { phase: 'open', deadlineMs: 15000, error: null, run: 79, droppedPositiveKeys: [] }
		vi.useFakeTimers()
		let view = buildOverlayView(state)
		const clock = createOverlayClock((nowMs) => { view = buildOverlayView(state, nowMs) })
		try {
			clock.update(state, 5000)
			vi.advanceTimersByTime(2000)
			expect(view).toMatchObject({ badge: 'Saving in 8 s', barPercent: 80 })
			vi.advanceTimersByTime(500)
			state = { ...state, native: { ...state.native! } }
			clock.update(state, 7500)
			vi.advanceTimersByTime(500)
			expect(view).toMatchObject({ badge: 'Saving in 7 s', barPercent: 70 })
			vi.advanceTimersByTime(1000)
			expect(view).toMatchObject({ badge: 'Saving in 6 s', barPercent: 60 })
			expect(vi.getTimerCount()).toBe(1)
		} finally { clock.destroy(); vi.useRealTimers() }
	})

	it('retains elapsed time when a scope update reuses the same snapshot timestamp', async () => {
		let state: OverlayInput = await input()
		state.native = { phase: 'open', deadlineMs: 15000, error: null, run: 79, droppedPositiveKeys: [] }
		vi.useFakeTimers()
		let view = buildOverlayView(state)
		const clock = createOverlayClock((nowMs) => { view = buildOverlayView(state, nowMs) })
		try {
			clock.update(state, 5000)
			vi.advanceTimersByTime(2500)
			state = { ...state, scope: 'collective' }
			clock.update(state, 5000)
			expect(view).toMatchObject({ badge: 'Saving in 8 s', barPercent: 80 })
			vi.advanceTimersByTime(500)
			expect(view).toMatchObject({ badge: 'Saving in 7 s', barPercent: 70 })
		} finally { clock.destroy(); vi.useRealTimers() }
	})

	it.each(['paused', 'saving', 'saved', 'unreachable', 'rejected', 'closed', 'off', 'fixture', 'absent', 'no snapshot'])('stops the running clock on %s and resumes from a fresh deadline', async (transition) => {
		const running: OverlayInput = await input()
		running.native = { phase: 'open', deadlineMs: 15000, error: null, run: 79, droppedPositiveKeys: [] }
		let state: OverlayInput | null = running
		vi.useFakeTimers()
		const render = vi.fn((nowMs: number | undefined) => state ? buildOverlayView(state, nowMs) : null)
		const clock = createOverlayClock(render)
		try {
			clock.update(state, 5000)
			vi.advanceTimersByTime(1000)
			expect(render.mock.results.at(-1)?.value).toMatchObject({ badge: 'Saving in 9 s', barPercent: 90 })
			state = { ...running, native: { ...running.native! } }
			if (transition === 'paused') { state.native!.deadlineMs = null; state.native!.error = 'Check guaranteed groups' }
			else if (transition === 'off') state.autoSaveSeconds = 0
			else if (transition === 'fixture') delete state.native
			else if (transition === 'absent') state = null
			else if (transition !== 'no snapshot') state.native!.phase = transition
			clock.update(state, transition === 'no snapshot' ? undefined : 6000)
			const stopped = render.mock.results.at(-1)?.value
			if (transition === 'paused') expect(stopped).toMatchObject({ badge: 'Paused: check guaranteed', barPercent: 0 })
			expect(vi.getTimerCount()).toBe(0)
			render.mockClear()
			vi.advanceTimersByTime(4000)
			expect(render).not.toHaveBeenCalled()
			state = { ...running, native: { ...running.native!, deadlineMs: 20000 } }
			clock.update(state, 10000)
			expect(render.mock.results.at(-1)?.value).toMatchObject({ badge: 'Saving in 10 s', barPercent: 100 })
			vi.advanceTimersByTime(1000)
			expect(render.mock.results.at(-1)?.value).toMatchObject({ badge: 'Saving in 9 s', barPercent: 90 })
			expect(vi.getTimerCount()).toBe(1)
		} finally { clock.destroy(); vi.useRealTimers() }
	})
})
