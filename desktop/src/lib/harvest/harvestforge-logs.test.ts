import { describe, it, expect } from 'vitest';
import { loadHarvestFamilies } from './sources/harvest-fixture';
import astrolabesCsv from './__fixtures__/harvestforge/HarvestForge_Astrolabes_20261010_134926.csv?raw';
import catalystsCsv from './__fixtures__/harvestforge/HarvestForge_Catalysts_20261010_023020.csv?raw';
import oilsCsv from './__fixtures__/harvestforge/HarvestForge_Oils_20261010_000148.csv?raw';

/**
 * The served weights of Astrolabes, Catalysts and Oils must match the
 * committed HarvestForge logs they came from. Lifeforce spent and reroll cost
 * are NOT read from the logs: HarvestForge's footer undercounts lifeforce
 * spent, so they are pinned as literals (the Operator's figures, 2026-10-10).
 */

interface HarvestForgeLog {
	types: [name: string, rolls: number][];
	totalRolls: number;
}

/** Type rows up to the first blank line, then `key,value` footer lines. */
function parseLog(csv: string): HarvestForgeLog {
	const lines = csv.replace(/^﻿/, '').split(/\r?\n/);
	const blank = lines.indexOf('');
	const types = lines.slice(1, blank).map((line): [string, number] => {
		const [name, rolls] = line.split(',');
		return [name.replace(/^"|"$/g, ''), Number(rolls)];
	});
	const footer = new Map(lines.slice(blank + 1).map((line) => line.split(',') as [string, string]));
	return { types, totalRolls: Number(footer.get('Total rolls')) };
}

const LOGS = [
	['astrolabe', parseLog(astrolabesCsv)],
	['catalyst', parseLog(catalystsCsv)],
	['oil', parseLog(oilsCsv)]
] as const;

async function family(id: string) {
	const data = await loadHarvestFamilies('day');
	const found = data.families.find((f) => f.id === id);
	if (!found) throw new Error(`family ${id} missing`);
	return found;
}

describe('HarvestForge logs behind Astrolabes, Catalysts and Oils', () => {
	it.each(LOGS)('serves %s weights, sampled over the log’s total rolls', async (id, log) => {
		expect((await family(id)).weights?.sample.rolls).toBe(log.totalRolls);
	});

	it.each(LOGS)('serves exactly the %s log’s types, in log order, with their logged rolls', async (id, log) => {
		const types = (await family(id)).weights?.types.map((t) => [t.name, t.loggedRolls]);
		expect(types).toEqual(log.types);
	});

	it.each([
		['astrolabe', 'Wild', 400, 804000],
		['catalyst', 'Wild', 30, 75120],
		['oil', 'Primal', 30, 78180]
	] as const)(
		'serves %s at the Operator’s figures: %s lifeforce, %i a reroll, %i spent',
		async (id, lifeforce, rerollCost, lifeforceSpent) => {
			const f = await family(id);
			expect([f.lifeforce, f.rerollCost, f.weights?.sample.lifeforceSpent]).toEqual([
				lifeforce,
				rerollCost,
				lifeforceSpent
			]);
		}
	);
});
