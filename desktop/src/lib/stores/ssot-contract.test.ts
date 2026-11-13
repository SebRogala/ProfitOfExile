// Reader of desktop/contracts/ssot-snapshot.json and ssot-snapshot-unmeasured.json.
// The writers are the Rust tests ssot::tests::ssot_snapshot_contract_*
// (desktop/src-tauri/src/ssot.rs);
// regenerate from the repo root with
// `docker compose run --rm -T -e SSOT_CONTRACT_UPDATE=1 -w /app/desktop/src-tauri desktop cargo test --lib ssot::tests::ssot_snapshot_contract`.
//
// Each table below is a `Record<keyof T, true>`, so `npm run check` fails when a
// TS key is added or removed without the table; the runtime assertion then
// fails when the Rust wire keys and the table disagree.
import { describe, it, expect } from 'vitest';
import golden from '../../../contracts/ssot-snapshot.json';
import unmeasuredGolden from '../../../contracts/ssot-snapshot-unmeasured.json';
import type {
	Anchors,
	BossLootSlice,
	Placements,
	ScreenSlice,
	SsotSnapshot
} from './ssot.svelte';
import type { MercenarySlice } from '../mercenaries/capture';
import type { TempleSlice } from '../temple/slice';

const snapshotKeys: Record<keyof SsotSnapshot, true> = {
	league: true,
	resolving: true,
	unreachable: true,
	normalVariant: true,
	dedicationVariant: true,
	dedicationPool: true,
	modules: true,
	bossLoot: true,
	mercenary: true,
	temple: true,
	screen: true,
	placements: true
};

const leagueKeys: Record<keyof SsotSnapshot['league'], true> = {
	name: true,
	generation: true
};

const bossLootKeys: Record<keyof BossLootSlice, true> = {
	enabled: true,
	settings: true,
	numberFormat: true,
	pending: true,
	frame: true,
	rebind: true,
	nowMs: true
};

const mercenaryKeys: Record<keyof MercenarySlice, true> = {
	status: true,
	capture: true,
	learnedFamilies: true,
	pooledFamilies: true,
	seededFamilies: true,
	lastError: true,
	burstSpeaker: true,
	geometrySource: true,
	sourcesOff: true,
	sync: true,
	trade: true,
	tradeAuto: true,
	tierFloor: true
};

const templeKeys: Record<keyof TempleSlice, true> = {
	status: true,
	waitingForPanel: true,
	readRetry: true,
	layout: true,
	panel: true,
	advice: true,
	mode: true,
	config: true,
	profile: true,
	preset: true,
	custom: true,
	market: true,
	pollMarket: true,
	unknownRooms: true,
	lastReadAt: true,
	calibration: true,
	readNotice: true,
	lastError: true
};

const screenKeys: Record<keyof ScreenSlice, true> = {
	width: true,
	height: true,
	uiScale: true,
	source: true,
	measuredAtMs: true,
	verifiedThisSession: true,
	monitorId: true,
	origin: true,
	client: true,
	anchors: true
};

const anchorsKeys: Record<keyof Anchors, true> = {
	templeEntrance: true,
	mercPanel: true
};

const placementsKeys: Record<keyof Placements, true> = {
	temple: true,
	merc: true,
	lab: true
};

const keysOf = (value: unknown): string[] => {
	expect(value, 'golden slice must be an object').toBeTypeOf('object');
	expect(value, 'golden slice must not be null').not.toBeNull();
	return Object.keys(value as object).sort();
};

const wire = golden as Record<string, unknown>;

describe('ssot-snapshot.json golden matches SsotSnapshot', () => {
	it('carries exactly the SsotSnapshot top-level keys', () => {
		expect(Object.keys(wire).sort()).toEqual(Object.keys(snapshotKeys).sort());
	});

	it.each([
		['league', leagueKeys],
		['bossLoot', bossLootKeys],
		['mercenary', mercenaryKeys],
		['temple', templeKeys],
		['screen', screenKeys],
		['placements', placementsKeys]
	] as const)('carries exactly the %s slice keys', (slice, table) => {
		expect(keysOf(wire[slice])).toEqual(Object.keys(table).sort());
	});

	it('carries exactly the screen.anchors keys', () => {
		const screen = wire.screen as Record<string, unknown>;
		expect(keysOf(screen.anchors)).toEqual(Object.keys(anchorsKeys).sort());
	});

	// ssot.svelte.ts SsotSnapshot.screen: an unmeasured screen is sent as null,
	// never omitted.
	it('sends an unmeasured screen as a literal null key', () => {
		const unmeasured = unmeasuredGolden as Record<string, unknown>;
		expect(Object.prototype.hasOwnProperty.call(unmeasured, 'screen')).toBe(true);
		expect(unmeasured.screen).toBeNull();
	});
});
