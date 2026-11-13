<script lang="ts">
	/**
	 * The blocking Data & Privacy notice (POE-395, ADR-033): first-run notice
	 * while no revision is accepted, the changes since the accepted revision
	 * while it is older than the bundled one. No Escape, no outside click;
	 * focus stays in the dialog. Continue accepts, Quit exits the app.
	 */
	import { untrack } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { noticeGate, type PrivacyState } from '$lib/privacy/gate.svelte';
	import { notice as bundledNotice, changesSince, type Notice } from '$lib/privacy/notice';

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

	$effect(() => {
		if (acceptedRevision !== undefined) return;
		invoke<PrivacyState>('get_privacy_state')
			.then((s) => { acceptedRevision = s.acceptedRevision; })
			.catch((e) => console.error('[privacy] get_privacy_state failed:', e));
	});

	let open = $derived(
		!noticeGate.accepted && !acceptedHere && acceptedRevision !== undefined
			&& (acceptedRevision === null || acceptedRevision < notice.revision)
	);
	let isChanges = $derived(open && acceptedRevision !== null && acceptedRevision !== undefined);
	let changes = $derived(isChanges ? changesSince(notice, acceptedRevision as number) : []);

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
		<div class="privacy-dialog" role="dialog" aria-modal="true" aria-labelledby="privacy-title" bind:this={dialog}>
			<div class="header">
				<h3 id="privacy-title">{isChanges ? 'Data & privacy changes' : 'Data & privacy'}</h3>
				{#if notice.draft}<span class="draft">Draft</span>{/if}
				<span class="rev">Rev {notice.revision} · {notice.date}</span>
			</div>

			<div class="body">
				{#if isChanges}
					{#each changes as change (change.revision)}
						<section class="change" data-revision={change.revision}>
							<h4>Rev {change.revision} · {change.date}</h4>
							<ul>
								{#each change.items as item}<li>{item}</li>{/each}
							</ul>
						</section>
					{/each}
					<button type="button" class="expand" aria-expanded={fullOpen} onclick={() => (fullOpen = !fullOpen)}>
						Full notice
					</button>
				{/if}
				{#if !isChanges || fullOpen}
					<div class="full-notice">
						{#each notice.sections as section (section.id)}
							<section data-section={section.id}>
								<h4>{section.heading}</h4>
								{#each section.paragraphs as p}<p>{p}</p>{/each}
								{#if section.items.length > 0}
									<ul>{#each section.items as item}<li>{item}</li>{/each}</ul>
								{/if}
							</section>
						{/each}
					</div>
				{/if}
			</div>

			<div class="actions">
				<button type="button" class="btn quit" onclick={quit}>Quit</button>
				<button type="button" class="btn continue" disabled={busy} onclick={accept}>Continue</button>
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
		width: min(640px, calc(100vw - 32px));
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

	.draft {
		font-size: 0.7rem;
		padding: 0.1rem 0.4rem;
		border: 1px solid var(--warning);
		color: var(--warning);
		border-radius: 4px;
	}

	.rev {
		margin-left: auto;
		font-size: 0.75rem;
		color: var(--text-muted);
	}

	.body {
		padding: 0.75rem 1rem;
		overflow-y: auto;
		font-size: 0.85rem;
	}

	h4 {
		margin: 0.75rem 0 0.25rem;
		font-size: 0.85rem;
	}

	p, ul {
		margin: 0.25rem 0;
		color: var(--text-muted);
	}

	.expand {
		margin-top: 0.75rem;
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
