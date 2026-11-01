<script lang="ts">
	import { getContext } from 'svelte';
	import { WIDGET_ACTIONS, type WidgetActionMap, type WidgetActionRegistry } from './widget-actions';

	let { widget, actions }: { widget: string; actions: WidgetActionMap } = $props();
	const registry = getContext<WidgetActionRegistry>(WIDGET_ACTIONS);
	$effect(() => registry.register(widget, { actions }));
</script>
