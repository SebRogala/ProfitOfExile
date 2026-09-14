/**
 * BetaGate, rendered: whether its children are drawn follows the device's
 * `beta` feature. `svelte/server`'s `render()` in vitest's node environment, so
 * what is asserted is the markup.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { createRawSnippet } from 'svelte';
import { render } from 'svelte/server';
import BetaGate from './BetaGate.svelte';
import { entitlements } from '$lib/stores/entitlements.svelte';

const children = createRawSnippet(() => ({ render: () => '<span>beta setting</span>' }));

function body(): string {
	return render(BetaGate, { props: { children } }).body;
}

describe('BetaGate', () => {
	afterEach(() => {
		entitlements.features = [];
	});

	it('draws its children on a device granted the beta feature', () => {
		entitlements.features = ['temple', 'beta'];
		expect(body()).toContain('beta setting');
	});

	it('draws nothing on a device with the hidden modules but no beta feature', () => {
		entitlements.features = ['merc', 'exchange', 'temple'];
		expect(body()).not.toContain('beta setting');
	});
});
