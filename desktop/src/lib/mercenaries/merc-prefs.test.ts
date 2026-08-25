import { describe, it, expect } from 'vitest';
import { enabledSources, parseSourcesOff, withSourceEnabled } from './merc-prefs';
import { SOURCE_IDS } from './rulesets';

describe('parseSourcesOff', () => {
	it('reads a stored source id as switched off', () => {
		expect([...parseSourcesOff(['guide-a'])]).toEqual(['guide-a']);
	});

	it('reads an empty list as nothing switched off', () => {
		expect(parseSourcesOff([]).size).toBe(0);
	});

	// The slice crosses a version boundary. A guide renamed or removed between
	// builds must not keep switching anything off from beyond the grave.
	it('drops an id that is no longer a source', () => {
		expect([...parseSourcesOff(['guide-a', 'guide-zzz'])]).toEqual(['guide-a']);
	});

	// Until the first poll answers the slice is the local default, and an older
	// Rust that does not send the field at all leaves it undefined. Both mean
	// "nothing known to be off", never "everything off".
	it('reads a missing list as nothing switched off', () => {
		expect(parseSourcesOff(undefined).size).toBe(0);
	});

	it('switches off every source when the list names them all', () => {
		expect(parseSourcesOff([...SOURCE_IDS]).size).toBe(SOURCE_IDS.length);
	});
});

describe('enabledSources', () => {
	it('returns every source when nothing is switched off', () => {
		expect([...enabledSources([])]).toEqual([...SOURCE_IDS]);
	});

	it('drops the switched-off source and keeps the rest', () => {
		expect([...enabledSources(['guide-a'])]).toEqual(SOURCE_IDS.filter((id) => id !== 'guide-a'));
	});

	it('returns nothing when every source is switched off', () => {
		expect(enabledSources([...SOURCE_IDS]).size).toBe(0);
	});
});

describe('withSourceEnabled', () => {
	it('adds the source to the off-list when it is switched off', () => {
		expect(withSourceEnabled([], 'guide-b', false)).toEqual(['guide-b']);
	});

	it('takes the source out of the off-list when it is switched on', () => {
		expect(withSourceEnabled(['guide-a', 'guide-b'], 'guide-a', true)).toEqual(['guide-b']);
	});

	// Whatever order the user clicked in, one stored value: a list that flipped
	// order on every toggle would read as a change on every settings write.
	it('writes the off-list in registry order', () => {
		expect(withSourceEnabled(['guide-b'], 'guide-a', false)).toEqual(['guide-a', 'guide-b']);
	});

	it('leaves the list alone when the source is already in the state asked for', () => {
		expect(withSourceEnabled(['guide-a'], 'guide-a', false)).toEqual(['guide-a']);
	});
});
