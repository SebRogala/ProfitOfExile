<script lang="ts" module>
	import type { Registration } from '$lib/stores/entitlements.svelte';

	export interface AccountView {
		glyph: 'initial' | 'person' | '?' | '…';
		initial: string;
		tooltipHtml: string;
		ariaLabel: string;
		filled: boolean;
		dashed: boolean;
	}

	function escapeHtml(s: string): string {
		return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
	}

	function tip(head: string, detail?: string): string {
		const second = detail ? `<div style="color:var(--color-lab-text-secondary)">${detail}</div>` : '';
		return `<div style="font-weight:700">${head}</div>${second}`;
	}

	/** The account button's look, tooltip and accessible name for one registration. */
	export function accountView(r: Registration): AccountView {
		switch (r.state) {
			case 'registered': {
				const alias = escapeHtml(r.alias);
				const detail = r.role ? `Role ${escapeHtml(r.role)} · click to open device identity` : 'click to open device identity';
				return {
					glyph: 'initial',
					initial: ([...r.alias.trim()][0] ?? '').toUpperCase(),
					tooltipHtml: tip(`Registered as “${alias}”`, detail),
					ariaLabel: `Account: registered as ${r.alias}`,
					filled: true,
					dashed: false
				};
			}
			case 'unregistered':
				return { glyph: 'person', initial: '', tooltipHtml: tip('Not registered', 'This device has no alias on the server yet. Click to set one.'), ariaLabel: 'Account: not registered', filled: false, dashed: false };
			case 'unknown':
				return { glyph: '?', initial: '', tooltipHtml: tip('Registration unknown', 'Server unreachable or outdated. Click to check again.'), ariaLabel: 'Account: registration unknown', filled: false, dashed: true };
			default:
				return { glyph: '…', initial: '', tooltipHtml: tip('Checking registration…'), ariaLabel: 'Account: checking registration', filled: false, dashed: true };
		}
	}
</script>

<script lang="ts">
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

	import { nav } from '$lib/stores/navigation.svelte';
	import { store } from '$lib/stores/status.svelte';
	import { persisted } from '$lib/prefs.svelte';
	import { LOCAL_SERVER_URL, serverToggle } from '$lib/server-toggle';
	import { getApiBase } from '$lib/api';
	import { serverBase } from '$lib/stores/server-base.svelte';
	import { CHAOS_ID, DIVINE_ID, currencyIconPath, iconSrc } from '$lib/exchange/view';
	import { divineRate } from '$lib/stores/divine-rate.svelte';
	import { divineRateView } from '$lib/stores/divine-rate';
	import ItemIcon from '$lib/components/ItemIcon.svelte';
	import Tooltip from '$lib/components/Tooltip.svelte';
	import { registration } from '$lib/stores/entitlements.svelte';

	let { status, onAccount }: {
		status: any;
		onAccount?: () => void;
	} = $props();

	/**
	 * The production target this build was given (Rust `DEFAULT_SERVER_URL`
	 * on the SSOT snapshot), '' until the first snapshot. A build without
	 * POE_SERVER_URL reports the local url here, which `serverToggle` reads as
	 * no target (docs/DEV-SETUP.md, "Build-time variables").
	 */
	const BUILT_PROD_URL: string = $derived(serverBase.defaultUrl);

	/**
	 * The url the app left when it last flipped to DEBUG — the way back for a
	 * build with no production target. Persisted (ADR-013) so a restart on
	 * DEBUG still has it. Only written by the dev-only button below.
	 */
	const returnUrl = persisted('devServerReturnUrl', '');

	const toggle = $derived(serverToggle(status?.server_url ?? '', BUILT_PROD_URL, returnUrl.value));

	const apiBase = $derived(getApiBase());
	const rateView = $derived(divineRateView(divineRate, divineRate.now));
	const account = $derived(accountView(registration));

	async function toggleServer() {
		// Nowhere to go: the tooltip has said why; take the click to where a
		// server url can be typed instead of eating it.
		if (!toggle.target) {
			nav.go('/settings');
			return;
		}
		if (toggle.target === LOCAL_SERVER_URL && status?.server_url) returnUrl.value = status.server_url;
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('set_server_url', { url: toggle.target });
	}

	async function minimizeWindow() {
		await getCurrentWebviewWindow().minimize();
	}

	async function toggleMaximize() {
		const win = getCurrentWebviewWindow();
		if (await win.isMaximized()) {
			await win.unmaximize();
		} else {
			await win.maximize();
		}
	}

	async function closeWindow() {
		await getCurrentWebviewWindow().close();
	}

	function startDrag(e: MouseEvent) {
		// Only drag from the topbar background, not from buttons/links
		if ((e.target as HTMLElement).closest('button, a, .status-dot')) return;
		getCurrentWebviewWindow().startDragging();
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<header class="topbar" onmousedown={startDrag}>
	<div class="left">
		<img src="/icon-32.png" alt="ProfitOfExile" class="logo" />
		<span class="app-name">ProfitOfExile</span>
	</div>
	<div class="center">
		<span class="status-dot" class:connected={store.serverConnected} title={store.serverConnected ? `Server: Connected (${status?.server_url})` : 'Server: Disconnected'}></span>
		<span class="status-dot scanning-dot" class:active={status?.state && status.state !== 'Idle'} title={`OCR Scanner: ${status?.state ?? 'Unknown'} ${status?.state === 'PickingGems' ? '— reading gem names' : status?.state === 'FontReady' ? '— font detected' : ''}`}></span>
		{#if import.meta.env.DEV}
			<button class="btn-debug" class:active={toggle.label === 'DEBUG'} class:no-target={!toggle.target} title={toggle.title} onclick={toggleServer}>
				{toggle.label}
			</button>
		{/if}
		{#if store.updateAvailable}
			<button class="update-badge" title="Update available: v{store.updateVersion}" onclick={() => nav.go('/settings')}>
				v{store.updateVersion}
			</button>
		{/if}
		{#if status?.proxy_error}
			<button class="update-badge proxy-error" title={status.proxy_error} onclick={() => nav.go('/settings')}>Proxy error</button>
		{/if}
		<button class="settings-link" title="Settings" onclick={() => nav.go('/settings')}>&#9881;&#65039;</button>
	</div>
	<div class="right">
		<Tooltip text={rateView.tooltipHtml} position="below">
			<button class="rate-chip" class:cold={rateView.state === 'cold'} class:stale={rateView.state === 'stale'}>
				<ItemIcon src={iconSrc(apiBase, currencyIconPath(DIVINE_ID))} alt="Divine Orb" size={18} />
				<span class="rate-text">
					<span class="rate-n">1</span>
					<span>=</span>
					<span class="rate-n">{rateView.chip}</span>
				</span>
				<ItemIcon src={iconSrc(apiBase, currencyIconPath(CHAOS_ID))} alt="Chaos Orb" size={18} />
				{#if rateView.state === 'stale'}
					<span class="rate-tag">stale</span>
				{/if}
			</button>
		</Tooltip>
		<Tooltip text={account.tooltipHtml} position="below">
			<button class="account" class:registered={account.filled} class:dashed={account.dashed} aria-label={account.ariaLabel} onclick={onAccount}>
				<span class="av" aria-hidden="true">
					{#if account.glyph === 'initial'}
						{account.initial}
					{:else if account.glyph === 'person'}
						<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><circle cx="8" cy="5.5" r="2.8"></circle><path d="M2.8 14c.6-2.7 2.7-4.2 5.2-4.2s4.6 1.5 5.2 4.2"></path></svg>
					{:else}
						{account.glyph}
					{/if}
				</span>
			</button>
		</Tooltip>
		<div class="window-controls">
			<button class="win-btn" onclick={minimizeWindow} title="Minimize">&#x2013;</button>
			<button class="win-btn" onclick={toggleMaximize} title="Maximize">&#9723;</button>
			<button class="win-btn close" onclick={closeWindow} title="Close">&#10005;</button>
		</div>
	</div>
</header>

<style>
	.topbar {
		height: 36px;
		min-height: 36px;
		background: var(--surface);
		border-bottom: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 0 0 0.75rem;
		user-select: none;
		-webkit-app-region: drag;
		overflow: hidden;
	}

	.left, .center {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.logo {
		width: 26px;
		height: 26px;
	}

	.app-name {
		font-size: 12px;
		font-weight: 600;
		color: var(--text);
	}

	.status-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--text-muted);
		flex-shrink: 0;
	}

	.status-dot.connected {
		background: var(--success);
	}

	.scanning-dot {
		background: var(--text-muted);
	}

	.scanning-dot.active {
		background: var(--success);
		animation: pulse 1.5s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50% { opacity: 0.3; }
	}

	.btn-debug {
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 0.15rem 0.5rem;
		border-radius: 4px;
		font-size: 0.65rem;
		font-weight: 700;
		letter-spacing: 0.05em;
		cursor: pointer;
		line-height: 1.2;
		-webkit-app-region: no-drag;
	}

	.btn-debug:hover {
		border-color: var(--text-muted);
	}

	/* Nowhere to switch to: the tooltip says why and the click opens Settings. */
	.btn-debug.no-target {
		border-style: dashed;
	}

	.btn-debug.active {
		border-color: var(--warning);
		color: var(--warning);
	}

	.update-badge {
		all: unset;
		font-size: 0.65rem;
		font-weight: 700;
		color: #22c55e;
		background: rgba(34, 197, 94, 0.12);
		border: 1px solid rgba(34, 197, 94, 0.3);
		padding: 0.1rem 0.5rem;
		border-radius: 4px;
		cursor: pointer;
		letter-spacing: 0.03em;
		-webkit-app-region: no-drag;
		animation: pulse 2s ease-in-out infinite;
	}

	.update-badge:hover {
		background: rgba(34, 197, 94, 0.25);
	}

	.update-badge.proxy-error {
		color: var(--color-lab-red);
		background: rgba(239, 68, 68, 0.15);
		border-color: rgba(239, 68, 68, 0.4);
		animation: none;
	}

	.update-badge.proxy-error:hover {
		background: rgba(239, 68, 68, 0.25);
	}

	.settings-link {
		all: unset;
		font-size: 1rem;
		opacity: 0.6;
		transition: opacity 0.15s;
		line-height: 1;
		cursor: pointer;
		-webkit-app-region: no-drag;
	}

	.settings-link:hover {
		opacity: 1;
	}

	.right {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	/* Tooltip's wrapper is an unstyled span: as a flex item it holds an inline
	   line box, which baseline-aligns the chip instead of centring it on the
	   window buttons. Centre it here; the shared Tooltip stays untouched. */
	.right :global(.tooltip-wrap) {
		display: flex;
		align-items: center;
	}

	.rate-chip {
		all: unset;
		cursor: help;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		color: var(--text);
		padding: 2px 6px;
		border-radius: 4px;
		-webkit-app-region: no-drag;
	}

	.rate-chip:hover {
		background: rgba(255, 255, 255, 0.06);
	}

	.rate-chip:focus-visible {
		outline: 1px solid var(--color-lab-blue);
		outline-offset: -1px;
	}

	/* Align the mixed-font text on one baseline; the chip centres this row between its icons. */
	.rate-text {
		display: inline-flex;
		align-items: baseline;
		gap: 4px;
	}

	.rate-n {
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 600;
	}

	.stale .rate-n,
	.rate-tag {
		color: var(--warning);
	}

	.cold .rate-n {
		color: var(--text-muted);
	}

	.rate-tag {
		font-size: 0.625rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.account {
		all: unset;
		box-sizing: border-box;
		cursor: pointer;
		display: inline-flex;
		align-items: center;
		height: 24px;
		padding: 1px;
		border-radius: 12px;
		-webkit-app-region: no-drag;
	}

	.account:hover {
		background: rgba(255, 255, 255, 0.06);
	}

	.account:focus-visible {
		outline: 1px solid var(--color-lab-blue);
		outline-offset: 1px;
	}

	.av {
		box-sizing: border-box;
		width: 22px;
		height: 22px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 11px;
		font-weight: 700;
		line-height: 1;
		color: var(--text-muted);
		border: 1.5px solid var(--text-muted);
	}

	.av svg {
		width: 13px;
		height: 13px;
	}

	.account:hover .av {
		color: var(--text);
		border-color: var(--text);
	}

	.account.registered .av {
		background: var(--text);
		color: var(--surface);
		border-color: var(--text);
	}

	.account.dashed .av {
		border-style: dashed;
	}

	.window-controls {
		display: flex;
		align-items: stretch;
		height: 36px;
		-webkit-app-region: no-drag;
	}

	.win-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		width: 46px;
		height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: pointer;
		font-size: 14px;
		transition: background 0.1s;
	}

	.win-btn:hover {
		background: rgba(255, 255, 255, 0.1);
		color: var(--text);
	}

	.win-btn.close:hover {
		background: #e81123;
		color: white;
	}
</style>
