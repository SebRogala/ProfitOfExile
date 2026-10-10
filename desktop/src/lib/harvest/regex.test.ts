import { describe, it, expect } from 'vitest';
import { stashRegex, STASH_SEARCH_LIMIT } from './regex';
import harvest from './__fixtures__/harvest.json';

/**
 * Full names come from the fixture's family type lists; the chosen sides are
 * the reference screens' default keep sets (01, 02, 04), named by short name
 * here so the regex is tested apart from the engine that picks them.
 */

type FamilyId = 'fossil' | 'delirium' | 'corrupt';

function familyNames(id: FamilyId): string[] {
	const family = harvest.families.find((f) => f.id === id);
	if (!family?.weights) throw new Error(`no weighted fixture family ${id}`);
	return family.weights.types.map((t) => t.name);
}

function side(id: FamilyId, keeperShortNames: string[], keepers: boolean): string[] {
	const family = harvest.families.find((f) => f.id === id)!;
	return family
		.weights!.types.filter((t) => keeperShortNames.includes(t.shortName) === keepers)
		.map((t) => t.name);
}

const FOSSIL_KEEPERS = ['Hollow', 'Faceted', 'Sanctified', 'Gilded', 'Glyphic', 'Fractured', 'Shuddering'];
const DELIRIUM_KEEPERS = ["Diviner's", 'Skittering'];
const CORRUPT_KEEPERS = ['Horror'];

describe('stashRegex', () => {
	it('Fossil default keepers give "shu|san|gil|fac|fra|hol|gly", 29 characters', () => {
		const r = stashRegex(side('fossil', FOSSIL_KEEPERS, true), familyNames('fossil'));
		expect(r.text).toBe('"shu|san|gil|fac|fra|hol|gly"');
		expect(r.length).toBe(29);
	});

	it('Fossil default feeders give 53 characters starting "met|jag|pri|den|fri|sco|abe|l', () => {
		const r = stashRegex(side('fossil', FOSSIL_KEEPERS, false), familyNames('fossil'));
		expect(r.text.startsWith('"met|jag|pri|den|fri|sco|abe|l')).toBe(true);
		expect(r.length).toBe(53);
	});

	it('Delirium keepers give "div|ski", 9 characters', () => {
		const r = stashRegex(side('delirium', DELIRIUM_KEEPERS, true), familyNames('delirium'));
		expect(r.text).toBe('"div|ski"');
		expect(r.length).toBe(9);
	});

	it('Delirium feeders give 41 characters starting "bla|arm|jew|fin|car|whi|sin|t', () => {
		const r = stashRegex(side('delirium', DELIRIUM_KEEPERS, false), familyNames('delirium'));
		expect(r.text.startsWith('"bla|arm|jew|fin|car|whi|sin|t')).toBe(true);
		expect(r.length).toBe(41);
	});

	it('Corrupted feeders give "hys|ins|del", 13 characters', () => {
		const r = stashRegex(side('corrupt', CORRUPT_KEEPERS, false), familyNames('corrupt'));
		expect(r.text).toBe('"hys|ins|del"');
		expect(r.length).toBe(13);
	});

	it('Corrupted keepers give "hor", 5 characters', () => {
		const r = stashRegex(side('corrupt', CORRUPT_KEEPERS, true), familyNames('corrupt'));
		expect(r.text).toBe('"hor"');
		expect(r.length).toBe(5);
	});

	it('joins fragments in family order, not in the order the names were passed', () => {
		const r = stashRegex(['Essence of Horror', 'Essence of Hysteria'], familyNames('corrupt'));
		expect(r.text).toBe('"hys|hor"');
	});

	it('skips a run that crosses a space', () => {
		// "ab " is the first run unique against "zzz"; the letters-only rule passes over it to "cdx".
		const r = stashRegex(['ab cdx'], ['ab cdx', 'zzz']);
		expect(r.text).toBe('"cdx"');
	});

	it('skips a run that crosses an apostrophe', () => {
		// "ab'" is the first run unique against "zzz"; the letters-only rule passes over it to "cdx".
		const r = stashRegex(["ab'cdx"], ["ab'cdx", 'zzz']);
		expect(r.text).toBe('"cdx"');
	});

	it('takes a longer run when no 3-letter run is unique', () => {
		// Every 3-letter run of "abcd" sits in another family name, so the 4-letter run is the shortest.
		const r = stashRegex(['abcd'], ['abcd', 'xabcx', 'ybcdy']);
		expect(r.text).toBe('"abcd"');
	});

	// Test-local names: "z" + two letters is unique against the other names, so each adds
	// 4 characters (3 + "|"); one "abcd" adds 5. 61 × 4 + 5 + 2 quotes − 1 leading "|" = 250.
	const shortNames = (count: number) => {
		const letters = 'efghijklmnop';
		const names: string[] = [];
		for (const a of letters) for (const b of letters) names.push(`z${a}${b}`);
		return names.slice(0, count);
	};
	const limitFamily = (count: number) => [...shortNames(count), 'abcd', 'xabcx', 'ybcdy'];

	it('exactly 250 characters is within the limit', () => {
		const r = stashRegex([...shortNames(61), 'abcd'], limitFamily(61));
		expect(r.length).toBe(STASH_SEARCH_LIMIT);
		expect(r.overLimit).toBe(false);
	});

	it('254 characters is over the limit', () => {
		const r = stashRegex([...shortNames(62), 'abcd'], limitFamily(62));
		expect(r.length).toBe(254);
		expect(r.overLimit).toBe(true);
	});

	it('rejects a name contained in another family name, naming it', () => {
		// "Orb" sits inside "Orbit": no run of it is unique.
		expect(() => stashRegex(['Orb'], ['Orb', 'Orbit'])).toThrow('"Orb"');
	});
});
