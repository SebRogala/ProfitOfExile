import { describe, expect, it, vi } from 'vitest'
import { createWidgetActions, type ActionElement } from './widget-actions'

function element(parentElement: ActionElement | null, dataset: ActionElement['dataset'] = {}, disabled = false): ActionElement {
	return { parentElement, dataset, matches: (selector) => selector === ':disabled' ? disabled
		: selector === '[data-action]' ? dataset.action !== undefined : dataset.widget !== undefined }
}
function panel(disabled = false) {
	const events: string[] = []
	const registry = createWidgetActions(() => false)
	registry.register('loot.confirmation', { interact: () => events.push('touch'), actions: { save: () => events.push('save'), 'loot.touch': () => {} } })
	const widget = element(null, { widget: 'loot.confirmation' })
	return { events, registry, widget, button: element(widget, { action: 'save' }, disabled) }
}

describe('widget action dispatch', () => {
	it('dispatches a direct button after the panel interaction', () => {
		const { registry, button, events } = panel()
		registry.dispatch(button)
		expect(events).toEqual(['touch', 'save'])
	})
	it('resolves the actionable wrapper from its child', () => {
		const { registry, button, events } = panel()
		registry.dispatch(element(button))
		expect(events).toEqual(['touch', 'save'])
	})
	it('touches without saving for a disabled action target', () => {
		const { registry, button, events } = panel(true)
		registry.dispatch(button)
		expect(events).toEqual(['touch'])
	})
	it('touches without saving for a disabled descendant of an actionable wrapper', () => {
		const { registry, button, events } = panel()
		registry.dispatch(element(button, {}, true))
		expect(events).toEqual(['touch'])
	})
	it('suppresses an action with a disabled intermediate ancestor', () => {
		const { registry, button, events } = panel()
		registry.dispatch(element(element(button, {}, true)))
		expect(events).toEqual(['touch'])
	})
	it('does not inspect disabled ancestors beyond the resolved target', () => {
		const { registry, widget, events } = panel()
		registry.dispatch(element(element(widget, {}, true), { action: 'save' }))
		expect(events).toEqual(['touch', 'save'])
	})
	it('does no widget interaction in config mode', () => {
		const perform = vi.fn()
		const registry = createWidgetActions(() => true)
		registry.register('loot.confirmation', { interact: perform, actions: { save: perform } })
		registry.dispatch(element(element(null, { widget: 'loot.confirmation' }), { action: 'save' }))
		expect(perform).not.toHaveBeenCalled()
	})
	it('touches the panel exactly once without a nested touch action', () => {
		const { registry, widget, events } = panel()
		registry.dispatch(element(widget, { action: 'loot.touch' }))
		expect(events).toEqual(['touch'])
	})
	it('ignores an unknown action after panel interaction', () => {
		const { registry, widget, events } = panel()
		registry.dispatch(element(widget, { action: 'unknown' }))
		expect(events).toEqual(['touch'])
	})
	it('ignores inherited object names as actions', () => {
		const { registry, widget, events } = panel()
		registry.dispatch(element(widget, { action: 'constructor' }))
		expect(events).toEqual(['touch'])
	})
	it('ignores a click outside an action', () => {
		const { registry, widget, events } = panel()
		registry.dispatch(element(widget))
		expect(events).toEqual([])
	})
	it('ignores a click outside any widget', () => {
		const { registry, events } = panel()
		registry.dispatch(element(null, { action: 'save' }))
		expect(events).toEqual([])
	})
	it('ignores a widget with no registration', () => {
		const { registry, events } = panel()
		registry.dispatch(element(element(null, { widget: 'unknown' }), { action: 'save' }))
		expect(events).toEqual([])
	})
	it('isolates identical action names by the nearest enclosing widget identity', () => {
		const { registry, events, button } = panel()
		registry.register('lab.comparator', { actions: { save: () => events.push('comparator') } })
		registry.dispatch(element(element(null, { widget: 'lab.comparator' }), { action: 'save' }))
		registry.dispatch(button)
		expect(events).toEqual(['comparator', 'touch', 'save'])
	})
	it('withdraws an unmounted widget registration', () => {
		const { registry, button, events } = panel()
		const remove = registry.register('loot.confirmation', { actions: { save: () => events.push('new') } })
		remove()
		registry.dispatch(button)
		expect(events).toEqual([])
	})
	it('keeps the replacement registration when an earlier registration cleans up', () => {
		const { registry, button, events } = panel()
		const remove = registry.register('loot.confirmation', { actions: { save: () => events.push('old') } })
		registry.register('loot.confirmation', { actions: { save: () => events.push('new') } })
		remove()
		registry.dispatch(button)
		expect(events).toEqual(['new'])
	})
})
