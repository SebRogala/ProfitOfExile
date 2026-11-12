import { flushSync } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { moduleOverlayWanted, moduleWindowStep } from './module-lifecycle';

const h = vi.hoisted(() => {
	interface FakeWindow {
		label: string;
		desired: boolean[];
		setBuilt(b: boolean): void;
	}
	const created: FakeWindow[] = [];
	return { created };
});

vi.mock('./widget-window', () => ({
	createWidgetWindow: (o: { label: string; moduleId: string }) => {
		let built = $state(false);
		const desired: boolean[] = [];
		const w = {
			label: o.label,
			moduleId: o.moduleId,
			desired,
			setBuilt: (b: boolean) => {
				built = b;
			},
			driver: {
				setDesired: (d: boolean) => desired.push(d),
				built: () => built
			},
			configTarget: { label: o.label, built: () => built }
		};
		h.created.push(w);
		return w;
	}
}));

import { createModuleWindows, type ModuleWindowRow } from './module-windows.svelte';

const cleanups: (() => void)[] = [];

afterEach(() => {
	cleanups.splice(0).forEach((c) => c());
	h.created.length = 0;
});

function reactiveInputs(enabled: boolean | undefined, granted: boolean) {
	const inputs = $state({ enabled, granted });
	return inputs;
}

function mount(inputs: { enabled: boolean | undefined; granted: boolean }) {
	const r: ModuleWindowRow = {
		label: 'win-a',
		moduleId: 'mod-a',
		granted: () => inputs.granted,
		enabled: () => inputs.enabled,
		debug: () => false
	};
	let mw!: ReturnType<typeof createModuleWindows>;
	cleanups.push(
		$effect.root(() => {
			mw = createModuleWindows([r]);
		})
	);
	flushSync();
	return { mw, fake: h.created.find((w) => w.label === 'win-a')! };
}

describe('rune-effect project probe', () => {
	it('runs an $effect and re-runs it after a $state change', () => {
		let source = $state(1);
		const seen: number[] = [];
		cleanups.push(
			$effect.root(() => {
				$effect(() => {
					seen.push(source);
				});
			})
		);
		flushSync();
		source = 2;
		flushSync();

		expect(seen).toEqual([1, 2]);
	});
});

describe('createModuleWindows desired-state effect', () => {
	it('reports nothing while the module flag is not yet known', () => {
		const { fake } = mount(reactiveInputs(undefined, true));

		expect(fake.desired).toEqual([]);
	});

	it('raises the window when the module flag is on', () => {
		const { fake } = mount(reactiveInputs(true, true));

		expect(fake.desired).toEqual([true]);
	});

	it('lowers the window when the module flag turns off', () => {
		const inputs = reactiveInputs(true, true);
		const { fake } = mount(inputs);

		inputs.enabled = false;
		flushSync();

		expect(fake.desired).toEqual([true, false]);
	});

	it('keeps the window down when the feature is not granted', () => {
		const { fake } = mount(reactiveInputs(true, false));

		expect(fake.desired).not.toContain(true);
	});

	it('tears down a pre-poll session window when the session ends', () => {
		const { mw, fake } = mount(reactiveInputs(undefined, true));
		mw.setSessions({ 'win-a': { shown: true } });
		flushSync();
		fake.setBuilt(true);
		flushSync();

		mw.setSessions({});
		flushSync();

		expect(fake.desired.at(-1)).toBe(false);
	});

	it('takes down a session-held window when the grant is withdrawn', () => {
		const inputs = reactiveInputs(false, true);
		const { mw, fake } = mount(inputs);
		mw.setSessions({ 'win-a': { shown: true } });
		flushSync();
		fake.setBuilt(true);
		flushSync();

		inputs.granted = false;
		flushSync();

		const step = moduleWindowStep({ enabled: false, configuring: true, built: true, granted: false });
		expect(step).toEqual({ skip: false, desired: moduleOverlayWanted(false, true, false) });
		expect(fake.desired.at(-1)).toBe(false);
	});
});
