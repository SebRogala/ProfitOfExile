<script module lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { ssot, fetchSsot, type ReadHotkeySettings } from '$lib/stores/ssot.svelte';
	import { parseShortcut } from '$lib/boss-loot/native-adapters';
	import { keyboardShortcut, mouseShortcut, type ShortcutKeyInput, type ShortcutMouseInput } from '$lib/boss-loot/page-view';

	/** `read_hotkey::MAX_DELAY_MS` on the Rust side. */
	export const READ_DELAY_MAX_MS = 2000;

	export interface ReadHotkeyDeps {
		current: () => ReadHotkeySettings | null;
		call: (command: string, args: Record<string, unknown>) => Promise<unknown>;
		refresh: () => Promise<void>;
	}

	const nativeDeps: ReadHotkeyDeps = { current: () => ssot.readHotkey, call: invoke, refresh: () => fetchSsot() };

	export function createReadHotkeyEditor(deps: ReadHotkeyDeps = nativeDeps) {
		let listening = $state(false);
		let pending = $state(false);
		let error = $state('');

		async function save(change: { shortcut?: string; delayMs?: number }) {
			const current = deps.current();
			if (!current || pending) return;
			let settings: ReadHotkeySettings;
			try {
				settings = {
					shortcut: change.shortcut === undefined ? current.shortcut : parseShortcut(change.shortcut, current.shortcut),
					delayMs: change.delayMs ?? current.delayMs
				};
			} catch (e) {
				error = e instanceof Error ? e.message : `${e}`;
				return;
			}
			pending = true;
			try {
				await deps.call('read_hotkey_set_settings', { settings });
				error = '';
				await deps.refresh();
			} catch (e) {
				error = `${e}`;
			} finally {
				pending = false;
			}
		}

		return {
			get listening() { return listening; },
			get pending() { return pending; },
			get error() { return error; },
			listen() { if (deps.current() && !pending) listening = true; },
			async captureKey(input: ShortcutKeyInput) {
				if (input.key === 'Escape') { listening = false; return; }
				const binding = keyboardShortcut(input);
				if (!binding) return;
				listening = false;
				await save({ shortcut: binding });
			},
			async captureMouse(input: ShortcutMouseInput) {
				const binding = mouseShortcut(input);
				if (!binding) return;
				listening = false;
				await save({ shortcut: binding });
			},
			async setDelay(value: number) {
				if (!Number.isInteger(value) || value < 0 || value > READ_DELAY_MAX_MS) {
					error = `Delay must be 0–${READ_DELAY_MAX_MS} ms`;
					return;
				}
				await save({ delayMs: value });
			}
		};
	}
	export type ReadHotkeyEditor = ReturnType<typeof createReadHotkeyEditor>;
</script>

<script lang="ts">
	import ShortcutCapture from '$lib/boss-loot/ShortcutCapture.svelte';
	import { shortcutLabel } from '$lib/boss-loot/native-adapters';

	let { editor = createReadHotkeyEditor() }: { editor?: ReadHotkeyEditor } = $props();
	const settings = $derived(ssot.readHotkey);
</script>

<div class="setting-row">
	<span class="setting-label">Read screen</span>
	{#if settings}
		<ShortcutCapture value={shortcutLabel(settings.shortcut)} label="Read screen" keycaps listening={editor.listening} disabled={editor.pending}
			onlisten={() => editor.listen()} onkey={(event) => { void editor.captureKey(event); }} onmouse={(event) => { void editor.captureMouse(event); }} />
	{:else}
		<span class="setting-value">...</span>
	{/if}
</div>
<div class="setting-row">
	<span class="setting-label">Read delay</span>
	{#if settings}
		<input type="number" class="setting-input narrow" aria-label="Read delay" min="0" max={READ_DELAY_MAX_MS} step="1"
			value={settings.delayMs} disabled={editor.pending}
			onchange={(event) => { void editor.setDelay(Number(event.currentTarget.value)); }} />
		<span class="setting-value">ms</span>
	{:else}
		<span class="setting-value">...</span>
	{/if}
</div>
{#if editor.error}
	<div class="setting-row">
		<span class="setting-label"></span>
		<span class="setting-error">{editor.error}</span>
	</div>
{/if}

<style>
	/* SettingsPage's row rules are scoped to it; every row here follows one of its rows. */
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

	.setting-input {
		flex: 1;
		max-width: 100px;
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		padding: 0.25rem 0.4rem;
		border-radius: 4px;
		font-size: 0.75rem;
		font-family: 'Consolas', 'Courier New', monospace;
	}

	.setting-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.setting-error {
		color: var(--color-lab-red, #ef4444);
		font-size: 0.75rem;
	}
</style>
