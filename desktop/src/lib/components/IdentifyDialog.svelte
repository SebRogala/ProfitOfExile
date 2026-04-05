<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { store } from '$lib/stores/status.svelte';
	import { getVersion } from '@tauri-apps/api/app';

	let { open = $bindable(false) } = $props();

	let alias = $state('');
	let shortId = $state('');
	let submitting = $state(false);
	let error = $state('');
	let success = $state('');
	let appVersion = $state('');

	$effect(() => {
		if (open) {
			alias = '';
			error = '';
			success = '';
			submitting = false;
			invoke<string>('get_device_id')
				.then(id => { shortId = id; })
				.catch(() => { shortId = '??'; });
			getVersion()
				.then(v => { appVersion = v; })
				.catch(() => {});
		}
	});

	async function identify() {
		const trimmed = alias.trim();
		if (!trimmed) {
			error = 'Alias cannot be empty';
			return;
		}
		if (trimmed.length > 64) {
			error = 'Alias too long (max 64 characters)';
			return;
		}

		submitting = true;
		error = '';
		success = '';

		try {
			const serverUrl = store.status?.server_url || '';
			if (!serverUrl) {
				error = 'No server URL configured';
				return;
			}

			const deviceId = store.status?.device_id;
			const headers: Record<string, string> = { 'Content-Type': 'application/json' };
			if (deviceId && typeof deviceId === 'string') {
				headers['X-Device-ID'] = deviceId;
			}
			if (appVersion) {
				headers['X-App-Version'] = appVersion;
			}

			const resp = await fetch(`${serverUrl}/api/device/identify`, {
				method: 'POST',
				headers,
				body: JSON.stringify({ alias: trimmed }),
			});

			if (!resp.ok) {
				const body = await resp.json().catch(() => ({ error: resp.statusText }));
				error = body.error || `Server returned ${resp.status}`;
				return;
			}

			const body = await resp.json();
			success = `Identified as "${body.alias}"`;

			// Auto-close after a short delay
			setTimeout(() => { open = false; }, 1500);
		} catch (e: any) {
			error = e.message || 'Failed to identify';
		} finally {
			submitting = false;
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			open = false;
		} else if (e.key === 'Enter' && !submitting) {
			identify();
		}
	}
</script>

{#if open}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="backdrop" onkeydown={handleKeydown} onclick={() => open = false}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="dialog" onclick={(e) => e.stopPropagation()}>
			<div class="header">
				<h3>Identify Device</h3>
				<button class="close-btn" onclick={() => open = false}>&times;</button>
			</div>

			<div class="body">
				<div class="device-info">
					<span class="label">Device</span>
					<code class="device-code">{shortId || '...'}</code>
				</div>

				<div class="field">
					<label for="identify-alias">Alias</label>
					<!-- svelte-ignore a11y_autofocus -->
					<input
						id="identify-alias"
						type="text"
						placeholder="e.g. Gaming PC, Laptop"
						bind:value={alias}
						maxlength="64"
						autofocus
						disabled={submitting}
					/>
					<span class="hint">A name for this device so the server can identify it.</span>
				</div>

				{#if error}
					<div class="message error">{error}</div>
				{/if}
				{#if success}
					<div class="message success">{success}</div>
				{/if}
			</div>

			<div class="footer">
				<button class="btn cancel" onclick={() => open = false} disabled={submitting}>Cancel</button>
				<button class="btn identify" onclick={identify} disabled={submitting || !alias.trim()}>
					{submitting ? 'Identifying...' : 'Identify'}
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 9999;
	}

	.dialog {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		width: 380px;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
	}

	.header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 16px;
		border-bottom: 1px solid var(--border);
	}

	.header h3 {
		font-size: 14px;
		font-weight: 600;
		color: var(--text);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		font-size: 18px;
		cursor: pointer;
		padding: 0 4px;
		line-height: 1;
	}
	.close-btn:hover { color: var(--text); }

	.body {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.device-info {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.label {
		color: var(--text-muted);
		font-size: 12px;
	}

	.device-code {
		background: var(--bg);
		padding: 2px 8px;
		border-radius: 4px;
		font-family: monospace;
		font-size: 13px;
		color: var(--accent);
		letter-spacing: 0.5px;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.field label {
		font-size: 12px;
		color: var(--text-muted);
	}

	.field input {
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 8px 10px;
		color: var(--text);
		font-size: 13px;
		outline: none;
	}
	.field input:focus {
		border-color: var(--accent);
	}
	.field input:disabled {
		opacity: 0.5;
	}

	.hint {
		font-size: 11px;
		color: var(--text-muted);
	}

	.message {
		font-size: 12px;
		padding: 6px 10px;
		border-radius: 4px;
	}

	.message.error {
		background: rgba(239, 68, 68, 0.15);
		color: #ef4444;
	}

	.message.success {
		background: rgba(74, 222, 128, 0.15);
		color: var(--success);
	}

	.footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding: 12px 16px;
		border-top: 1px solid var(--border);
	}

	.btn {
		padding: 6px 16px;
		border-radius: 4px;
		font-size: 13px;
		cursor: pointer;
		border: none;
	}
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn.cancel {
		background: transparent;
		color: var(--text-muted);
		border: 1px solid var(--border);
	}
	.btn.cancel:hover:not(:disabled) { color: var(--text); }

	.btn.identify {
		background: var(--accent);
		color: white;
	}
	.btn.identify:hover:not(:disabled) {
		filter: brightness(1.1);
	}
</style>
