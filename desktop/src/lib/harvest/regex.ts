/**
 * Stash-search regex for one side of a Harvest family (README § Engine, the
 * Regex bullet).
 *
 * Each chosen type contributes the shortest lowercase run of at least three
 * letters (no space, no apostrophe) of its full name that appears in no OTHER
 * full name of the same family; ties go to the first occurrence. Fragments are
 * joined with `|` in family type order and wrapped in double quotes, and the
 * length counts the quotes against PoE 1's stash-search cap.
 *
 * Uniqueness is checked against the whole family, not just the other side:
 * the stash holds both, so a feeder fragment that also matched a keeper would
 * light up the keeper too.
 */

/** PoE 1's stash search accepts at most this many characters. */
export const STASH_SEARCH_LIMIT = 250;

const MIN_FRAGMENT = 3;
const LETTERS_ONLY = /^[a-z]+$/;

export interface StashRegex {
	text: string;
	length: number;
	overLimit: boolean;
}

/**
 * Shortest unique run of `name` among `others`. A name with no unique run (it
 * is contained in another name) breaks the generator's uniqueness invariant,
 * so it throws naming the type rather than widening or dropping its search.
 * No current fixture family reaches this: all 56 weighted types have a unique run.
 */
function fragment(name: string, others: string[]): string {
	const lower = name.toLowerCase();
	const lowerOthers = others.map((o) => o.toLowerCase());
	for (let size = MIN_FRAGMENT; size <= lower.length; size++) {
		for (let start = 0; start + size <= lower.length; start++) {
			const run = lower.slice(start, start + size);
			if (!LETTERS_ONLY.test(run)) continue;
			if (lowerOthers.every((o) => !o.includes(run))) return run;
		}
	}
	throw new Error(`stashRegex: no unique fragment of at least ${MIN_FRAGMENT} letters for "${name}"`);
}

/**
 * @param names the full names on this side (keepers or feeders)
 * @param familyNames every full name of the family, in family type order
 */
export function stashRegex(names: string[], familyNames: string[]): StashRegex {
	const chosen = new Set(names);
	const fragments = familyNames
		.filter((name) => chosen.has(name))
		.map((name) => fragment(name, familyNames.filter((other) => other !== name)));
	const text = `"${fragments.join('|')}"`;
	return { text, length: text.length, overLimit: text.length > STASH_SEARCH_LIMIT };
}
