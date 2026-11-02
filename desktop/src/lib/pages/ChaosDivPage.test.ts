import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import ChaosDivPage from './ChaosDivPage.svelte';

vi.mock('$lib/stores/divine-rate.svelte', () => ({ currentDivineRate: () => null }));

describe('Companion totals', () => {
	it('shows a copyable item amount beside both currency totals with a cold rate', () => {
		const html = render(ChaosDivPage).body.replace(/<!--[\s\S]*?-->/g, '');
		const totals = html.match(/<section[^>]*aria-label="Totals"[^>]*>([\s\S]*?)<\/section>/)?.[1] ?? '';
		expect(totals.match(/class="total-card\b/g)).toHaveLength(3);
		expect(totals).toContain('Item amount');
		expect(totals).toMatch(/<button[^>]*title="Copy item count"[^>]*>\s*Copy\s*<\/button>/);
		expect(totals).toMatch(/<button[^>]*disabled[^>]*title="Copy divine total"/);
	});
	it('provides locale-independent decimal entry for every numeric field', () => {
		const html = render(ChaosDivPage).body;
		for (const id of ['chaos-div-amount', 'chaos-div-ratio']) {
			expect(html.match(new RegExp(`<input[^>]*id="${id}"[^>]*>`))?.[0]).toMatch(/type="text" inputmode="decimal"/);
		}
	});
});
