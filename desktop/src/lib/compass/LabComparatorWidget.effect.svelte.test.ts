// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import LabComparatorWidget from './LabComparatorWidget.svelte';
import widgetSource from './LabComparatorWidget.svelte?raw';
import { createWidgetActions, WIDGET_ACTIONS } from '$lib/overlay/widgets/widget-actions';
import { setDivineRate } from '$lib/price.svelte';

const native = vi.hoisted(() => ({ invoke: vi.fn(), stop: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => native.stop) }));
vi.mock('@tauri-apps/api/webviewWindow', () => ({ getCurrentWebviewWindow: () => ({ emit: vi.fn(async () => {}) }) }));

let component: ReturnType<typeof mount> | undefined;
let style: HTMLStyleElement | undefined;
afterEach(async () => {
	if (component) await unmount(component);
	component = undefined;
	style?.remove();
	document.body.replaceChildren();
	vi.useRealTimers();
	vi.resetAllMocks();
	setDivineRate(0);
});

it('keep:critical WI-9 widget: polled UNRATED reads yellow at 11px so an unrated gem cannot silently look MID or TOP', async () => {
	vi.useFakeTimers();
	const results = ['UNRATED', 'MID', 'TOP'].map((priceTier, index) => ({
		name: ['Ice Nova of Frostbolts', 'Lacerate of Butchering', 'Smite of Divine Judgement'][index],
		variant: '20/20', recommendation: 'OK', priceTier, confidence: 'OK',
		transPrice: 42, roi: 30, signal: 'STABLE', low7d: 38, high7d: 45
	}));
	native.invoke.mockImplementation(async (command: string) => {
		if (command === 'get_comparator_data') return { results, tradeData: {}, divineRate: 250 };
		if (command === 'get_status') return {};
		throw new Error('Unexpected command ' + command);
	});
	const target = document.body.appendChild(document.createElement('div'));
	flushSync(() => {
		component = mount(LabComparatorWidget, {
			target, props: { configMode: false, label: 'Comparator' },
			context: new Map([[WIDGET_ACTIONS, createWidgetActions(() => false)]])
		});
	});
	await vi.advanceTimersByTimeAsync(500);
	flushSync();
	style = document.createElement('style');
	style.textContent = widgetSource.match(/<style[^>]*>([\s\S]*?)<\/style>/)?.[1] ?? '';
	document.head.append(style);
	const tiers = [...target.querySelectorAll<HTMLElement>('.gem-row .tier')];
	expect(tiers.map((node) => node.textContent)).toEqual(['UNRATED', 'MID', 'TOP']);
	expect(tiers.map((node) => getComputedStyle(node).color)).toEqual(['#eab308', '#94a3b8', '#fbbf24']);
	expect(getComputedStyle(tiers[0]).fontSize).toBe('11px');
});
