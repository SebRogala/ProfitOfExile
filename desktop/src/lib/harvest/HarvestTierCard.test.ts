/**
 * The tier card, rendered with `svelte/server` (no DOM): its header (tier
 * name, split mark, meta, move label) and one chip per prop chip. Which tier
 * is split and what the meta says is `tierCards()`'s decision
 * (`view.test.ts`); the tier move is the controller's (WI-8).
 *
 * Visible text is read from its own element (`texts`), so a string elsewhere
 * in the markup cannot stand in for a dropped one.
 */
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import HarvestTierCard from './HarvestTierCard.svelte';
import type { Chip, TierCard } from './view';

function chip(shortName: string, priceText: string): Chip {
	return {
		itemId: `id-${shortName}`,
		name: `${shortName} Fossil`,
		shortName,
		icon: `/icon/${shortName}`,
		priceText,
		unpriced: false,
		picked: false,
		pickTitle: null,
		evText: '+1.8c',
		evTone: 'gain',
		shareText: null
	};
}

const FEEDER_MID: TierCard = {
	tier: 'MID',
	split: true,
	meta: '7 types · 30.0c–109c',
	moveLabel: '← Keep tier',
	moveTo: 'keep',
	chips: [chip('Dense', '33.1c'), chip('Corroded', '30.0c')]
};

const KEEPER_TOP: TierCard = {
	tier: 'TOP',
	split: false,
	meta: '1 type · 402c',
	moveLabel: 'Feed tier →',
	moveTo: 'reroll',
	chips: [{ ...chip('Hollow', '402c'), evText: null, evTone: null, shareText: '0.02%' }]
};

function html(card: TierCard): string {
	return render(HarvestTierCard, {
		props: { card, apiBase: 'http://api', onmove: () => {}, onpick: () => {} }
	}).body;
}

/** Inner text of every `<tag>` whose class list holds `cls` (Svelte comments and scope classes tolerated). */
function texts(markup: string, tag: string, cls: string): string[] {
	const re = new RegExp(`<${tag}\\b[^>]*class="[^"]*\\b${cls}\\b[^"]*"[^>]*>([\\s\\S]*?)</${tag}>`, 'g');
	return [...markup.matchAll(re)].map((m) => m[1].replace(/<!--[\s\S]*?-->/g, '').trim());
}

/** Text of every button that is not a chip (the move button). */
function moveButtons(markup: string): string[] {
	const re = /<button\b(?![^>]*class="[^"]*\bchip\b)[^>]*>([\s\S]*?)<\/button>/g;
	return [...markup.matchAll(re)].map((m) => m[1].replace(/<!--[\s\S]*?-->/g, '').trim());
}

describe('HarvestTierCard', () => {
	it('draws the tier name in its tier element', () => {
		expect(texts(html(FEEDER_MID), 'span', 'tier')).toEqual(['MID']);
	});

	it('draws the meta line in its meta element', () => {
		expect(texts(html(FEEDER_MID), 'span', 'meta')).toEqual(['7 types · 30.0c–109c']);
	});

	it('draws the SPLIT mark on a split tier', () => {
		expect(texts(html(FEEDER_MID), 'span', 'mark')).toEqual(['SPLIT']);
	});

	it('draws no SPLIT mark on a whole tier', () => {
		expect(texts(html(KEEPER_TOP), 'span', 'mark')).toEqual([]);
	});

	it('labels a feeder card move button "← Keep tier"', () => {
		expect(moveButtons(html(FEEDER_MID))).toEqual(['← Keep tier']);
	});

	it('labels a keeper card move button "Feed tier →"', () => {
		expect(moveButtons(html(KEEPER_TOP))).toEqual(['Feed tier →']);
	});

	it('draws exactly one chip per prop chip, in prop order', () => {
		expect(texts(html(FEEDER_MID), 'span', 'name')).toEqual(['Dense', 'Corroded']);
	});

	it('gives the chips of a keeper card the keeper style', () => {
		expect(html(KEEPER_TOP)).toMatch(/<button\b[^>]*class="[^"]*\bchip\b[^"]*\bkept\b/);
	});

	it('gives the chips of a feeder card no keeper style', () => {
		expect(html(FEEDER_MID)).not.toMatch(/class="[^"]*\bkept\b/);
	});
});
