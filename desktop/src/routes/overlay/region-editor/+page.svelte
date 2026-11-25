<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { currentMonitor } from '@tauri-apps/api/window';
	import type { SsotSnapshot, OcrRegionsSnapshot } from '$lib/stores/ssot.svelte';
	import type { GameMonitorInfo } from '$lib/overlay/monitor-choice';
	import { edgeAt, type ResizeEdge } from '$lib/overlay/widgets/widget-geometry';
	import {
		createRegionEditorDraft, reduceRegionEditor, editorView, savePayload, autoRegion,
		REGION_NAMES, EDITABLE_REGIONS, RECT_FIELDS, shapeLocked, stripSlots, templeRemaining,
		editorPanelPosition, type RegionEditorDraft, type RegionEditorAction,
		type RegionRect, type RectField, type EditableRegion
	} from '$lib/overlay/ocr-region-editor';

	let draft = $state<RegionEditorDraft | null>(null);
	let error = $state('');
	let monitorKnown = $state(false);
	let saving = $state(false);
	let hover = $state<EditableRegion | null>(null);
	let typedValues = $state<Partial<Record<RectField, string>>>({});
	let panel: HTMLElement;
	let panelWidth = $state(470);
	let panelHeight = $state(360);
	let gesture: { id: number; from: RegionRect; edge: ResizeEdge | null; x: number; y: number } | null = null;
	let alive = false;
	const view = $derived(draft ? editorView(draft) : null);
	const selected = $derived(view?.selected ?? null);
	const pickerRect = $derived(draft && view?.view === 'picker' && (hover || selected) ? draft.rects[(hover ?? selected)!] : null);
	const position = $derived(draft ? editorPanelPosition(draft, { width: panelWidth, height: panelHeight }) : { x: 16, y: 16 });
	const handles: { edge: ResizeEdge; x: number; y: number }[] = [
		{ edge: 'NorthWest', x: 0, y: 0 }, { edge: 'NorthEast', x: 100, y: 0 },
		{ edge: 'SouthWest', x: 0, y: 100 }, { edge: 'SouthEast', x: 100, y: 100 },
		{ edge: 'North', x: 50, y: 0 }, { edge: 'South', x: 50, y: 100 },
		{ edge: 'West', x: 0, y: 50 }, { edge: 'East', x: 100, y: 50 }
	];

	function report(message: string) {
		console.error(message);
		void invoke('app_log_from_frontend', { msg: `[region-editor] ${message}` }).catch(console.error);
	}

	$effect(() => {
		alive = true;
		void (async () => {
			try {
				const win = getCurrentWebviewWindow();
				const [snapshot, game, monitor, sf] = await Promise.all([
					invoke<SsotSnapshot>('get_ssot'), invoke<GameMonitorInfo | null>('get_game_monitor'),
					currentMonitor(), win.scaleFactor()
				]);
				if (!alive) return;
				if (!monitor || !(sf > 0)) throw new Error('Monitor geometry unknown');
				if (!snapshot.ocrRegions) throw new Error('OCR regions unavailable');
				monitorKnown = !!game && game.x === monitor.position.x && game.y === monitor.position.y;
				const requested = new URLSearchParams(window.location.search).get('region');
				draft = createRegionEditorDraft({
					snapshot: snapshot.ocrRegions, uiScale: snapshot.screen?.uiScale ?? null,
					monitor: { x: monitor.position.x, y: monitor.position.y, width: monitor.size.width,
						height: monitor.size.height, scaleFactor: sf },
					select: EDITABLE_REGIONS.includes(requested as EditableRegion) ? requested as EditableRegion : null
				});
			} catch (reason) {
				if (alive) error = String(reason);
				report(`Editor unavailable: ${reason}`);
			}
		})();
		return () => { alive = false; gesture = null; };
	});

	$effect(() => {
		if (!panel || typeof ResizeObserver === 'undefined') return;
		const observer = new ResizeObserver(entries => {
			const rect = entries[0]?.target.getBoundingClientRect();
			if (rect?.width && rect.height) { panelWidth = rect.width; panelHeight = rect.height; }
		});
		observer.observe(panel);
		return () => observer.disconnect();
	});

	$effect(() => {
		if (view?.view !== 'picker' || !selected) return;
		panel?.querySelector<HTMLButtonElement>(`[data-picker="${selected}"]`)?.focus();
	});

	function dispatch(action: RegionEditorAction) {
		if (draft && !saving) draft = reduceRegionEditor(draft, action);
	}

	function style(rect: RegionRect): string {
		const sf = draft!.monitor.scaleFactor;
		return `left:${rect[0] / sf}px;top:${rect[1] / sf}px;width:${rect[2] / sf}px;height:${rect[3] / sf}px`;
	}
	const numbers = (rect: RegionRect | null) => rect ? `[${rect.join(', ')}]` : 'not set';

	function begin(event: PointerEvent, edge?: ResizeEdge) {
		if (!draft || !view?.frame || !view.frameCss || saving || view.leavePrompt || event.button !== 0) return;
		event.preventDefault();
		event.stopPropagation();
		const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
		const picked = edge ?? edgeAt(view.frameCss, event.clientX - box.left, event.clientY - box.top);
		if (shapeLocked(selected) && picked && !['NorthWest', 'NorthEast', 'SouthWest', 'SouthEast'].includes(picked)) return;
		gesture = { id: event.pointerId, from: [...view.frame], edge: picked, x: event.clientX, y: event.clientY };
		(event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId);
	}

	function move(event: PointerEvent) {
		if (!gesture || gesture.id !== event.pointerId) return;
		const delta = { from: gesture.from, dx: event.clientX - gesture.x, dy: event.clientY - gesture.y };
		dispatch(gesture.edge ? { type: 'resize', edge: gesture.edge, ...delta } : { type: 'drag', ...delta });
	}

	function typeField(field: RectField, value: string) {
		typedValues[field] = value;
		dispatch({ type: 'typeField', field, value });
	}

	async function save() {
		if (!draft || !view?.saveEnabled || saving) return;
		const candidate = draft;
		const payload = savePayload(candidate);
		saving = true;
		try {
			if (!monitorKnown) throw new Error('game monitor unknown — focus the game once');
			if (candidate.snapshot.mode !== 'manual') throw new Error('OCR regions are Auto');
			await invoke('set_manual_ocr_regions', { regions: payload });
			if (!alive) return;
			const snapshot: OcrRegionsSnapshot = { ...candidate.snapshot, manual: {
				setOn: payload.set_on, edited: true, labGem: payload.lab_gem, labFont: payload.lab_font,
				labGemSlots: payload.lab_gem_slots, templePanel: payload.temple_panel, mercPanel: payload.merc_panel
			} };
			draft = reduceRegionEditor(candidate, { type: 'saved', snapshot });
		} catch (reason) {
			if (alive) draft = reduceRegionEditor(candidate, { type: 'saveFailed', error: String(reason).replace(/^Error: /, '') });
			report(`Save failed: ${reason}`);
		} finally { saving = false; }
	}

	async function close() {
		try { await getCurrentWebviewWindow().close(); }
		catch (reason) { error = `Close failed: ${reason}`; report(error); }
	}

	function keydown(event: KeyboardEvent) {
		if (saving) { event.preventDefault(); return; }
		if (event.ctrlKey && event.key.toLowerCase() === 's') { event.preventDefault(); void save(); return; }
		if ((event.target as HTMLElement)?.tagName === 'INPUT') {
			if (event.key === 'Enter') { event.preventDefault(); (event.target as HTMLInputElement).blur(); }
			if (event.key !== 'Escape' && event.key !== 'Tab') return;
		}
		if (event.key === 'Escape' && view?.view === 'picker') { event.preventDefault(); void close(); return; }
		if (!['Escape', 'Tab', 'Enter', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(event.key)) return;
		if (event.key === 'Enter' && (event.target as HTMLElement)?.tagName === 'BUTTON') return;
		event.preventDefault();
		dispatch({ type: 'key', key: event.key, shift: event.shiftKey, alt: event.altKey });
	}
</script>

<svelte:window onkeydown={keydown} onpointermove={move} onpointerup={() => { gesture = null; }} onpointercancel={() => { gesture = null; }} />

<main class="editor" aria-label="OCR region editor">
	{#if draft && view}
		{#if view.view === 'picker' && pickerRect}
			<div class="region hover" style={style(pickerRect)}><span class="rname">{REGION_NAMES[hover ?? selected ?? 'labGem']}</span></div>
		{/if}
		{#if view.view === 'edit' && view.frame && selected}
			{#if view.autoGhost}
				<div class="region ghost" data-region={selected} style={style(view.autoGhost)}><span class="rname">{REGION_NAMES[selected]}, Auto {numbers(view.autoGhost)}</span></div>
			{/if}
			<div class="region selected" data-region={selected} role="button" tabindex="0" aria-label={`${REGION_NAMES[selected]} frame`} style={style(view.frame)} onpointerdown={event => begin(event)}>
				<span class="rname">{REGION_NAMES[selected]}</span>
				{#each handles.filter((_, i) => !shapeLocked(selected) || i < 4) as handle}
					<button class="handle" aria-label={`Resize ${handle.edge}`} style={`left:${handle.x}%;top:${handle.y}%`} onpointerdown={event => begin(event, handle.edge)} disabled={saving || view.leavePrompt}></button>
				{/each}
				<div class="readout">{numbers(view.frame)}</div>
			</div>
			{#if view.carried}
				<div class="region carried" style={style(view.carried)}><span class="rname">Gem choice slots</span></div>
			{/if}
			{#if view.carried || selected === 'labGemSlots'}
				{#each stripSlots(view.carried ?? view.frame) as slot, i}
					<div class="slot" style={style(slot)}><span>{i}</span></div>
				{/each}
			{/if}
			{#if selected === 'templePanel'}
				<div class="region derived" style={style(templeRemaining(view.frame))}><span class="rname">Temple remaining</span></div>
			{/if}
		{/if}
		<section class="edit-panel" bind:this={panel} style={`left:${position.x}px;top:${position.y}px`} aria-label={view.view === 'picker' ? 'Region picker' : 'Edit region'}>
			<div class="ep-title">{view.view === 'picker' ? 'Edit OCR regions' : REGION_NAMES[selected!]}
				{#if view.view === 'edit' && shapeLocked(selected)}<span class="tag">shape locked</span>{/if}
				{#if view.tag}<span class="changed">{view.tag}</span>{/if}
			</div>
			<div class="ep-sub" class:err={!monitorKnown}>{monitorKnown ? `${draft.monitor.width}×${draft.monitor.height} @ (${draft.monitor.x}, ${draft.monitor.y})` : 'Game monitor unknown — focus the game once'}</div>
			{#if view.view === 'picker'}
				<ul class="ep-list">
					{#each EDITABLE_REGIONS as region}
						{@const rect = draft.rects[region]}
						{@const auto = autoRegion(draft, region)}
						<li class:child={region === 'labGemSlots'}>
							<button data-picker={region} class:focused={selected === region} onmouseenter={() => { hover = region; }} onmouseleave={() => { hover = null; }} onfocus={() => dispatch({ type: 'focus', region })} onclick={() => dispatch({ type: 'pick', region })}>
								<span class="dot"></span>{REGION_NAMES[region]}
								{#if rect && auto && rect.some((value, i) => value !== auto[i])}<span class="changed">{rect[2] !== auto[2] || rect[3] !== auto[3] ? 'resized' : 'moved'}</span>{/if}
								<span class="r" class:unset={!rect}>{numbers(rect)}</span>
							</button>
						</li>
						{#if region === 'templePanel'}<li class="derived child">Temple remaining <span class="tag">follows Temple panel</span></li>{/if}
					{/each}
				</ul>
			{:else if view.frame}
				<div class="ep-fields">
					{#each RECT_FIELDS as field, i}
						<label>{field}<input aria-label={field} aria-invalid={!!draft.invalid[field]} class:bad={!!draft.invalid[field]} value={draft.invalid[field] ? typedValues[field] : view.frame[i]} inputmode="numeric" disabled={saving || view.leavePrompt} oninput={event => typeField(field, event.currentTarget.value)} /></label>
					{/each}
				</div>
				{#if view.carried}<div class="ep-carried">Gem choice slots <span class="r">{numbers(view.carried)}</span></div>{/if}
				{#if selected === 'labGemSlots'}<div class="ep-extra">3 × {stripSlots(view.frame)[0][2]} px · pitch {stripSlots(view.frame)[1][0] - view.frame[0]}</div>{/if}
				{#if selected === 'templePanel'}<div class="ep-extra">Temple remaining {numbers(templeRemaining(view.frame))}</div>{/if}
			{/if}
			<div class="config-hint" role="status" class:failed={!!view.invalidField || !!draft.error} class:unsaved={view.hint.startsWith('Unsaved:')} class:saved={view.hint.startsWith('Saved:')}>{error || view.hint}</div>
			<div class="ep-actions" class:ep-prompt={view.leavePrompt}>
				{#if view.view === 'picker'}
					<span class="spacer"></span><button class="config-btn" onclick={close}>Close</button>
				{:else}
					{#if view.leavePrompt}
						<button class="config-btn" disabled={saving} onclick={() => dispatch({ type: 'leaveChoice', choice: 'keep' })}>Keep editing</button>
						<span class="spacer"></span><button class="config-btn danger" disabled={saving} onclick={() => dispatch({ type: 'leaveChoice', choice: 'discard' })}>Discard</button>
					{:else}
						<button class="config-btn reset" disabled={!view.resetEnabled || saving} title={view.resetTitle} onclick={() => dispatch({ type: 'resetToAuto' })}>Reset to Auto</button>
						<span class="spacer"></span><button class="config-btn" disabled={saving} onclick={() => dispatch({ type: 'back' })}>Back</button>
					{/if}
					<button class="config-btn save" disabled={!view.saveEnabled || saving} title={!view.saveEnabled ? view.invalidField ? view.hint : 'No changes' : 'Save'} onclick={save}>{saving ? 'Saving…' : 'Save'}</button>
				{/if}
			</div>
		</section>
	{:else}
		<section class="edit-panel unavailable"><div class="ep-title">Edit OCR regions</div><div class="config-hint failed" role="status">{error || 'Loading…'}</div><button class="config-btn" onclick={close}>Close</button></section>
	{/if}
</main>

<style>
	:global(html), :global(body) { margin: 0; padding: 0; background: transparent !important; overflow: hidden; }
	.editor { position: fixed; inset: 0; background: rgb(6 8 12 / 32%); user-select: none; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; }
	.region, .slot { position: absolute; box-sizing: border-box; pointer-events: none; }
	.region { border: 1px dashed var(--color-lab-text-secondary); }
	.rname { position: absolute; top: 0; left: 0; padding: 1px 6px; font-size: 12px; font-weight: 600; color: var(--color-lab-text); background: rgb(15 17 23 / 85%); white-space: nowrap; }
	.region.hover { border-width: 2px; }
	.region.ghost, .region.derived { border-style: dotted; }
	.ghost .rname, .derived .rname { color: var(--color-lab-text-secondary); font-weight: 500; }
	.region.selected { border: 3px solid var(--color-lab-red); cursor: move; pointer-events: auto; touch-action: none; }
	.selected .rname { color: var(--color-lab-bg); background: var(--color-lab-red); font-weight: 700; }
	.handle { position: absolute; width: 12px; height: 12px; transform: translate(-50%, -50%); padding: 0; background: white; border: 2px solid var(--color-lab-red); pointer-events: auto; touch-action: none; }
	.handle:nth-of-type(1), .handle:nth-of-type(4) { cursor: nwse-resize; }
	.handle:nth-of-type(2), .handle:nth-of-type(3) { cursor: nesw-resize; }
	.handle:nth-of-type(5), .handle:nth-of-type(6) { cursor: ns-resize; }
	.handle:nth-of-type(7), .handle:nth-of-type(8) { cursor: ew-resize; }
	.readout { position: absolute; right: -3px; bottom: -30px; padding: 3px 8px; background: rgb(15 17 23 / 92%); border: 1px solid var(--color-lab-red); border-radius: 3px; color: var(--color-lab-text); font-size: 13px; font-family: 'Consolas', 'Courier New', monospace; white-space: nowrap; }
	.region.carried { border-color: #53d3ff; }
	.carried .rname { color: #53d3ff; top: auto; bottom: -21px; }
	.slot { border: 1px solid #53d3ff; }
	.slot span { position: absolute; top: 0; left: 0; padding: 1px 3px; background: rgb(17 17 17 / 82%); color: #53d3ff; font-size: 9px; font-weight: 600; }
	.edit-panel { position: absolute; z-index: 2; width: min(470px, calc(100vw - 32px)); box-sizing: border-box; padding: 14px 16px; background: rgb(15 17 23 / 96%); border: 1px solid var(--color-lab-red); border-radius: 8px; box-shadow: 0 6px 24px rgb(0 0 0 / 55%); color: var(--color-lab-text); }
	.unavailable { left: 16px; top: 16px; }
	.ep-title { font-size: 15px; font-weight: 700; color: var(--color-lab-red); }
	.tag { font-size: 11px; font-weight: 500; color: var(--color-lab-text-muted); margin-left: 6px; }
	.changed { color: var(--color-lab-yellow); font-size: 11px; font-weight: 700; margin-left: 6px; }
	.ep-sub { font-size: 13px; color: var(--color-lab-text-secondary); margin-bottom: 10px; font-family: 'Consolas', 'Courier New', monospace; }
	.ep-sub.err { color: var(--color-lab-red); font-family: inherit; font-weight: 700; }
	.ep-list { list-style: none; padding: 0; border: 1px solid var(--color-lab-border); border-radius: 4px; margin: 0 0 10px; }
	.ep-list li + li { border-top: 1px solid var(--color-lab-border); }
	.ep-list button { width: 100%; display: flex; align-items: center; gap: 8px; padding: 6px 10px; font: inherit; font-size: 14px; text-align: left; border: none; color: inherit; background: transparent; cursor: pointer; }
	.ep-list .child button, .ep-list .derived { padding-left: 28px; }
	.ep-list .derived { padding-top: 6px; padding-bottom: 6px; color: var(--color-lab-text-muted); font-size: 13px; }
	.ep-list button:hover, .ep-list button.focused { background: rgb(228 228 231 / 8%); outline: 1px dashed var(--color-lab-text-secondary); outline-offset: -1px; }
	.dot { width: 10px; height: 10px; border-radius: 50%; border: 2px solid var(--color-lab-text-secondary); box-sizing: border-box; flex-shrink: 0; }
	.r { margin-left: auto; font-family: 'Consolas', 'Courier New', monospace; font-size: 12px; color: var(--color-lab-text-secondary); white-space: nowrap; }
	.r.unset { font-family: inherit; font-style: italic; }
	.ep-fields { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; margin-bottom: 8px; }
	.ep-fields label { font-size: 11px; color: var(--color-lab-text-secondary); display: flex; flex-direction: column; gap: 2px; }
	.ep-fields input { box-sizing: border-box; background: var(--color-lab-bg); border: 1px solid var(--color-lab-border); color: var(--color-lab-text); border-radius: 4px; padding: 4px 6px; font-size: 14px; font-family: 'Consolas', 'Courier New', monospace; width: 100%; }
	.ep-fields input.bad { border-color: var(--color-lab-red); color: #fca5a5; background: rgb(239 68 68 / 12%); }
	.ep-carried { display: flex; gap: 8px; align-items: center; color: #53d3ff; font-size: 13px; margin: -2px 0 8px; }
	.ep-extra { color: var(--color-lab-text-secondary); font: 12px 'Consolas', 'Courier New', monospace; margin: -2px 0 8px; }
	.config-hint { font-size: 14px; color: var(--color-lab-text-secondary); margin-bottom: 12px; line-height: 1.4; min-height: 20px; overflow-wrap: anywhere; }
	.config-hint.failed { font-weight: 700; color: var(--color-lab-red); }
	.config-hint.unsaved { font-weight: 700; color: var(--color-lab-yellow); }
	.config-hint.saved { color: var(--color-lab-green); }
	.ep-actions { display: flex; gap: 8px; }
	.ep-prompt { border-top: 1px solid var(--color-lab-border); margin-top: 10px; padding-top: 10px; }
	.spacer { flex: 1; }
	.config-btn { min-height: 32px; padding: 6px 18px; font-size: 14px; font-weight: 600; color: var(--color-lab-text); background: var(--color-lab-surface); border: 1px solid var(--color-lab-border); border-radius: 4px; font-family: inherit; white-space: nowrap; }
	.config-btn.reset { padding: 6px 12px; }
	.config-btn.save { color: var(--color-lab-bg); background: var(--color-lab-green); border-color: var(--color-lab-green); }
	.config-btn:disabled { color: var(--color-lab-text-muted); background: var(--color-lab-surface); border-color: var(--color-lab-border); }
	.config-btn.danger { color: #fca5a5; border-color: rgb(239 68 68 / 60%); }
</style>
