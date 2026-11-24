<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { ssot, fetchSsot } from '$lib/stores/ssot.svelte';
	import ShortcutCapture from '$lib/boss-loot/ShortcutCapture.svelte';
	import { parseShortcut, shortcutLabel } from '$lib/boss-loot/native-adapters';
	import { keyboardShortcut, mouseShortcut, type ShortcutKeyInput, type ShortcutMouseInput } from '$lib/boss-loot/page-view';

	const settings = $derived(ssot.exchangeRead);
	let listening = $state(false);
	let pending = $state(false);
	let error = $state('');

	async function save(binding: string) {
		const current = settings;
		if (!current || pending) return;
		pending = true;
		try {
			await invoke('exchange_read_set_settings', {
				settings: { shortcut: parseShortcut(binding, current.shortcut) }
			});
			await fetchSsot();
			error = '';
		} catch (e) {
			error = e instanceof Error ? e.message : `${e}`;
		} finally {
			pending = false;
		}
	}

	function captureKey(input: ShortcutKeyInput) {
		if (input.key === 'Escape') { listening = false; return; }
		const binding = keyboardShortcut(input);
		if (!binding) return;
		listening = false;
		void save(binding);
	}

	function captureMouse(input: ShortcutMouseInput) {
		const binding = mouseShortcut(input);
		if (!binding) return;
		listening = false;
		void save(binding);
	}
</script>

<div class="setting-row">
	<span class="setting-label">Read exchange</span>
	{#if settings}
		<ShortcutCapture value={shortcutLabel(settings.shortcut)} label="Read exchange" keycaps {listening} disabled={pending}
			onlisten={() => { if (!pending) listening = true; }} onkey={captureKey} onmouse={captureMouse} />
	{:else}
		<span class="setting-value">...</span>
	{/if}
</div>
{#if error}
	<div class="setting-row">
		<span class="setting-label"></span>
		<span class="setting-error">{error}</span>
	</div>
{/if}

<style>
	.setting-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.4rem 0;
		min-height: 32px;
		border-top: 1px solid rgba(255, 255, 255, 0.05);
	}

	.setting-label {
		min-width: 140px;
		flex-shrink: 0;
		font-size: 0.85rem;
		color: var(--text);
	}

	.setting-value {
		flex: 1;
		font-size: 0.8rem;
		color: var(--text-muted);
	}

	.setting-error {
		color: var(--color-lab-red, #ef4444);
		font-size: 0.75rem;
	}
</style>
