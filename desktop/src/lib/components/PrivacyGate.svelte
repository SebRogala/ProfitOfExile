<script lang="ts">
	/**
	 * The blocking Data & Privacy notice (POE-395, ADR-033). The Operator's
	 * 17:52 ruling keeps the full notice behind an in-app link. No Escape, no outside click;
	 * focus stays in the dialog. Continue accepts, Quit exits the app.
	 */
	import { untrack } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { noticeGate, type PrivacyState } from '$lib/privacy/gate.svelte';
	import { notice as bundledNotice, type Notice } from '$lib/privacy/notice';
	import PrivacyNoticeView from './PrivacyNoticeView.svelte';

	let {
		notice = bundledNotice,
		acceptedRevision: initialAccepted = undefined
	}: {
		notice?: Notice;
		/** Known accepted revision (tests, previews); omitted = asked from Rust. */
		acceptedRevision?: number | null;
	} = $props();

	// undefined = not yet known, null = never accepted.
	let acceptedRevision = $state<number | null | undefined>(untrack(() => initialAccepted));
	let acceptedHere = $state(false);
	let busy = $state(false);
	let fullOpen = $state(false);
	let dialog = $state<HTMLDivElement | null>(null);
	let noticeLink = $state<HTMLAnchorElement | null>(null);
	let backButton = $state<HTMLButtonElement | null>(null);

	$effect(() => {
		if (acceptedRevision !== undefined) return;
		invoke<PrivacyState>('get_privacy_state')
			.then((s) => { acceptedRevision = s.acceptedRevision; })
			.catch((e) => {
				console.error('[privacy] get_privacy_state failed:', e);
				stateFailed = true;
			});
	});

	// Blocks until acceptance is confirmed: while the state is pending, and
	// after a failed state request (then shown as a first run).
	let stateFailed = $state(false);
	let open = $derived(
		!noticeGate.accepted && !acceptedHere
			&& (acceptedRevision === undefined || acceptedRevision === null || acceptedRevision < notice.revision)
	);
	let pending = $derived(open && acceptedRevision === undefined && !stateFailed);
	let isChanges = $derived(open && acceptedRevision !== null && acceptedRevision !== undefined);

	$effect(() => {
		if (fullOpen) backButton?.focus();
		else noticeLink?.focus();
	});

	function focusables(): HTMLElement[] {
		if (!dialog) return [];
		return [...dialog.querySelectorAll<HTMLElement>('button:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])')];
	}

	$effect(() => {
		if (!open || !dialog) return;
		focusables()[0]?.focus();
		function onKeydown(e: KeyboardEvent) {
			if (e.key === 'Escape') {
				e.preventDefault();
				e.stopPropagation();
				return;
			}
			if (e.key !== 'Tab') return;
			const els = focusables();
			if (els.length === 0) return;
			const i = els.indexOf(document.activeElement as HTMLElement);
			e.preventDefault();
			const next = i === -1 ? 0 : (i + (e.shiftKey ? -1 : 1) + els.length) % els.length;
			els[next].focus();
		}
		window.addEventListener('keydown', onKeydown, true);
		return () => window.removeEventListener('keydown', onKeydown, true);
	});

	async function accept() {
		busy = true;
		try {
			await invoke('accept_privacy_revision', { revision: notice.revision });
			acceptedHere = true;
		} catch (e) {
			console.error('[privacy] accept_privacy_revision failed:', e);
		} finally {
			busy = false;
		}
	}

	function quit() {
		invoke('quit_app').catch((e) => console.error('[privacy] quit_app failed:', e));
	}
</script>

{#if open}
	<div class="privacy-backdrop">
		<div class="privacy-dialog" role="dialog" aria-modal="true" aria-labelledby="privacy-title" aria-busy={pending} bind:this={dialog}>
			<div class="header">
				<h3 id="privacy-title">Data & privacy</h3>
			</div>

			{#if !pending}
			<div class="body">
				{#if fullOpen}
					<button type="button" class="expand" bind:this={backButton} onclick={() => (fullOpen = false)}>Back</button>
					<PrivacyNoticeView {notice} />
				{:else}
					<p class="summary">{isChanges ? `Data & privacy updated (revision ${notice.revision}).` : 'ProfitOfExile reads your game screen on demand and sends game data to its server. No screenshot leaves your device.'}</p>
					<a class="expand" href="#privacy-notice" bind:this={noticeLink} onclick={(event) => { event.preventDefault(); fullOpen = true; }}>Data & privacy</a>
				{/if}
			</div>
			{/if}

			<div class="actions">
				<button type="button" class="btn quit" onclick={quit}>Quit</button>
				{#if !pending}
					<button type="button" class="btn continue" disabled={busy} onclick={accept}>Continue</button>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	.privacy-backdrop {
		position: fixed;
		inset: 0;
		z-index: 10000;
		background: rgba(0, 0, 0, 0.7);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.privacy-dialog {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		width: min(960px, calc(100vw - 32px));
		max-height: calc(100vh - 64px);
		display: flex;
		flex-direction: column;
		color: var(--text);
	}

	.header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.75rem 1rem;
		border-bottom: 1px solid var(--border);
	}

	h3 {
		margin: 0;
		font-size: 1rem;
	}

	.body {
		padding: 0.75rem 1rem;
		min-height: 0;
		overflow-y: auto;
		font-size: 0.85rem;
	}

	.summary {
		margin: 0.25rem 0;
		color: var(--text-muted);
	}

	.expand {
		display: inline-block;
		margin: 0.75rem 0;
		background: none;
		border: none;
		padding: 0;
		color: var(--accent);
		cursor: pointer;
		text-decoration: underline;
		font-size: 0.8rem;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		padding: 0.75rem 1rem;
		border-top: 1px solid var(--border);
	}

	.btn {
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 0.35rem 0.9rem;
		border-radius: 4px;
		font-size: 0.8rem;
		cursor: pointer;
	}

	.btn.continue {
		border-color: var(--accent);
		color: var(--accent);
	}

	.btn:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.btn:focus-visible, .expand:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}
</style>
