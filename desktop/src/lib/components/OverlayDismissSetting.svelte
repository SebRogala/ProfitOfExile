<script module lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { ssot, fetchSsot, type BossLootShortcut, type DismissSettings, type OverlayDismiss } from '$lib/stores/ssot.svelte';
	import { parseShortcut, shortcutLabel } from '$lib/boss-loot/native-adapters';
	import { keyboardShortcut, mouseShortcut, type ShortcutKeyInput, type ShortcutMouseInput } from '$lib/boss-loot/page-view';
	import { WIDGETS } from '$lib/overlay/widgets/widget-registry';

	export interface OverlayDismissDeps {
		current(): DismissSettings | null;
		call(command: string, args: Record<string, unknown>): Promise<unknown>;
		refresh(): Promise<void>;
	}
	const nativeDeps: OverlayDismissDeps = { current: () => ssot.overlayDismiss, call: invoke, refresh: () => fetchSsot('write') };
	const timeoutOptions = [0, 5, 10, 20, 30];
	const timeoutLabel = (seconds: number) => seconds === 0 ? 'Off' : `${seconds} s`;
	function dismissShortcutLabel(shortcut: BossLootShortcut): string {
		return shortcut.key === 27 ? shortcutLabel(shortcut).replace('Key 27', 'Esc') : shortcutLabel(shortcut);
	}

	export function createOverlayDismissEditor(deps: OverlayDismissDeps = nativeDeps) {
		let listening = $state<string | null>(null);
		let pending = $state(false);
		let error = $state('');
		const widgets = WIDGETS.filter(widget => widget.dismiss?.timeout || widget.dismiss?.key);
		function overlay(id: string): OverlayDismiss | null {
			const settings = deps.current();
			if (!settings) return null;
			return Object.hasOwn(settings.overlays, id) ? settings.overlays[id] : { timeout: 'inherit', shortcut: 'inherit' };
		}
		async function save(change: (settings: DismissSettings) => DismissSettings) {
			const current = deps.current();
			if (!current || pending) return;
			pending = true;
			try {
				// SSOT also carries transient pending generations; only settings cross this boundary.
				const settings = change({ timeoutSeconds: current.timeoutSeconds, shortcut: current.shortcut, overlays: { ...current.overlays } });
				await deps.call('overlay_dismiss_set_settings', { settings });
				await deps.refresh();
				error = '';
			} catch (cause) {
				error = cause instanceof Error ? cause.message : `${cause}`;
			} finally {
				pending = false;
			}
		}
		async function saveShortcut(shortcut: BossLootShortcut, id?: string) {
			await save(settings => id ? { ...settings, overlays: { ...settings.overlays, [id]: { ...overlay(id)!, shortcut: { override: shortcut } } } } : { ...settings, shortcut });
		}
		async function capture(binding: string | null, escape?: ShortcutKeyInput) {
			if (!listening || pending || !deps.current() || (!binding && !escape)) return;
			try {
				const shortcut = escape ? { key: 27, shift: escape.shiftKey, ctrl: escape.ctrlKey, alt: escape.altKey }
					: parseShortcut(binding!, deps.current()!.shortcut);
				const id = listening === 'global' ? undefined : listening;
				listening = null;
				await saveShortcut(shortcut, id);
			} catch (cause) {
				error = cause instanceof Error ? cause.message : `${cause}`;
			}
		}
		return {
			widgets,
			get settings() { return deps.current(); },
			get listening() { return listening; },
			get pending() { return pending; },
			get error() { return error; },
			overlay,
			listen(id?: string) { if (deps.current() && !pending) listening = id ?? 'global'; },
			cancel() { listening = null; },
			async captureKey(input: ShortcutKeyInput) {
				if (input.metaKey) return;
				await capture(keyboardShortcut(input), input.key === 'Escape' ? input : undefined);
			},
			async captureMouse(input: ShortcutMouseInput) { if (!input.metaKey) await capture(mouseShortcut(input)); },
			async reset() {
				listening = null;
				await saveShortcut({ key: 27, shift: false, ctrl: false, alt: false });
			},
			async setTimeout(value: number | 'inherit', id?: string) {
				listening = null;
				await save(settings => id ? { ...settings, overlays: { ...settings.overlays, [id]: { ...overlay(id)!, timeout: value === 'inherit' ? 'inherit' : { override: value } } } }
					: { ...settings, timeoutSeconds: value as number });
			},
			async inheritShortcut(id: string) {
				listening = null;
				await save(settings => ({ ...settings, overlays: { ...settings.overlays, [id]: { ...overlay(id)!, shortcut: 'inherit' } } }));
			}
		};
	}
	export type OverlayDismissEditor = ReturnType<typeof createOverlayDismissEditor>;
</script>

<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	let { editor = createOverlayDismissEditor() }: { editor?: OverlayDismissEditor } = $props();
	let root: HTMLDivElement | undefined = $state();
	const settings = $derived(editor.settings);
	const disabled = $derived(!settings || editor.pending);

	$effect(() => {
		let consumedMouse: number | null = null;
		const consume = (event: Event) => { event.preventDefault(); event.stopImmediatePropagation(); };
		const available = () => editor.listening && !disabled && root?.getClientRects().length;
		const cancelTarget = (event: Event) => event.target instanceof Element && event.target.closest('[data-dismiss-cancel]');
		const key = (event: KeyboardEvent) => {
			if (!available() || event.repeat || (cancelTarget(event) && ['Enter', ' '].includes(event.key))) return;
			consume(event);
			void editor.captureKey(event);
		};
		const mouse = (event: MouseEvent) => {
			if (!available() || cancelTarget(event) || event.button < 0 || event.button > 4) return;
			consumedMouse = event.button;
			consume(event);
			void editor.captureMouse(event);
		};
		const click = (event: MouseEvent) => {
			if (consumedMouse !== event.button) return;
			consume(event);
			consumedMouse = null;
		};
		const mouseUp = (event: MouseEvent) => {
			// Chromium navigated Back on release even after the captured press was consumed.
			if (consumedMouse === event.button) consume(event);
		};
		window.addEventListener('keydown', key, true);
		window.addEventListener('mousedown', mouse, true);
		window.addEventListener('mouseup', mouseUp, true);
		window.addEventListener('click', click, true);
		window.addEventListener('auxclick', click, true);
		window.addEventListener('contextmenu', click, true);
		return () => {
			window.removeEventListener('keydown', key, true);
			window.removeEventListener('mousedown', mouse, true);
			window.removeEventListener('mouseup', mouseUp, true);
			window.removeEventListener('click', click, true);
			window.removeEventListener('auxclick', click, true);
			window.removeEventListener('contextmenu', click, true);
		};
	});
</script>

{#snippet shortcutControl(id: string, label: string, value: BossLootShortcut | null)}
	<Button aria-label={label} disabled={disabled} onclick={() => editor.listen(id === 'global' ? undefined : id)}>
		{editor.listening === id ? 'Listening…' : value ? dismissShortcutLabel(value) : '…'}
	</Button>
	{#if editor.listening === id}
		<Button data-dismiss-cancel aria-label="Cancel {label}" onclick={() => editor.cancel()}>Cancel</Button>
	{/if}
{/snippet}

<div class="dismiss-settings" bind:this={root}>
	<h3>Overlay dismissal</h3>
	<div class="setting-row">
		<span class="setting-label">Global timeout</span>
		<select aria-label="Global dismissal timeout" value={settings?.timeoutSeconds ?? ''} {disabled}
			onchange={(event) => { const value = Number(event.currentTarget.value); event.currentTarget.value = `${settings?.timeoutSeconds ?? ''}`; void editor.setTimeout(value); }}>
			{#if !settings}<option value="">…</option>{/if}
			{#each timeoutOptions as seconds}<option value={seconds}>{timeoutLabel(seconds)}</option>{/each}
		</select>
	</div>
	<div class="setting-row">
		<span class="setting-label">Global shortcut</span>
		{@render shortcutControl('global', 'Global dismissal shortcut', settings?.shortcut ?? null)}
		<Button {disabled} onclick={() => { void editor.reset(); }}>Reset to Esc</Button>
	</div>
	{#each editor.widgets as widget (widget.id)}
		{@const row = editor.overlay(widget.id)}
		<div class="widget-row" data-dismiss-widget={widget.id}>
			<span class="widget-label">{widget.label}</span>
			{#if widget.dismiss?.timeout}
				<label>Timeout
					<select aria-label="{widget.label} dismissal timeout" {disabled} value={row?.timeout === 'inherit' ? 'inherit' : row?.timeout.override ?? ''}
						onchange={(event) => { const value = event.currentTarget.value; event.currentTarget.value = row?.timeout === 'inherit' ? 'inherit' : `${row?.timeout.override ?? ''}`; void editor.setTimeout(value === 'inherit' ? value : Number(value), widget.id); }}>
						{#if !settings}<option value="">…</option>{/if}
						<option value="inherit">Use global ({settings ? timeoutLabel(settings.timeoutSeconds) : '…'})</option>
						{#each timeoutOptions as seconds}<option value={seconds}>{timeoutLabel(seconds)}</option>{/each}
					</select>
				</label>
			{/if}
			{#if widget.dismiss?.key}
				<label>Shortcut
					<select aria-label="{widget.label} dismissal shortcut mode" {disabled} value={editor.listening === widget.id ? 'override' : row?.shortcut === 'inherit' ? 'inherit' : row ? 'override' : ''}
						onchange={(event) => { const value = event.currentTarget.value; event.currentTarget.value = row?.shortcut === 'inherit' ? 'inherit' : 'override'; if (value === 'inherit') { void editor.inheritShortcut(widget.id); } else editor.listen(widget.id); }}>
						{#if !settings}<option value="">…</option>{/if}
						<option value="inherit">Use global ({settings ? dismissShortcutLabel(settings.shortcut) : '…'})</option>
						<option value="override">Override</option>
					</select>
				</label>
				{#if row && (row.shortcut !== 'inherit' || editor.listening === widget.id)}
					{@render shortcutControl(widget.id, `${widget.label} dismissal shortcut`, row.shortcut === 'inherit' ? null : row.shortcut.override)}
				{/if}
			{/if}
		</div>
	{/each}
	{#if editor.error}<div class="setting-error" role="alert">{editor.error}</div>{/if}
</div>

<style>
	h3 { font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--text-muted); margin: 0.75rem 0 0.15rem; }
	.setting-row, .widget-row { display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem; padding: 0.4rem 0; min-height: 32px; border-top: 1px solid rgba(255, 255, 255, 0.05); }
	.setting-label, .widget-label { min-width: 140px; flex-shrink: 0; font-size: 0.85rem; color: var(--text); }
	.widget-label { width: 100%; font-size: 0.8rem; }
	label { display: inline-flex; align-items: center; gap: 0.5rem; font-size: 0.75rem; color: var(--text-muted); }
	select { background: var(--bg); border: 1px solid var(--border); color: var(--text); padding: 0.25rem 0.4rem; border-radius: 4px; font: inherit; font-size: 0.75rem; }
	select:focus { outline: 1px solid var(--accent); }
	select:disabled { opacity: 0.35; }
	.setting-error { color: var(--color-lab-red, #ef4444); font-size: 0.75rem; padding: 0.4rem 0; }
</style>
