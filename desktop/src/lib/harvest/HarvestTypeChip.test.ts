/**
 * The type chip, rendered with `svelte/server` (no DOM): which parts it draws
 * for a given `Chip` prop. WHAT the prop holds is `view.ts`'s decision
 * (`view.test.ts`); the pick click is the controller's (WI-8).
 *
 * Visible text is read from its own element (`texts`), so the icon's `alt`
 * cannot stand in for a dropped label.
 */
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import HarvestTypeChip from './HarvestTypeChip.svelte';
import { PICK_TITLE_KEPT, type Chip } from './view';

const HOLLOW: Chip = {
	itemId: 'Metadata/Items/Currency/CurrencyDelveCraftingSockets',
	name: 'Hollow Fossil',
	shortName: 'Hollow',
	icon: '/icon/currency-exchange/hollow',
	priceText: '402c',
	unpriced: false,
	picked: false,
	pickTitle: null,
	evText: null,
	evTone: null,
	shareText: '0.02%'
};

const LUCENT: Chip = {
	...HOLLOW,
	itemId: 'Metadata/Items/Currency/CurrencyDelveCraftingMana',
	name: 'Lucent Fossil',
	shortName: 'Lucent',
	priceText: '9.1c',
	evText: '+25.7c',
	evTone: 'gain',
	shareText: null
};

const UNPRICED: Chip = { ...HOLLOW, priceText: '—', unpriced: true };
const PICKED: Chip = { ...HOLLOW, picked: true, pickTitle: PICK_TITLE_KEPT };

function html(chip: Chip): string {
	return render(HarvestTypeChip, { props: { chip, apiBase: 'http://api', onpick: () => {} } }).body;
}

/** Inner text of every `<tag>` whose class list holds `cls` (Svelte comments and scope classes tolerated). */
function texts(markup: string, tag: string, cls: string): string[] {
	const re = new RegExp(`<${tag}\\b[^>]*class="[^"]*\\b${cls}\\b[^"]*"[^>]*>([\\s\\S]*?)</${tag}>`, 'g');
	return [...markup.matchAll(re)].map((m) => m[1].replace(/<!--[\s\S]*?-->/g, '').trim());
}

/** The opening tag of the chip button. */
function chipTag(markup: string): string {
	const m = markup.match(/<button\b[^>]*class="[^"]*\bchip\b[^"]*"[^>]*>/);
	return m ? m[0] : '';
}

describe('HarvestTypeChip', () => {
	it('draws the short name in its name element', () => {
		expect(texts(html(HOLLOW), 'span', 'name')).toEqual(['Hollow']);
	});

	it('draws a keeper chip price in its price element', () => {
		expect(texts(html(HOLLOW), 'span', 'price')).toEqual(['402c']);
	});

	it('draws a feeder chip price in its price element', () => {
		expect(texts(html(LUCENT), 'span', 'price')).toEqual(['9.1c']);
	});

	it('draws a keeper chip weight share', () => {
		expect(texts(html(HOLLOW), 'span', 'share')).toEqual(['0.02%']);
	});

	it('draws a feeder chip signed loop EV', () => {
		expect(texts(html(LUCENT), 'span', 'ev')).toEqual(['+25.7c']);
	});

	it('draws no EV on a keeper chip', () => {
		expect(texts(html(HOLLOW), 'span', 'ev')).toEqual([]);
	});

	it('colours a gaining feeder EV as a gain', () => {
		expect(html(LUCENT)).toMatch(/class="[^"]*\bev\b[^"]*\bgain\b/);
	});

	it('colours a losing feeder EV as a loss', () => {
		expect(html({ ...LUCENT, evText: '−0.3c', evTone: 'loss' })).toMatch(/class="[^"]*\bev\b[^"]*\bloss\b/);
	});

	it('draws an unpriced chip price as a dash', () => {
		expect(texts(html(UNPRICED), 'span', 'price')).toEqual(['—']);
	});

	it('draws the UNPRICED mark on an unpriced chip', () => {
		expect(texts(html(UNPRICED), 'span', 'mark')).toEqual(['UNPRICED']);
	});

	it('draws no UNPRICED mark on a priced chip', () => {
		expect(texts(html(HOLLOW), 'span', 'mark')).toEqual([]);
	});

	it('draws the item icon from the api base and the chip icon path', () => {
		expect(html(HOLLOW)).toContain('src="http://api/icon/currency-exchange/hollow"');
	});

	it('is a button so the keyboard reaches it', () => {
		expect(chipTag(html(HOLLOW))).not.toBe('');
	});

	it('styles a picked chip as picked', () => {
		expect(chipTag(html(PICKED))).toMatch(/class="[^"]*\bpicked\b/);
	});

	it('titles a picked chip with its pick title', () => {
		expect(chipTag(html(PICKED))).toContain(`title="${PICK_TITLE_KEPT}"`);
	});

	it('carries no picked style on a computed chip', () => {
		expect(chipTag(html(HOLLOW))).not.toMatch(/class="[^"]*\bpicked\b/);
	});

	it('carries no title on a computed chip', () => {
		expect(chipTag(html(HOLLOW))).not.toContain('title=');
	});
});
