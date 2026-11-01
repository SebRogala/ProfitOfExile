/** Structural DOM boundary: forwarding and native callers share one action owner. */
export interface ActionElement {
	dataset: { action?: string; widget?: string; index?: string; item?: string }
	parentElement: ActionElement | null
	matches: (selector: string) => boolean
}

export const ACTION_SELECTOR = '[data-action]'
export const WIDGET_ACTIONS = Symbol('widget-actions')
export type WidgetActionMap = Record<string, (target: ActionElement) => void>
export interface WidgetActions {
	actions: WidgetActionMap
	interact?: () => void
}

/** One registry per WidgetHost, keyed by the enclosing widget's identity. */
export function createWidgetActions(configMode: () => boolean) {
	const widgets = new Map<string, WidgetActions>()
	return {
		register(widget: string, actions: WidgetActions): () => void {
			widgets.set(widget, actions)
			return () => { if (widgets.get(widget) === actions) widgets.delete(widget) }
		},
		dispatch(hit: ActionElement | null): void {
			if (configMode()) return
			let target = hit
			while (target && !target.matches(ACTION_SELECTOR)) target = target.parentElement
			const action = target?.dataset.action
			if (!target || !action) return
			let owner: ActionElement | null = target
			while (owner && !owner.matches('[data-widget]')) owner = owner.parentElement
			const widget = owner?.dataset.widget
			const registration = widget ? widgets.get(widget) : undefined
			if (!registration) return
			// Panel interaction precedes suppression: disabled Save still resets its countdown.
			registration.interact?.()
			for (let element = hit; element; element = element.parentElement) {
				if (element.matches(':disabled')) return
				if (element === target) break
			}
			if (Object.hasOwn(registration.actions, action)) registration.actions[action](target)
		}
	}
}

export type WidgetActionRegistry = ReturnType<typeof createWidgetActions>
