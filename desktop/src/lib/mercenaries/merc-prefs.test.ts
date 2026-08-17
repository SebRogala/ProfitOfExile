import { describe, it, expect } from 'vitest';
import { enabledSources, parseSourcesOff } from './merc-prefs';
import { SOURCE_IDS } from './rulesets';

describe('parseSourcesOff', () => {
	it('reads a persisted source id as switched off', () => {
		expect([...parseSourcesOff('guide-a')]).toEqual(['guide-a']);
	});

	it('reads an empty preference as nothing switched off', () => {
		expect(parseSourcesOff('').size).toBe(0);
	});

	// The preference outlives the code that wrote it. A source that was renamed
	// or removed must not keep switching anything off from beyond the grave.
	it('drops an id that is no longer a source', () => {
		expect([...parseSourcesOff('guide-a,guide-zzz')]).toEqual(['guide-a']);
	});

	it('tolerates the spaces a hand-edited settings file picks up', () => {
		expect([...parseSourcesOff(' guide-b , guide-a ')]).toEqual(['guide-b', 'guide-a']);
	});

	it('switches off every source when the preference names them all', () => {
		expect(parseSourcesOff(SOURCE_IDS.join(',')).size).toBe(SOURCE_IDS.length);
	});
});

describe('enabledSources', () => {
	it('returns every source when nothing is switched off', () => {
		expect([...enabledSources('')]).toEqual([...SOURCE_IDS]);
	});

	it('drops the switched-off source and keeps the rest', () => {
		expect([...enabledSources('guide-a')]).toEqual(SOURCE_IDS.filter((id) => id !== 'guide-a'));
	});

	it('returns nothing when every source is switched off', () => {
		expect(enabledSources(SOURCE_IDS.join(',')).size).toBe(0);
	});
});
