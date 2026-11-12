import { afterEach, describe, expect, it, vi } from 'vitest';

const h = vi.hoisted(() => {
	interface FakeWindow {
		label: string;
		moduleId: string;
		wanted: () => boolean;
		configLive: () => boolean;
		desired: boolean[];
		isBuilt: boolean;
		driver: { setDesired(d: boolean): void; built(): boolean };
		configTarget: { label: string; built: () => boolean };
	}
	const created: FakeWindow[] = [];
	return { created };
});

vi.mock('./widget-window', () => ({
	createWidgetWindow: (o: {
		label: string;
		moduleId: string;
		wanted: () => boolean;
		configLive: () => boolean;
	}) => {
		const w = {
			label: o.label,
			moduleId: o.moduleId,
			wanted: o.wanted,
			configLive: o.configLive,
			desired: [] as boolean[],
			isBuilt: false,
			driver: {
				setDesired: (d: boolean) => w.desired.push(d),
				built: () => w.isBuilt
			},
			configTarget: { label: o.label, built: () => w.isBuilt }
		};
		h.created.push(w);
		return w;
	}
}));

import { createModuleWindows, type ModuleWindowRow } from './module-windows.svelte';

function row(label: string, moduleId: string, enabled: boolean | undefined, granted: boolean): ModuleWindowRow {
	return { label, moduleId, granted: () => granted, enabled: () => enabled, debug: () => false };
}

function mutableRow(
	label: string,
	moduleId: string,
	inputs: { enabled: boolean | undefined; granted: boolean }
): ModuleWindowRow {
	return { label, moduleId, granted: () => inputs.granted, enabled: () => inputs.enabled, debug: () => false };
}

afterEach(() => {
	h.created.length = 0;
});

const fakeOf = (label: string) => h.created.find((w) => w.label === label)!;

describe('createModuleWindows', () => {
	it('lists windows in row order', () => {
		const mw = createModuleWindows([row('win-b', 'mod-b', false, true), row('win-a', 'mod-a', false, true)]);

		expect(mw.windows.map((w) => w.label)).toEqual(['win-b', 'win-a']);
	});

	it('keys configTargets by window label, not moduleId', () => {
		const mw = createModuleWindows([row('win-a', 'mod-a', false, true), row('win-b', 'mod-b', false, true)]);

		expect(Object.keys(mw.configTargets).sort()).toEqual(['win-a', 'win-b']);
		expect(mw.configTargets['win-b']).toBe(fakeOf('win-b').configTarget);
	});

	it('scopes a config session to its own row', () => {
		const mw = createModuleWindows([row('win-a', 'mod-a', false, true), row('win-b', 'mod-b', false, true)]);

		mw.setSessions({ 'win-a': { shown: true } });

		expect(fakeOf('win-a').configLive()).toBe(true);
		expect(fakeOf('win-b').configLive()).toBe(false);
	});

	it('does not treat a session keyed by moduleId as live', () => {
		const mw = createModuleWindows([row('win-a', 'mod-a', false, true)]);

		mw.setSessions({ 'mod-a': { shown: true } });

		expect(fakeOf('win-a').configLive()).toBe(false);
	});

	it.each([
		{ enabled: undefined, session: false, granted: true, wanted: false },
		{ enabled: false, session: false, granted: true, wanted: false },
		{ enabled: true, session: false, granted: true, wanted: true },
		{ enabled: true, session: false, granted: false, wanted: false },
		{ enabled: false, session: true, granted: true, wanted: true },
		{ enabled: undefined, session: true, granted: true, wanted: true },
		{ enabled: false, session: true, granted: false, wanted: false }
	])('wanted() is $wanted for enabled=$enabled session=$session granted=$granted', (c) => {
		const mw = createModuleWindows([
			row('win-a', 'mod-a', c.enabled, c.granted),
			row('win-b', 'mod-b', true, true)
		]);
		if (c.session) mw.setSessions({ 'win-a': { shown: true } });

		expect(fakeOf('win-a').wanted()).toBe(c.wanted);
	});

	it('a session on row A leaves row B unwanted', () => {
		const mw = createModuleWindows([row('win-a', 'mod-a', false, true), row('win-b', 'mod-b', false, true)]);

		mw.setSessions({ 'win-a': { shown: true } });

		expect(fakeOf('win-b').wanted()).toBe(false);
	});

	it('getSessions returns the map setSessions stored', () => {
		const mw = createModuleWindows([row('win-a', 'mod-a', false, true)]);

		mw.setSessions({ 'win-a': { shown: true } });

		expect(mw.getSessions()).toEqual({ 'win-a': { shown: true } });
	});

	it('wanted() drops when the module flag turns off on the same table', () => {
		const inputs = { enabled: true as boolean | undefined, granted: true };
		createModuleWindows([mutableRow('win-a', 'mod-a', inputs)]);
		const wanted = fakeOf('win-a').wanted;
		expect(wanted()).toBe(true);

		inputs.enabled = false;

		expect(wanted()).toBe(false);
	});

	it('wanted() drops a session-held window when the grant is withdrawn on the same table', () => {
		const inputs = { enabled: false as boolean | undefined, granted: true };
		const mw = createModuleWindows([mutableRow('win-a', 'mod-a', inputs)]);
		mw.setSessions({ 'win-a': { shown: true } });
		const wanted = fakeOf('win-a').wanted;
		expect(wanted()).toBe(true);

		inputs.granted = false;

		expect(wanted()).toBe(false);
	});
});
