import { describe, it, expect } from 'vitest';
import offerBoxesSource from './TempleOfferBoxes.svelte?raw';
import {
	COL_PITCH,
	EDGE_STATE_LABEL,
	ENTRANCE_DROP,
	LEAVE_MAP_ACTION,
	OVERLAY_VISIBLE_STATUSES,
	PLATE_H,
	PLATE_W,
	ROW_PITCH,
	SLOT_IDS,
	TEMPLE_STATUS_LABEL,
	TEMPLE_STATUS_TONE,
	edgeId,
	edgeState,
	formatRisk,
	gambleLabel,
	incursionsText,
	latticeEdges,
	latticePoints,
	latticeViewBox,
	forcedKillNote,
	leaveMapBanner,
	markerFallbackNotice,
	modeLabel,
	moveLine,
	offerBuilds,
	offerHeadline,
	overlayShowsBoard,
	overlayShowsDoors,
	overlayShowsWaiting,
	plateGlyph,
	convenienceDoor,
	convenienceNote,
	doorWidget,
	faintDoor,
	recommendedExit,
	secondDoor,
	suggestedDoors,
	topGamble,
	topRecommendation,
	unknownRoomsBadge,
	chosenOffer,
	doorWarning,
	marketNote,
	marketStale,
	offerBoxes,
	offerBoxSignature,
	offerChaos,
	type OfferBox
} from './view';
import { templeSliceDefault, type AdviceView, type DriverView, type ItemSlotId, type LayoutView, type MarketView, type ModWorth, type OfferView, type RankedView, type RoomValueView, type SlotId, type SlotView, type TempleStatus } from './slice';

/** A fixed clock, so every age below is the difference the test states. */
const NOW = 1_788_665_199_649;
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

/** The market a build that has never reached a server is on — Rust's own
 *  default, taken from the mirror rather than retyped. */
const NO_MARKET: MarketView = templeSliceDefault().market;

/** The line an age is judged against — Rust's `market::STALE_AFTER_MS`, taken
 *  from the mirror rather than retyped, like `NO_MARKET` above. */
const STALE_AFTER = NO_MARKET.staleAfterMs;

/** Every wire status, listed once so the totality checks below cannot drift. */
const ALL_STATUSES: TempleStatus[] = [
	'off',
	'idle',
	'waiting',
	'panel_not_visible',
	'reading',
	'read',
	'no_current_room',
	'unavailable',
	'error'
];

function layout(over: Partial<LayoutView> = {}): LayoutView {
	return {
		slots: [],
		doors: [],
		uncertain: [],
		unresolvedIncident: [],
		markerError: null,
		current: null,
		scale: 1,
		ncc: 0.95,
		confidence: 'high',
		origin: [0, 0],
		// Nothing in `view.ts` reads the published lattice — it draws the board
		// on its own `PLATE_CENTRES` — but `LayoutView.centres` is a 13-tuple
		// mirroring Rust's `[[i32; 2]; 13]`, so the degenerate board this
		// fixture describes still has to carry all thirteen.
		centres: [
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0]
		],
		// The never-cover set and the room's diamond (POE-244). Empty here:
		// nothing in `view.ts` reads either — `overlay-geometry.ts` does — and a
		// fixture that carried 42 rects nothing asserts on would be noise.
		rois: [],
		diamond: null,
		...over
	};
}

/**
 * The room's shape, as Rust publishes it.
 *
 * Nothing in `view.ts` reads a coordinate off it — `overlay-geometry.ts` does —
 * so the numbers are a placeholder and only its PRESENCE is asserted: it is
 * what `overlayShowsDoors` tests for, because a move with no room to draw it on
 * has nothing to show.
 */
function diamond() {
	return {
		corners: [
			[1, 0],
			[0, 1],
			[-1, 0],
			[0, -1]
		] as [[number, number], [number, number], [number, number], [number, number]],
		seals: [],
		topIcon: [0.34, -0.3] as [number, number],
		bottomIcon: [-0.34, 0.3] as [number, number]
	};
}

function ranked(over: Partial<RankedView> = {}): RankedView {
	return {
		headline: 'upgrade → Locus of Corruption',
		doorsLabel: 'C1-C2',
		doors: ['C1-C2'],
		architectIndex: 0,
		ev: 12,
		risk: null,
		reasons: ['R1: connects toward the top', 'RS: C1 is the scarcest hub left'],
		...over
	};
}

function advice(over: Partial<AdviceView> = {}): AdviceView {
	return {
		recommendations: [ranked()],
		gambles: [],
		mapAction: 'continue',
		warnings: [],
		forcedKill: false,
		...over
	};
}

function offer(over: Partial<OfferView> = {}): OfferView {
	return {
		index: 0,
		architectName: 'Guatelitzi',
		kind: 'upgrade',
		printedTarget: "Sadist's Den",
		displayName: 'Torment Cells',
		builtTier: 2,
		// The sadist's-den line's real pair (POE-249), so an offer built by this
		// fixture is one Rust could have published: the letter is the LINE's and
		// the name is the tier-3 room it was given for, which is NOT the tier-2
		// room `displayName` carries.
		grade: 'C',
		lineTop: "Sadist's Den",
		rect: null,
		...over
	};
}

describe('status vocabulary', () => {
	it('words every status the wire can carry', () => {
		// A status added in Rust and forgotten here renders as an empty badge,
		// which reads as "nothing is wrong". Asserted over the list rather than
		// trusting the Record type, because a cast anywhere would hide it.
		for (const status of ALL_STATUSES) {
			expect(TEMPLE_STATUS_LABEL[status], status).toBeTruthy();
			expect(TEMPLE_STATUS_TONE[status], status).toBeTruthy();
		}
		expect(Object.keys(TEMPLE_STATUS_LABEL).sort()).toEqual([...ALL_STATUSES].sort());
	});

	it('gives each status a distinct label', () => {
		// Two statuses sharing wording is the same failure as no wording: the
		// user cannot tell "no panel on screen" from "module off".
		const labels = ALL_STATUSES.map((s) => TEMPLE_STATUS_LABEL[s]);
		expect(new Set(labels).size).toBe(ALL_STATUSES.length);
	});

	it('tones the two dead-end statuses as failures and the read as a pass', () => {
		expect(TEMPLE_STATUS_TONE.unavailable).toBe('fail');
		expect(TEMPLE_STATUS_TONE.error).toBe('fail');
		expect(TEMPLE_STATUS_TONE.read).toBe('pass');
		expect(TEMPLE_STATUS_TONE.off).toBe('muted');
	});
});

describe('overlayShowsBoard', () => {
	it('shows the board only while there is one on screen', () => {
		expect(OVERLAY_VISIBLE_STATUSES).toEqual(['reading', 'read', 'no_current_room']);
		expect(overlayShowsBoard('read')).toBe(true);
		expect(overlayShowsBoard('reading')).toBe(true);
		// Between rooms there is a layout but no advice — still worth drawing.
		expect(overlayShowsBoard('no_current_room')).toBe(true);
	});

	it('shows nothing for every status with no board behind it', () => {
		for (const status of [
			'off',
			'idle',
			'waiting',
			'panel_not_visible',
			'unavailable',
			'error'
		] as const) {
			expect(overlayShowsBoard(status), status).toBe(false);
		}
	});
});

describe('overlayShowsWaiting', () => {
	/** A slice with Alva's start line heard and nothing read yet. */
	function waiting(status: TempleStatus) {
		return { ...templeSliceDefault(), waitingForPanel: true, status };
	}

	it('shows the notice for every status with no board behind it', () => {
		// Derived from the two lists rather than transcribed: `off`,
		// `unavailable` and `error` have no board behind them either, and a
		// hand-written trio silently stops covering a status added to the wire.
		// The `off` and `unavailable` rows are truths about THIS pure function
		// and nothing more — upstream writers make both unreachable on a real
		// slice (`force_off` clears the flag on every composed snapshot, and a
		// capture-less module gets `unavailable()`'s `end_cycle` plus
		// `start_cycle`'s own refusal), which is where that belongs.
		const noBoard = ALL_STATUSES.filter((s) => !OVERLAY_VISIBLE_STATUSES.includes(s));
		for (const status of noBoard) {
			expect(overlayShowsWaiting(waiting(status)), status).toBe(true);
		}
	});

	it('never draws the notice over a board', () => {
		// Alva's start line fires when the PORTAL OPENS, so it can land with the
		// sheet already on screen and read. Without the second clause the notice
		// would blink over the board the player is reading.
		for (const status of ['reading', 'read'] as const) {
			expect(overlayShowsWaiting(waiting(status)), status).toBe(false);
		}
	});

	it('shows nothing while no cycle is waiting, whatever the status', () => {
		// The flag is the first half of the gate: without it a module that is
		// merely idle would claim to be waiting for a panel nobody opened. Over
		// the whole wire vocabulary, because "whatever the status" is the claim.
		for (const status of ALL_STATUSES) {
			expect(
				overlayShowsWaiting({ ...templeSliceDefault(), status }),
				status
			).toBe(false);
		}
	});
});

describe('lattice geometry', () => {
	const points = latticePoints();

	it('places all 13 slots, in Slot::ALL order', () => {
		expect(points).toHaveLength(13);
		expect(points.map((p) => p.slot)).toEqual([...SLOT_IDS]);
	});

	it('uses the measured x offsets, row by row', () => {
		// The table from lattice.rs, transcribed independently of the source so
		// a wrong offset there fails here rather than being agreed with.
		const x = (slot: SlotId) => points.find((p) => p.slot === slot)!.x;
		expect(x('A0')).toBe(0);
		expect([x('B0'), x('B1')]).toEqual([-106, 106]);
		expect([x('C0'), x('C1'), x('C2')]).toEqual([-212, 0, 212]);
		expect([x('D0'), x('D1'), x('D2'), x('D3')]).toEqual([-318, -106, 106, 318]);
		expect([x('E0'), x('E1'), x('E2')]).toEqual([-212, 0, 212]);
	});

	it('separates the rows by one row pitch, with the Entrance at the origin', () => {
		const y = (slot: SlotId) => points.find((p) => p.slot === slot)!.y;
		expect(y('E1')).toBe(0);
		expect(y('E0')).toBe(-ENTRANCE_DROP);
		expect(y('D1') - y('E0')).toBe(-ROW_PITCH);
		expect(y('C1') - y('D1')).toBe(-ROW_PITCH);
		expect(y('B0') - y('C1')).toBe(-ROW_PITCH);
		expect(y('A0') - y('B0')).toBe(-ROW_PITCH);
	});

	it('drops the Entrance 19 px below its two row-E siblings', () => {
		const y = (slot: SlotId) => points.find((p) => p.slot === slot)!.y;
		// `+y` is down, so the Entrance's y is the LARGER one.
		expect(y('E1') - y('E0')).toBe(ENTRANCE_DROP);
		expect(y('E1') - y('E2')).toBe(ENTRANCE_DROP);
	});

	it('derives exactly the 26 corridors Rust derives, by name and in order', () => {
		// Copied from `edge_derivation_yields_the_twenty_six_measured_corridors`
		// in lattice.rs. Both sides derive the set from their own offsets; this
		// is where a transcription error in either table surfaces.
		expect(latticeEdges().map((e) => e.id)).toEqual([
			'A0-B0', 'A0-B1', 'B0-B1', 'B0-C0', 'B0-C1', 'B1-C1', 'B1-C2', 'C0-C1', 'C0-D0',
			'C0-D1', 'C1-C2', 'C1-D1', 'C1-D2', 'C2-D2', 'C2-D3', 'D0-D1', 'D0-E0', 'D1-D2',
			'D1-E0', 'D1-E1', 'D2-D3', 'D2-E1', 'D2-E2', 'D3-E2', 'E0-E1', 'E1-E2'
		]);
	});

	it('gives every slot the degree Rust measured on the board', () => {
		// From `slot_degrees_match_the_measured_board`. Degree is POE-170's
		// scarcity input, so a wrong one is a wrong recommendation drawn on a
		// wrong board — not a cosmetic slip.
		const degree: Record<SlotId, number> = {
			A0: 0, B0: 0, B1: 0, C0: 0, C1: 0, C2: 0, D0: 0, D1: 0, D2: 0, D3: 0, E0: 0, E1: 0, E2: 0
		};
		for (const edge of latticeEdges()) {
			degree[edge.a] += 1;
			degree[edge.b] += 1;
		}
		expect(degree).toEqual({
			A0: 2, B0: 4, B1: 4, C0: 4, C1: 6, C2: 4, D0: 3, D1: 6, D2: 6, D3: 3, E0: 3, E1: 4, E2: 3
		});
	});

	it('classifies the two corridor families by their geometry', () => {
		const kindOf = (id: string) => latticeEdges().find((e) => e.id === id)!.kind;
		// Same row, one column pitch apart.
		expect(kindOf('C1-C2')).toBe('horizontal');
		// Across the Entrance drop — still horizontal, which is what the
		// tolerance in the rule exists for.
		expect(kindOf('E0-E1')).toBe('horizontal');
		// Half a column pitch and one row pitch.
		expect(kindOf('B0-C1')).toBe('diagonal');
	});

	it('draws every edge between the centres of the slots it names', () => {
		const at = (slot: SlotId) => points.find((p) => p.slot === slot)!;
		for (const edge of latticeEdges()) {
			expect([edge.x1, edge.y1], edge.id).toEqual([at(edge.a).x, at(edge.a).y]);
			expect([edge.x2, edge.y2], edge.id).toEqual([at(edge.b).x, at(edge.b).y]);
		}
	});

	it('orders an edge label the way Rust orders its endpoints, whichever way it is asked', () => {
		expect(edgeId('C1', 'C2')).toBe('C1-C2');
		expect(edgeId('C2', 'C1')).toBe('C1-C2');
		expect(edgeId('E1', 'D2')).toBe('D2-E1');
	});

	it('scales to a viewBox that contains every plate with margin to spare', () => {
		const box = latticeViewBox(0);
		// Every plate, at its full size, inside the box.
		for (const p of points) {
			expect(p.x - PLATE_W / 2, p.slot).toBeGreaterThanOrEqual(box.minX);
			expect(p.x + PLATE_W / 2, p.slot).toBeLessThanOrEqual(box.minX + box.width);
			expect(p.y - PLATE_H / 2, p.slot).toBeGreaterThanOrEqual(box.minY);
			expect(p.y + PLATE_H / 2, p.slot).toBeLessThanOrEqual(box.minY + box.height);
		}
		// The board is 4 column pitches wide and 4 row pitches plus the drop tall.
		expect(box.width).toBe(2 * COL_PITCH * 1.5 + PLATE_W);
		expect(box.height).toBe(4 * ROW_PITCH + ENTRANCE_DROP + PLATE_H);
	});

	it('adds the margin it is asked for on all four sides', () => {
		const tight = latticeViewBox(0);
		const padded = latticeViewBox(10);
		expect(padded.minX).toBe(tight.minX - 10);
		expect(padded.minY).toBe(tight.minY - 10);
		expect(padded.width).toBe(tight.width + 20);
		expect(padded.height).toBe(tight.height + 20);
	});
});

describe('edgeState', () => {
	it('calls a settled door open', () => {
		expect(edgeState('C1-C2', layout({ doors: ['C1-C2'] }))).toBe('open');
	});

	it('keeps a settled door open even though the beam flagged it uncertain', () => {
		// POE-248's live bug, as a fixture: this is the shape a SUCCESSFUL read
		// publishes. `doors.rs` puts every corridor incident to the current room
		// in `uncertain` before any judgement — the selection frame covers their
		// midpoints — and the diamond read then settles them into `doors`. The
		// old rule tested `uncertain` and drew C1-C2 grey where the game's own
		// seal was green.
		expect(edgeState('C1-C2', layout({ doors: ['C1-C2'], uncertain: ['C1-C2'] }))).toBe(
			'open'
		);
	});

	it('marks an unresolved corridor even though it is also uncertain', () => {
		// `unresolvedIncident` is a SUBSET of `uncertain` — the fallback flags a
		// corridor uncertain and then reports it as unresolved when nothing
		// settled it. "We could not see it" must not render as "it is shut".
		const l = layout({ doors: [], uncertain: ['B0-C1'], unresolvedIncident: ['B0-C1'] });
		expect(edgeState('B0-C1', l)).toBe('unresolved');
	});

	it('marks unresolved ahead of open even when the edge is also in doors', () => {
		// Today's fallback publishes `doors = doors − uncertain`, so this exact
		// payload does not occur — which is the point. The precedence is the
		// honesty guard, and a rule only checked on inputs that cannot
		// distinguish it is a rule that can be reordered away unnoticed.
		const l = layout({
			doors: ['B0-C1'],
			uncertain: ['B0-C1'],
			unresolvedIncident: ['B0-C1']
		});
		expect(edgeState('B0-C1', l)).toBe('unresolved');
	});

	it('calls a corridor in neither set closed', () => {
		expect(edgeState('A0-B0', layout({ doors: ['C1-C2'] }))).toBe('closed');
	});

	it('calls everything closed with no layout published', () => {
		expect(edgeState('C1-C2', null)).toBe('closed');
	});

	it('words all three states', () => {
		for (const state of ['open', 'unresolved', 'closed'] as const) {
			expect(EDGE_STATE_LABEL[state], state).toBeTruthy();
		}
		expect(new Set(Object.values(EDGE_STATE_LABEL)).size).toBe(3);
	});
});

describe('plateGlyph', () => {
	/** One plate, as the reader publishes it. */
	function slot(over: Partial<SlotView> = {}): SlotView {
		return { slot: 'C1', name: 'Locus of Corruption', tier: 3, exact: true, known: true, current: false, ...over };
	}

	it('marks a plate that did not resolve', () => {
		// The distinction the compact board exists to keep: unread is junk to
		// the advisor, and a blank plate would read as an empty room.
		expect(plateGlyph(slot({ known: false, name: null, tier: 0 }))).toBe('?');
	});

	it('shows the tier of a plate that did resolve', () => {
		expect(plateGlyph(slot({ tier: 2 }))).toBe('2');
	});

	it('marks a read room that has no tier rather than printing a zero', () => {
		// The Entrance, the Apex and the fillers are tier 0 AND read. A "0"
		// there would read as a tier the game does not have.
		expect(plateGlyph(slot({ tier: 0 }))).toBe('·');
	});

	it('draws nothing for a slot the board carries no entry for', () => {
		expect(plateGlyph(undefined)).toBe('');
	});
});

describe('leaveMapBanner', () => {
	it('banners only the exact wire string R5 sends', () => {
		expect(LEAVE_MAP_ACTION).toBe('leaveMap');
		expect(leaveMapBanner(advice({ mapAction: 'leaveMap' }))).toContain('Leave this map');
	});

	it('says nothing when the advisor said to continue', () => {
		expect(leaveMapBanner(advice({ mapAction: 'continue' }))).toBeNull();
	});

	it('says nothing for a snake_case near-miss', () => {
		// `mapAction` is projected by a hand-written match, NOT by `rename_all`,
		// so it is camelCase while `TempleStatus` is snake_case. A surface that
		// assumed the snake form would silently never banner.
		expect(leaveMapBanner(advice({ mapAction: 'leave_map' }))).toBeNull();
	});

	it('says nothing with no advice at all', () => {
		expect(leaveMapBanner(null)).toBeNull();
	});
});

describe('risk and ranking wording', () => {
	it('formats a risk fraction as a whole percent', () => {
		expect(formatRisk(0.31)).toBe('31%');
		expect(formatRisk(0.315)).toBe('32%');
		expect(formatRisk(0)).toBe('0%');
		expect(formatRisk(1)).toBe('100%');
	});

	it('formats no risk as null, not as 0%', () => {
		// Null is the recommended side: RV never measured it. A "0%" there
		// would claim a measurement that was not taken.
		expect(formatRisk(null)).toBeNull();
	});

	it('refuses a non-finite risk rather than printing NaN%', () => {
		expect(formatRisk(Number.NaN)).toBeNull();
	});

	it('labels a gamble with the word and its risk', () => {
		expect(gambleLabel(ranked({ risk: 0.31 }))).toBe('gamble · 31% risk');
	});

	it('still labels a gamble that carries no risk figure', () => {
		expect(gambleLabel(ranked({ risk: null }))).toBe('gamble');
	});

	it('puts the kill and the doors in one line', () => {
		expect(moveLine(ranked())).toBe('upgrade → Locus of Corruption · C1-C2');
	});

	it('marks a kill the read did not choose', () => {
		// The side panel prints two architect blocks. With one read, the
		// headline is the only kill there was — and a surface that showed it
		// like a ranked choice would claim a decision nothing made.
		expect(forcedKillNote(advice({ forcedKill: true }))).toBe('only architect read');
	});

	it('marks nothing when both architects were read', () => {
		expect(forcedKillNote(advice({ forcedKill: false }))).toBeNull();
		expect(forcedKillNote(null)).toBeNull();
	});

	it('marks nothing for a payload whose flag is missing rather than false', () => {
		// A slice from an older build, or one a rename dropped the key from,
		// reads `undefined` here. Marking every kill forced on that would be a
		// warning the player learns to ignore — so the flag has to be the
		// literal `true` before the mark is drawn.
		const stale = { ...advice(), forcedKill: undefined } as unknown as AdviceView;
		expect(forcedKillNote(stale)).toBeNull();
	});

	it('picks the best recommendation and the best gamble, or null', () => {
		const best = ranked({ headline: 'best' });
		const worse = ranked({ headline: 'worse' });
		expect(topRecommendation(advice({ recommendations: [best, worse] }))?.headline).toBe('best');
		expect(topGamble(advice({ gambles: [best, worse] }))?.headline).toBe('best');
		expect(topRecommendation(advice({ recommendations: [] }))).toBeNull();
		expect(topGamble(advice())).toBeNull();
		expect(topRecommendation(null)).toBeNull();
	});
});

describe('offer wording', () => {
	it('shows the room the kill BUILDS, never the printed name alone', () => {
		// POE-169: Contested Development prints one line and builds
		// `currentTier + 1` of it. A surface showing the printed name is
		// showing a room the player is not getting.
		const text = offerBuilds(offer({ printedTarget: "Sadist's Den", displayName: 'Torment Cells', builtTier: 2 }));
		expect(text).toContain('Torment Cells');
		expect(text).toContain('tier 2');
		expect(text).not.toContain("Sadist's Den");
	});

	it('says the target does not resolve rather than falling back to the printed name', () => {
		const text = offerBuilds(offer({ displayName: null, builtTier: null }));
		expect(text).not.toContain("Sadist's Den");
		expect(text).toContain('does not resolve');
	});

	it('shows a resolved name with no tier when the tier is absent', () => {
		expect(offerBuilds(offer({ displayName: 'Torment Cells', builtTier: null }))).toBe(
			'Torment Cells'
		);
	});

	it('heads an offer with its architect and which kill it is', () => {
		expect(offerHeadline(offer({ architectName: 'Tacati', kind: 'change' }))).toBe(
			'Tacati · change'
		);
	});

	it('says the incursion budget is not legible instead of showing nothing', () => {
		expect(incursionsText(6)).toBe('incursions remaining: 6');
		expect(incursionsText(null)).toContain('not legible');
	});
});

describe('badges', () => {
	it('names the unread plates rather than counting them', () => {
		const slice = { ...templeSliceDefault(), unknownRooms: ['A0', 'D3'] as SlotId[] };
		const badge = unknownRoomsBadge(slice);
		expect(badge).toContain('A0');
		expect(badge).toContain('D3');
		expect(badge).toContain('2 unread plates');
	});

	it('singularises one unread plate', () => {
		const slice = { ...templeSliceDefault(), unknownRooms: ['A0'] as SlotId[] };
		expect(unknownRoomsBadge(slice)).toBe('1 unread plate: A0');
	});

	it('says nothing when every plate resolved', () => {
		expect(unknownRoomsBadge(templeSliceDefault())).toBeNull();
	});

	it('carries the reader’s own message into the marker-fallback notice', () => {
		const notice = markerFallbackNotice(layout({ markerError: 'the diamond rect fell outside' }));
		expect(notice).toContain('the diamond rect fell outside');
	});

	it('says nothing about markers when the diamond read settled the doors', () => {
		expect(markerFallbackNotice(layout())).toBeNull();
		expect(markerFallbackNotice(null)).toBeNull();
	});

	it('labels the two modes and passes an unknown one through', () => {
		expect(modeLabel('chase')).toBe('Chase');
		expect(modeLabel('scarab')).toBe('Scarab');
		// Never swallow a mode Rust added: showing the raw string beats showing
		// nothing where the mode belongs.
		expect(modeLabel('ritual')).toBe('ritual');
		expect(modeLabel(null)).toBeNull();
	});
});


describe('overlayShowsDoors', () => {
	/** A slice mid-incursion: a room read, a move ranked. */
	function showing(over: Partial<ReturnType<typeof templeSliceDefault>> = {}) {
		return {
			...templeSliceDefault(),
			status: 'panel_not_visible' as const,
			layout: layout({ current: 'C1', diamond: diamond() }),
			advice: advice({ recommendations: [ranked()] }),
			...over
		};
	}

	it('shows the widget while there is a move and a room to draw it on', () => {
		expect(overlayShowsDoors(showing())).toBe(true);
	});

	it('survives every status the loop can publish while the advice stands', () => {
		// POE-248's rule, and the regression it closes: the gate is no longer a
		// status list. `waiting` is the capture having stood down, which is
		// exactly what the tail of an incursion looks like — the owner watched
		// the widget vanish there while he was still in the room.
		for (const status of ALL_STATUSES) {
			expect(overlayShowsDoors(showing({ status })), status).toBe(true);
		}
	});

	it('hides the widget once the advice is cleared', () => {
		// The only thing that takes it down. Rust clears the advice on a zone
		// change, on the next Alva line after the read, and when the module is
		// switched off (`trigger::advice_end`, `slice::force_off`).
		expect(overlayShowsDoors(showing({ advice: null }))).toBe(false);
	});

	it('hides the widget when the read settled no room to draw', () => {
		// Between rooms: the layout is published without a diamond, and there is
		// no shape to hang a door on.
		expect(overlayShowsDoors(showing({ layout: layout({ diamond: null }) }))).toBe(false);
		expect(overlayShowsDoors(showing({ layout: null }))).toBe(false);
	});
});

describe('doorWidget', () => {
	/** The room the previous read of this incursion settled. */
	const room = diamond();

	/** A slice with a move ranked and a room read — what the previous read of
	 *  this incursion left on the slice. */
	function inRoom(over: Partial<ReturnType<typeof templeSliceDefault>> = {}) {
		return {
			...templeSliceDefault(),
			status: 'read' as const,
			layout: layout({ current: 'C1', diamond: room }),
			advice: advice({ recommendations: [ranked()] }),
			...over
		};
	}

	it('draws the reading line alone on the first read of an incursion', () => {
		// Rust publishes `reading` from the anchoring tick, before any advice or
		// layout exists. Without this the widget drew nothing for the seconds
		// the read takes, which looked the same as a broken read (POE-276).
		const firstRead = { ...templeSliceDefault(), status: 'reading' as const };
		expect(doorWidget(firstRead)).toEqual({ diamond: null, reading: 'reading…' });
	});

	it('draws no room left over from the last incursion on a first read', () => {
		// The advice is cleared when an incursion ends, but the Temple page keeps
		// the last LAYOUT standing — so its diamond is still on the slice when
		// the next incursion's first read starts. It is the wrong room.
		const firstRead = inRoom({ status: 'reading', advice: null });
		expect(doorWidget(firstRead)).toEqual({ diamond: null, reading: 'reading…' });
	});

	it('keeps the previous room under the reading line on a re-read', () => {
		// The next room's read: the slice still carries the last advice and
		// layout while `reading` is published, and the widget is the only
		// surface left in the room, so it must not blank for the read.
		const reRead = inRoom({ status: 'reading' });
		expect(doorWidget(reRead)).toEqual({ diamond: room, reading: 'reading…' });
	});

	it('draws the room without the reading line once the read lands', () => {
		expect(doorWidget(inRoom({ status: 'read' }))).toEqual({ diamond: room, reading: null });
	});

	it('draws nothing while idle, before anything was read', () => {
		expect(doorWidget(templeSliceDefault())).toBeNull();
	});

	it('draws nothing while waiting for the panel — the notice has that moment', () => {
		// Alva's start line with the sheet not up yet: `TempleWaitingNotice`
		// speaks, and the door widget hands off from it only at `reading`.
		const waiting = { ...templeSliceDefault(), status: 'idle' as const, waitingForPanel: true };
		expect(doorWidget(waiting)).toBeNull();
	});

	it('draws nothing for a read that landed between rooms', () => {
		// `no_current_room` publishes a layout with no diamond and no advice.
		const between = inRoom({
			status: 'no_current_room',
			advice: null,
			layout: layout({ diamond: null })
		});
		expect(doorWidget(between)).toBeNull();
	});

	it('draws no reading line on any status but reading while a move stands', () => {
		// The constraint the line was added under: outside `reading` the widget
		// is what it was before POE-276 — the room, on every status (POE-248),
		// and no line. `error` included: a failed re-read replaces `reading`,
		// and the move from the read before it still stands.
		for (const status of ALL_STATUSES.filter((s) => s !== 'reading')) {
			expect(doorWidget(inRoom({ status })), status).toEqual({ diamond: room, reading: null });
		}
	});

	it('draws nothing on any status but reading once the advice is cleared', () => {
		// A zone change, the next Alva line, the module off, a failed first read:
		// no move stands, and no read is in progress to say anything about.
		for (const status of ALL_STATUSES.filter((s) => s !== 'reading')) {
			expect(doorWidget(inRoom({ status, advice: null })), status).toBeNull();
		}
	});
});

describe('chosenOffer', () => {
	const panel = (offers: OfferView[]) => ({
		room: 'Chamber of Iron',
		roomRect: null,
		offers,
		incursionsRemaining: 6
	});

	it('finds the block the top recommendation names, not the first one', () => {
		const slice = {
			...templeSliceDefault(),
			advice: advice({ recommendations: [ranked({ architectIndex: 1 })] }),
			panel: panel([offer(), offer({ index: 1, architectName: 'Atmohua' })])
		};
		expect(chosenOffer(slice)?.architectName).toBe('Atmohua');
	});

	it('is null when the ranking named no architect', () => {
		const slice = {
			...templeSliceDefault(),
			advice: advice({ recommendations: [ranked({ architectIndex: null })] }),
			panel: panel([offer()])
		};
		expect(chosenOffer(slice)).toBeNull();
	});

	it('is null when the named block is not in the panel view', () => {
		// An index past the end is a read the two halves disagree about. Null is
		// "nothing to point at", which is a state the callout already draws;
		// an undefined offer reaching a surface is not.
		const slice = {
			...templeSliceDefault(),
			advice: advice({ recommendations: [ranked({ architectIndex: 3 })] }),
			panel: panel([offer()])
		};
		expect(chosenOffer(slice)).toBeNull();
	});
});

describe('offerBoxes', () => {
	const panel = (offers: OfferView[]) => ({
		room: 'Chamber of Iron',
		roomRect: null,
		offers,
		incursionsRemaining: 6
	});
	/** A slice with a read panel and a ranking over it. */
	const slice = (offers: OfferView[], over: Partial<AdviceView> = {}) => ({
		...templeSliceDefault(),
		advice: advice(over),
		panel: panel(offers)
	});

	it('draws one box per architect block, in the panel\'s own order', () => {
		// PANEL order and not "upgrade first": `offers` is reading order
		// top-to-bottom and a real board can print two `change` blocks
		// (`panel.rs`'s own fixture does), so box i mirrors offers[i] and each
		// box says its own kind.
		const boxes = offerBoxes(
			slice([
				offer({ index: 0, architectName: 'Guatelitzi', kind: 'change' }),
				offer({ index: 1, architectName: 'Atmohua', kind: 'change' })
			])
		);
		expect(boxes.map((box) => box.headline)).toEqual(['Torment Cells', 'Torment Cells']);
		expect(boxes.map((box) => box.builds)).toEqual(['change · tier 2', 'change · tier 2']);
	});

	it('keeps architect names out of the box headers', () => {
		const box = offerBoxes(
			slice([offer({ architectName: 'Puhuarte', displayName: 'Torment Cells' })])
		)[0];

		expect(box.headline).not.toContain('Puhuarte');
		expect(box.builds).not.toContain('Puhuarte');
		expect(box.headline).toBe('Torment Cells');
	});

	it('names the room each kill BUILDS, not the one its block printed', () => {
		// POE-169 again, on the surface that has the room: Contested Development
		// prints one line and builds `currentTier + 1` of it.
		const boxes = offerBoxes(
			slice([offer({ printedTarget: "Sadist's Den", displayName: 'Torment Cells', builtTier: 2 })])
		);
		expect(boxes[0].headline).toBe('Torment Cells');
		expect(boxes[0].builds).toBe('upgrade · tier 2');
	});

	it('builds lines carry the kind and tier', () => {
		const boxes = offerBoxes(
			slice([
				offer({ index: 0, kind: 'change', builtTier: 1 }),
				offer({ index: 1, kind: 'upgrade', builtTier: 3 })
			])
		);

		expect(boxes.map((box) => box.builds)).toEqual(['change · tier 1', 'upgrade · tier 3']);
	});

	it('uses the printed target and refusal sentence for an unresolved offer', () => {
		const box = offerBoxes(
			slice([offer({ printedTarget: "Sadist's Den", displayName: null, builtTier: null })])
		)[0];

		expect(box.headline).toBe("Sadist's Den");
		expect(box.builds).toBe('does not resolve to a known room');
	});

	it('leaves the kind alone when the resolved tier is absent', () => {
		const box = offerBoxes(slice([offer({ kind: 'change', builtTier: null })]))[0];

		expect(box.builds).toBe('change');
	});

	it('carries the line ladders and built tier on the offer box', () => {
		const boxes = offerBoxes(
			slice([offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: [2, 4, 6],
					rarityPct: [4, 8, 12]
				},
				builtTier: 2
			})])
		);

		expect(boxes[0].ladder).toEqual({
			quant: [2, 4, 6],
			rarity: [4, 8, 12],
			quantText: ['2', '4', '6'],
			rarityText: ['4', '8', '12'],
			tier: 2
		});
	});

	it('leaves the ladder null when line facts are absent', () => {
		const boxes = offerBoxes(slice([offer({ line: null })]));

		expect(boxes[0].ladder).toBeNull();
	});

	it('leaves the ladder null when both line ladders are absent', () => {
		const boxes = offerBoxes(
			slice([offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				}
			})])
		);

		expect(boxes[0].ladder).toBeNull();
	});

	it('keeps the ladder when exactly one line ladder exists', () => {
		const boxes = offerBoxes(
			slice([offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: [2, 4, 6],
					rarityPct: null
				}
			})])
		);

		expect(boxes[0].ladder).toEqual({
			quant: [2, 4, 6],
			rarity: null,
			quantText: ['2', '4', '6'],
			rarityText: null,
			tier: 2
		});
	});

	it('keeps one decimal place in ladder text', () => {
		const boxes = offerBoxes(
			slice([offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: [2, 4, 22.5],
					rarityPct: null
				}
			})])
		);

		expect(boxes[0].ladder?.quant).toEqual([2, 4, 22.5]);
		expect(boxes[0].ladder?.quantText).toEqual(['2', '4', '22.5']);
	});

	it('leaves the ladder null when the built tier is absent', () => {
		const boxes = offerBoxes(
			slice([offer({
				builtTier: null,
				line: {
					kind: 'chest',
					content: null,
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: [2, 4, 6],
					rarityPct: null
				}
			})])
		);

		expect(boxes[0].ladder).toBeNull();
	});

	it('names what a content line gives instead of a chest unique', () => {
		// The 19 lines with no tier-3 chest are worth their USE, and WI-6 puts
		// that use in the item row where the `A + B → C` recipe sits on a chest
		// box. The prose is the wire's — `view.ts` words none of it.
		const boxes = offerBoxes(
			slice([offer({
				line: {
					kind: 'content',
					content: 'Queen Atziri',
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				}
			})])
		);

		expect(boxes[0].content).toBe('Queen Atziri');
	});

	it('decides the content cell on the line KIND, not on the content field', () => {
		// `kind` is the discriminator because it is the one Rust DERIVES — from
		// `unique.is_some()` — while `content` is prose that a payload could
		// carry for any reason. A box reading the string instead would draw a
		// content cell over the chest row the same line's unique still fills.
		const boxes = offerBoxes(
			slice([offer({
				line: {
					kind: 'chest',
					content: 'Double-corrupt an item',
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				}
			})])
		);

		expect(boxes[0].content).toBeNull();
	});

	it('invents no wording for a content line the wire left blank', () => {
		// The strings are UNCONFIRMED draft prose, so a line can reach the box
		// with none. A placeholder here would be the box making a claim about
		// the room that nobody made.
		const boxes = offerBoxes(
			slice([offer({
				line: {
					kind: 'content',
					content: null,
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				}
			})])
		);

		expect(boxes[0].content).toBeNull();
	});

	it('marks the advisor\'s block as the pick, and only that one', () => {
		// The cyan frame is the whole pointer (owner: no arrows anywhere), so
		// exactly one box may carry it — a second would point at two blocks and
		// none would point at one.
		const boxes = offerBoxes(
			slice([offer({ index: 0 }), offer({ index: 1 })], {
				recommendations: [ranked({ architectIndex: 1 })]
			})
		);
		expect(boxes.map((box) => box.pick)).toEqual([false, true]);
	});

	it('marks nothing when the ranking named no architect', () => {
		// `kill either` points at neither block, and framing one would invent a
		// preference the advisor did not state.
		const boxes = offerBoxes(
			slice([offer({ index: 0 }), offer({ index: 1 })], {
				recommendations: [ranked({ headline: 'kill either', architectIndex: null })]
			})
		);
		expect(boxes.map((box) => box.pick)).toEqual([false, false]);
	});

	it('marks a forced kill on the pick alone', () => {
		// The note says the kill on the frame was the only block read, which is
		// a statement about the CHOSEN box. On the other box it would be saying
		// it about the wrong one.
		const boxes = offerBoxes(
			slice([offer({ index: 0 }), offer({ index: 1 })], {
				forcedKill: true,
				recommendations: [ranked({ architectIndex: 1 })]
			})
		);
		expect(boxes.map((box) => box.forced)).toEqual([null, 'only architect read']);
	});

	it('leaves the note off a kill the advisor chose between two', () => {
		const boxes = offerBoxes(slice([offer({ index: 0 })], { forcedKill: false }));
		expect(boxes[0].forced).toBeNull();
	});

	it('draws nothing until there is a ranking to draw', () => {
		// The boxes carry the PICK, so a panel read with no advice behind it —
		// the gap between a sighting and a completed read in a new cycle — has
		// nothing to say. Two unmarked boxes would read as "the advisor has no
		// preference", which is a different claim.
		expect(offerBoxes({ ...templeSliceDefault(), panel: panel([offer()]) })).toEqual([]);
	});

	it('draws nothing with no panel to draw about', () => {
		expect(offerBoxes({ ...templeSliceDefault(), advice: advice() })).toEqual([]);
		expect(offerBoxes(templeSliceDefault())).toEqual([]);
	});

	it('tells every box where the numbers on it came from', () => {
		// POE-258: the box exists to make one comparison readable at arm's
		// length, and "these came from the grade ladder, not the market" is
		// part of that comparison. On EVERY box, and the same line on both —
		// Rust values a read once, so two different answers here would be two
		// answers to a question with one.
		const priced = {
			...slice([offer({ index: 0 }), offer({ index: 1 })]),
			market: { asOf: NOW - 12 * MINUTE, stale: false, staleAfterMs: STALE_AFTER, unavailable: false }
		};

		const boxes = offerBoxes(priced, NOW);

		expect(boxes.map((box) => box.market)).toEqual([
			'prices 12 min old',
			'prices 12 min old'
		]);
		expect(boxes.map((box) => box.ageLine)).toEqual([null, null]);
	});

	it('says on the box when the ranking is running on base values', () => {
		// The failure this catches is the one the player cannot see: a board
		// ranked off the cold grade ladder prints the same kind of chaos number
		// as a board ranked off the feed. Fails if the box stops carrying the
		// note, or carries the priced wording for an unavailable market.
		const cold = { ...slice([offer({ index: 0 })]), market: NO_MARKET };

		const boxes = offerBoxes(cold, NOW);

		expect(boxes[0].market).toBe('prices unavailable — base values');
		expect(boxes[0].ageLine).toBe('prices unavailable — base values');
	});

	it("prices its line off the READ's market and never off the latest poll", () => {
		// The M1 defect, in the shape that shipped: the board on screen was
		// priced twelve minutes ago against a live read, and a DEBUG/PROD switch
		// has since dropped the market — so the POLL says unavailable while the
		// boxes still carry real chaos figures from the read. The box must
		// describe its own numbers. Fails the moment `offerBoxes` reads
		// `pollMarket`.
		const switched = {
			...slice([offer({ index: 0 }), offer({ index: 1 })]),
			market: LIVE_MARKET,
			pollMarket: NO_MARKET
		};

		const boxes = offerBoxes(switched, NOW);

		expect(boxes.map((box) => box.market)).toEqual([
			'prices 12 min old',
			'prices 12 min old'
		]);
		// And the other half of the same fact: the page's row is the poll's.
		expect(marketNote(switched.pollMarket, NOW)).toBe('prices unavailable — base values');
	});

	it('ages a standing board into stale on the clock, with nothing republished', () => {
		// A read published while the market was live, still on screen three
		// hours later. Rust's `stale` was FALSE when this view was written and
		// nothing has rewritten it — `TempleSlice.market` belongs to the read —
		// so a box that trusted the wire flag would still print `prices 3 h old`
		// beside numbers nobody should trade on. Fails if `marketStale` stops
		// reading `staleAfterMs` and the clock.
		//
		// And NO `— base values` suffix: this box's chaos figures came off a
		// live market and have merely gone old. The suffix is a claim about the
		// numbers, and here it would be false.
		const aged = {
			...slice([offer({ index: 0 })]),
			market: { asOf: NOW - 3 * HOUR, stale: false, staleAfterMs: STALE_AFTER, unavailable: false }
		};

		const boxes = offerBoxes(aged, NOW);

		expect(boxes[0].market).toBe('prices stale (3 h)');
		expect(boxes[0].stale).toBe(true);
		expect(boxes[0].ageLine).toBe('prices stale (3 h)');
	});
});


// ------------------------------------------- the value fixtures (POE-260) --

/** A live read, twelve minutes old — the age the design's artboards print. */
const LIVE_MARKET: MarketView = {
	asOf: NOW - 12 * MINUTE,
	stale: false,
	staleAfterMs: STALE_AFTER,
	unavailable: false
};

/**
 * One room-tier's value, as Rust publishes it.
 *
 * The defaults are a whole, market-priced tier-3 row with no terms; every test
 * below states the terms it is about. Nothing here computes anything — the
 * point of these tests is that `view.ts` computes nothing either.
 */
function value(over: Partial<RoomValueView> = {}): RoomValueView {
	return {
		total: 96,
		priced: 'market',
		guessed: false,
		league: 'Allflame',
		asOf: NOW - 12 * MINUTE,
		scaledFromTier3: null,
		drivers: [],
		...over
	};
}

/** One term of that sum. The named builders below are the real shapes: a term
 *  with a count and a unit price, one with neither, and the two bonuses. */
function term(over: Partial<DriverView> & { kind: string; name: string }): DriverView {
	return {
		count: null,
		unitPrice: null,
		chaos: null,
		guessed: false,
		lowConfidence: false,
		windowPriced: false,
		...over
	};
}

/** Crucible of Flame's own line on the committed capture, at the numbers the
 *  design's priced artboard prints. */
const saleTerm = (over: Partial<DriverView> = {}) =>
	term({ kind: 'sale', name: 'Crucible of Flame', unitPrice: 196, chaos: 186, ...over });
const uniqueTerm = (over: Partial<DriverView> = {}) =>
	term({
		kind: 'unique_drop',
		name: 'Story of the Vaal',
		count: 0.25,
		unitPrice: 68,
		chaos: 17,
		guessed: true,
		...over
	});
const vialTerm = (over: Partial<DriverView> = {}) =>
	term({
		kind: 'vial_drop',
		name: 'Vial of Fate',
		count: 0.1,
		unitPrice: 41,
		chaos: 4.1,
		guessed: true,
		...over
	});
const modTerm = (over: Partial<DriverView> = {}) =>
	term({
		kind: 'mod_item',
		name: 'temple gloves',
		count: 2,
		unitPrice: 30,
		chaos: 60,
		guessed: true,
		...over
	});
const quantityTerm = (over: Partial<DriverView> = {}) =>
	term({
		kind: 'quantity_bonus',
		name: 'increased Quantity of Items found in this Area',
		count: 6,
		unitPrice: 0.5,
		chaos: 3,
		...over
	});
const rarityTerm = (over: Partial<DriverView> = {}) =>
	term({
		kind: 'rarity_bonus',
		name: 'increased Rarity of Items found in this Area',
		count: 12,
		unitPrice: 0.25,
		chaos: 3,
		...over
	});
/** The driver that makes a tier-1 row a fraction of its LINE (epic lock L2). */
const fractionTerm = (over: Partial<DriverView> = {}) =>
	term({
		kind: 'tier_fraction',
		name: 'Crucible of Flame',
		count: 0.8,
		unitPrice: 846,
		chaos: 676.8,
		...over
	});

describe('offerBoxes — what the number is made of (POE-260)', () => {
	const panel = (offers: OfferView[]) => ({
		room: 'Chamber of Iron',
		roomRect: null,
		offers,
		incursionsRemaining: 6
	});
	const slice = (offers: OfferView[], market: MarketView = LIVE_MARKET) => ({
		...templeSliceDefault(),
		advice: advice(),
		panel: panel(offers),
		market
	});
	const only = (offer: OfferView, market?: MarketView) =>
		offerBoxes(slice([offer], market), NOW)[0];

	it('projects the temple mod identity and priced marks', () => {
		const box = only(
			offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: 'Puhuarte',
					modHint: 'temple gloves',
					modSlots: ['gloves'],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				value: value({ drivers: [modTerm()] })
			})
		);

		expect(box.mod).toEqual({
			name: 'Puhuarte',
			hint: 'temple gloves',
			slots: ['gloves'],
			price: '30c',
			perRun: '×2',
			// POE-277 v5: the wire's verdict on the mod family, and `neutral`
			// here because this fixture's line states none — which is the
			// guard's answer for "nobody said", not a middling grade.
			worth: 'neutral',
			marks: ['G']
		});
	});

	it('carries the wire\'s good verdict onto the mod block', () => {
		// Vertolka's 2026-09-10 call on Puhuarte, which is what turns the mod
		// NAME green on the box (POE-277 v5). Rust decides it from the mod
		// family; nothing on this side re-derives it from the price, and this
		// is the assertion that the wire's word reaches the box at all.
		const box = only(
			offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: 'Puhuarte',
					modHint: 'temple gloves',
					modSlots: ['gloves'],
					modWorth: 'good',
					quantityPct: null,
					rarityPct: null
				},
				value: value({ drivers: [modTerm()] })
			})
		);

		expect(box.mod?.worth).toBe('good');
	});

	it('carries the wire\'s junk verdict onto the mod block', () => {
		// The other end of the same message — Matatl, red — and its own test
		// rather than a second assertion above: a guard narrowed to the good
		// arm alone would pass that one and drop this verdict to `neutral`,
		// which reads on screen as a family nobody has graded.
		const box = only(
			offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: 'Matatl',
					modHint: 'temple boots',
					modSlots: ['boots'],
					modWorth: 'junk',
					quantityPct: null,
					rarityPct: null
				},
				value: value({ drivers: [modTerm()] })
			})
		);

		expect(box.mod?.worth).toBe('junk');
	});

	it('reads a verdict this file has not been taught as no verdict at all', () => {
		// Same rule as `offerState`'s: the wire spelling is a plain string and
		// `serde(default)` lets an older payload omit it, so a word this file
		// does not know must land on the one answer that claims nothing.
		// Without the guard the raw string reaches the markup's class list and
		// colours the name off a verdict nobody chose.
		const box = only(
			offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: 'Puhuarte',
					modHint: 'temple gloves',
					modSlots: ['gloves'],
					modWorth: 'chase' as unknown as ModWorth,
					quantityPct: null,
					rarityPct: null
				},
				value: value({ drivers: [modTerm()] })
			})
		);

		expect(box.mod?.worth).toBe('neutral');
	});

	it('leaves the mod block absent when the line has no mod', () => {
		const box = only(
			offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				value: value({ drivers: [uniqueTerm()] })
			})
		);

		expect(box.mod).toBeNull();
	});

	it('falls back to the driver name when the line is absent', () => {
		const box = only(offer({ line: null, value: value({ drivers: [modTerm()] }) }));

		expect(box.mod?.name).toBe('temple gloves');
		expect(box.mod?.slots).toEqual([]);
	});

	it('prints no price for an unpriced mod term', () => {
		const box = only(
			offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: 'Puhuarte',
					modHint: 'temple gloves',
					modSlots: ['gloves'],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				value: value({
					priced: 'partial',
					drivers: [modTerm({ unitPrice: null, chaos: null })]
				})
			})
		);

		expect(box.mod?.price).toBe('no price');
	});

	it('keeps a named mod block on a fallback read without a mod term', () => {
		const box = only(
			offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: 'Puhuarte',
					modHint: 'temple gloves',
					modSlots: ['gloves'],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				value: value({ priced: 'fallback', drivers: [] })
			})
		);

		expect(box.mod).toEqual({
			name: 'Puhuarte',
			hint: 'temple gloves',
			slots: ['gloves'],
			price: '—',
			perRun: null,
			worth: 'neutral',
			marks: []
		});
	});

	it('gives the mod cell a price its drawer can tell apart from having none', () => {
		const noPrice = only(
			offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: 'Puhuarte',
					modHint: 'temple gloves',
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				value: value({ priced: 'partial', drivers: [modTerm({ unitPrice: null, chaos: null })] })
			})
		);
		const priced = only(
			offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: 'Puhuarte',
					modHint: 'temple gloves',
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				value: value({ drivers: [modTerm()] })
			})
		);
		const fallback = only(
			offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: 'Puhuarte',
					modHint: 'temple gloves',
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				value: value({ priced: 'fallback', drivers: [] })
			})
		);

		expect(noPrice.mod?.price).toBe('no price');
		expect(priced.mod?.price).toBe('30c');
		expect(fallback.mod?.price).toBe('—');
	});

	it('draws the mod price cell only when a real number sits in it', () => {
		// The two stand-ins above are the box's answer to "what does this rare
		// go for", and neither is a number: `no price` means the sheet named
		// none and `—` means the ladder priced the room. Printing either in the
		// parenthesis beside the architect's name would read as a quote.
		//
		// Read off the source because this app has no DOM harness for a
		// `.svelte` file — the same seam `overlay-geometry.test.ts` uses.
		const modBlock = offerBoxesSource.slice(offerBoxesSource.indexOf('{#if box.mod}'));

		expect(modBlock).toContain("{#if box.mod.price !== 'no price' && box.mod.price !== '—'}");
		expect(modBlock.match(/\{box\.mod\.price\}/g)).toHaveLength(1);
	});

	it('excludes the mod row from visible driver counts', () => {
		const box = only(
			offer({ value: value({ drivers: [saleTerm(), uniqueTerm(), vialTerm(), modTerm()] }) })
		);

		expect(box.drivers.map((driver) => driver.kind)).toEqual(['sale', 'unique', 'vial']);
		expect(box.driverCount).toBe(3);
		expect(box.fold).toBeNull();
	});

	it('counts an unpriced mod in the completeness chip', () => {
		const box = only(
			offer({
				line: {
					kind: 'chest',
					content: null,
					modArchitect: 'Puhuarte',
					modHint: 'temple gloves',
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				value: value({
					priced: 'partial',
					drivers: [uniqueTerm(), modTerm({ unitPrice: null, chaos: null })]
				})
			})
		);

		expect(box.chip).toBe('floor · 1 unpriced');
	});

	it('rolls a mod estimate up to the box while retaining its row data', () => {
		const box = only(offer({ value: value({ guessed: true, drivers: [modTerm()] }) }));

		expect(box.mod?.marks).toEqual(['G']);
		expect(box.marks).toEqual(['G']);
	});

	it('does not call a mod-only line an empty unique and vial drop', () => {
		const box = only(offer({ value: value({ drivers: [modTerm()] }) }));

		expect(box.note).toBeNull();
	});

	it('lists the sale first and then what the room drops, in the wire order', () => {
		// `valuation.rs` pushes sale, unique, vial, mod, and the box draws that
		// order: the sale is money the room pays out now and the drops are what
		// falls out of it, which is the order a player reads them in. A box
		// re-sorting them by price would put a 428 c vial above the 846 c sale.
		const box = only(offer({ value: value({ drivers: [saleTerm(), uniqueTerm(), vialTerm()] }) }));

		expect(box.drivers.map((driver) => [driver.kind, driver.name])).toEqual([
			['sale', 'sale price above floor'],
			['unique', 'Story of the Vaal'],
			['vial', 'Vial of Fate']
		]);
		expect(box.dropPair.map((driver) => driver?.kind ?? null)).toEqual(['unique', 'vial']);
	});

	it('prints the ITEM price and its count on a drop row, not the term it contributed', () => {
		// 0.25 x 68 c is 17 c, and 17 is not a number anybody can look up. The
		// row shows what one Story of the Vaal goes for and how many a run is
		// worth, which is the pair a player checks against poe.ninja. A row
		// printing the contribution would read as a price nobody trades at.
		const box = only(offer({ value: value({ drivers: [uniqueTerm()] }) }));

		expect(box.drivers[0].price).toBe('68c');
		expect(box.drivers[0].perRun).toBe('×0.25');
		expect(box.drivers[0].iconName).toBe('Story of the Vaal');
		expect(box.dropPair.map((driver) => driver?.kind ?? null)).toEqual(['unique', null]);
	});

	it('puts a vial-only drop in the vial cell and leaves the unique cell empty', () => {
		const box = only(offer({ value: value({ drivers: [vialTerm()] }) }));

		expect(box.drivers.map((driver) => driver.kind)).toEqual(['vial']);
		expect(box.dropPair.map((driver) => driver?.kind ?? null)).toEqual([null, 'vial']);
	});

	it('never lifts a sale into a drop cell', () => {
		// The full form draws the sale on its own full-width row above the
		// two-column drop cell, because a sale is chaos the room pays out and a
		// drop is an item that falls out of it. A sale in a drop cell would sit
		// under a unique's 39 px icon column with no icon to put there.
		const box = only(offer({ value: value({ drivers: [saleTerm()] }) }));

		expect(box.drivers.map((driver) => driver.kind)).toEqual(['sale']);
		expect(box.dropPair).toEqual([null, null]);
	});

	// The count on the row beside it is no longer a number anybody typed:
	// POE-262 derives a vial rate per line from poedb's chance stat, so
	// Glittering Halls' is 0.1 x 2815/1689 and interpolating it raw puts
	// `×0.16666666666666666` in a 24 px overlay cell. Fails if `formatCount`
	// stops being applied to the count.
	it('rounds a DERIVED count rather than printing the float the wire carried', () => {
		const box = only(
			offer({
				value: value({
					drivers: [
						vialTerm({
							name: 'Vial of Transcendence',
							count: 0.1 * (2815 / 1689),
							unitPrice: 428,
							chaos: 71.33
						})
					]
				})
			})
		);

		expect(box.drivers[0].perRun).toBe('×0.17');
	});

	it('captions a drop the player will rarely see with its chance per run', () => {
		// Defense Research Lab's own rate on the committed capture, 0.1 x
		// 804/1689. `×0.05` is a count a player reads as "some fraction"; the
		// same number as 4.8% of runs is the sentence they can act on, which is
		// the whole reason the caption exists.
		const box = only(
			offer({
				value: value({
					drivers: [
						vialTerm({
							name: 'Vial of Dominance',
							count: 0.1 * (804 / 1689),
							unitPrice: 41,
							chaos: 1.95
						})
					]
				})
			})
		);

		expect(box.drivers[0].caption).toBe('4.8% / run');
	});

	it('keeps the rarest rate in the table off a zero the player would read as never', () => {
		// Locus of Corruption and Throne of Atziri, 0.1 x 20/1689 — the two
		// smallest rates POE-262 derives. One decimal place is what stands
		// between `0.1% / run` and a caption saying the drop cannot happen.
		const box = only(
			offer({
				value: value({
					drivers: [
						vialTerm({
							name: 'Vial of Sacrifice',
							count: 0.1 * (20 / 1689),
							unitPrice: 428,
							chaos: 0.51
						})
					]
				})
			})
		);

		expect(box.drivers[0].caption).toBe('0.1% / run');
	});

	it('captions a rare unique on the same rule as a rare vial', () => {
		// The threshold is stated over BOTH drop kinds. Nothing in the table
		// trips the unique half today — Vertolka's 0.25 is every chest line's
		// rate — so this is the assertion that the rule does not quietly become
		// a vial-only one the day a unique rate is derived like a vial's is.
		const box = only(
			offer({ value: value({ drivers: [uniqueTerm({ count: 0.1 * (804 / 1689) })] }) })
		);

		expect(box.drivers[0].caption).toBe('4.8% / run');
	});

	it('captions nothing at exactly the five-percent line', () => {
		// The threshold is `< 0.05`, not `<= 0.05`: a drop the player sees on
		// one run in twenty is not one the box has to explain, and a caption
		// there costs the row 12 px it did not need to spend.
		const box = only(offer({ value: value({ drivers: [vialTerm({ count: 0.05 })] }) }));

		expect(box.drivers[0].caption).toBeNull();
	});

	it('leaves an ordinary count uncaptioned', () => {
		// Vertolka's quarter — the rate on all six chest lines. The owner took
		// `×0.25` off the ordinary cells on purpose, and a caption spelling the
		// same count as `25.0% / run` would put it straight back.
		const box = only(offer({ value: value({ drivers: [uniqueTerm()] }) }));

		expect(box.drivers[0].perRun).toBe('×0.25');
		expect(box.drivers[0].caption).toBeNull();
	});

	it('captions nothing where the wire carried no count at all', () => {
		// A row with no count has no chance to state. Reading the missing count
		// as a zero would caption it `0.0% / run`, which is a claim about the
		// room rather than an admission that nothing counted it.
		const box = only(offer({ value: value({ drivers: [uniqueTerm({ count: null })] }) }));

		expect(box.drivers[0].perRun).toBeNull();
		expect(box.drivers[0].caption).toBeNull();
	});

	it('never captions the sale row, whatever count the wire hung on it', () => {
		// A sale is chaos the room pays out when it is taken, not an item that
		// falls out of it some fraction of the time, so there is no "per run"
		// for it to be. This fixture gives it a count under the threshold so the
		// row is excluded by its KIND and not by having nothing to divide.
		const box = only(offer({ value: value({ drivers: [saleTerm({ count: 0.01 })] }) }));

		expect(box.drivers[0].kind).toBe('sale');
		expect(box.drivers[0].caption).toBeNull();
	});

	it('prints the sale row as the delta above the floor, in the gain form', () => {
		// The one row whose number IS its contribution: what the room pays above
		// the floor. The `+` is the difference from a drop price, and the row
		// carries no count because a sale is not a per-run expectation.
		const box = only(
			offer({ value: value({ drivers: [saleTerm({ chaos: 186, unitPrice: 196 })] }) })
		);

		expect(box.drivers[0].price).toBe('+186c');
		expect(box.drivers[0].perRun).toBeNull();
		expect(box.drivers[0].iconName).toBeNull();
	});

	it('drops a sale worth exactly zero rather than printing +0c on every box', () => {
		// 84 of the capture's 86 room-tiers sit at the floor. A `+0c` row on
		// nearly every box would push a row saying what the room DROPS into the
		// fold, which is the row the player is reading for.
		const box = only(
			offer({ value: value({ drivers: [saleTerm({ chaos: 0 }), uniqueTerm()] }) })
		);

		expect(box.drivers.map((driver) => driver.kind)).toEqual(['unique']);
	});

	it('keeps an UNPRICED item on screen and says so, rather than inventing a number', () => {
		// The acceptance criterion in one line: icon shown, `no price` in the
		// price cell, never a fabricated 0. A production change that read a
		// missing price as zero prints `0c` here and fails.
		const box = only(
			offer({ value: value({ priced: 'partial', drivers: [uniqueTerm(), vialTerm({ unitPrice: null, chaos: null })] }) })
		);

		expect(box.drivers[1].price).toBe('no price');
		expect(box.drivers[1].priced).toBe(false);
		expect(box.drivers[1].iconName).toBe('Vial of Fate');
	});

	it('says an em dash instead where there is no market to have an answer', () => {
		// Two different absences, and the box has to tell them apart: `no price`
		// means this item was looked up and came back empty while other terms
		// priced, and `—` means the whole read is on the grade ladder. A player
		// who cannot tell them apart cannot tell a dead feed from a dead item.
		const box = only(
			offer({ value: value({ priced: 'fallback', drivers: [uniqueTerm({ unitPrice: null, chaos: null })] }) })
		);

		expect(box.drivers[0].price).toBe('—');
	});

	it('counts the unpriced rows in the partial chip', () => {
		const box = only(
			offer({
				value: value({
					priced: 'partial',
					drivers: [uniqueTerm({ unitPrice: null, chaos: null }), vialTerm({ unitPrice: null, chaos: null })]
				})
			})
		);

		expect(box.chip).toBe('floor · 2 unpriced');
	});

	it('carries no chip at all when every term priced', () => {
		expect(only(offer({ value: value({ drivers: [uniqueTerm()] }) })).chip).toBeNull();
	});

	it('counts an unpriced term the FOLD hid, because the chip is about the number', () => {
		// The chip says how complete the sum is, and the sum is every term —
		// not the three the box has room to draw. A partial board whose only
		// unpriced item happens to be the fourth would otherwise print a bare
		// `floor`, which is the wording for a missing COUNT and says nothing
		// about the price that is actually missing.
		const box = only(
			offer({
				value: value({
					priced: 'partial',
					drivers: [
						saleTerm({ chaos: 186 }),
						uniqueTerm(),
						vialTerm(),
						modTerm({ unitPrice: null, chaos: null })
					]
				})
			})
		);

		expect(box.drivers).toHaveLength(3);
		expect(box.chip).toBe('floor · 1 unpriced');
	});

	it('words a state this file has not been taught as a partial sum in EVERY field', () => {
		// `offerState` answers `partial` for a wire string it does not know,
		// because it is the one state that claims nothing. That only holds if
		// the rest of the wording follows its answer instead of re-reading the
		// raw string: the chip's own branch used to, so an unknown state gave a
		// box that called itself partial and then drew no `floor` chip at all.
		const box = only(
			offer({
				value: value({
					priced: 'a_state_a_later_rust_grew',
					drivers: [uniqueTerm({ unitPrice: null, chaos: null })]
				})
			})
		);

		expect(box.state).toBe('partial');
		expect(box.chip).toBe('floor · 1 unpriced');
		expect(box.drivers[0].price).toBe('no price');
	});

	it('shows the LETTER with F and G marks where the ladder priced the room', () => {
		// Epic lock L4: no market, no chaos number. The grade is the answer and
		// the box says so rather than printing a rung as if it were a price.
		const box = only(
			offer({
				grade: 'A++',
				value: value({ priced: 'fallback', total: 846, drivers: [uniqueTerm({ unitPrice: null, chaos: null })] })
			})
		);

		expect(box.valueText).toBe('grade A++');
		expect(box.value).toBeNull();
		expect(box.marks).toEqual(['F', 'G']);
	});

	it('words an instrumental line as what it DOES, never as a missing price', () => {
		// Temple Nexus and Shrine of Unmaking are priced at their letter because
		// summing is the wrong question for them — no price is missing. Wording
		// them like the ladder would tell a player the market is down when it is
		// not, and the number is one the advisor ranked on.
		const box = only(
			offer({ grade: 'B+', value: value({ priced: 'instrumental', total: 105.75, drivers: [] }) })
		);

		expect(box.note).toBe('valued at its letter — its worth is what it does, not what it drops');
		expect(box.valueText).toBe('106');
		expect(box.marks).not.toContain('F');
	});

	it('names the player as the source of a number they typed', () => {
		const box = only(
			offer({ value: value({ priced: 'override', total: 500, drivers: [] }) })
		);

		expect(box.note).toBe('your own number for this room');
	});

	it('says outright when a line drops nothing at all', () => {
		// An empty row list is a real answer — Chamber of Iron is worth its
		// quantity bonus and nothing else — and silence there reads as a read
		// that failed rather than as a room that drops nothing.
		const box = only(offer({ value: value({ drivers: [quantityTerm()] }) }));

		expect(box.drivers).toEqual([]);
		expect(box.note).toBe('this line drops no unique and no vial');
	});

	it('keeps the instrumental wording under a content row', () => {
		// Temple Nexus is an instrumental line AND a content line, and the two
		// say different things: the cell names what the room DOES, the note says
		// why the number is a rung and not a sum. Only the drops-nothing wording
		// yields to the cell (next test); this one stays.
		const box = only(
			offer({
				line: {
					kind: 'content',
					content: 'Upgrade a room',
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				grade: 'B+',
				value: value({ priced: 'instrumental', total: 105.75, drivers: [] })
			})
		);

		expect(box.content).toBe('Upgrade a room');
		expect(box.note).toBe('valued at its letter — its worth is what it does, not what it drops');
	});

	it('yields the empty-drop wording to the content cell', () => {
		// `this line drops no unique and no vial` is the negative of what the
		// content cell states positively, and every content line is a line with
		// no chest unique — so on this box the note would print under the cell
		// on all nineteen of them.
		const box = only(
			offer({
				line: {
					kind: 'content',
					content: 'Armour drops',
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				value: value({ drivers: [quantityTerm()] })
			})
		);

		expect(box.drivers).toEqual([]);
		expect(box.note).toBeNull();
	});

	it('keeps the player\'s own-number wording on a content line', () => {
		// The one wording the content cell does NOT say: where the number came
		// from. A player who typed 500 c for Locus of Corruption is owed that
		// note whatever the row beside it draws, so the override branch stays
		// ahead of the content one.
		const box = only(
			offer({
				line: {
					kind: 'content',
					content: 'Double-corrupt an item',
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				value: value({ priced: 'override', total: 500, drivers: [] })
			})
		);

		expect(box.note).toBe('your own number for this room');
	});

	it('sums the two area bonuses into one line and names both percentages', () => {
		const box = only(
			offer({ value: value({ drivers: [quantityTerm(), rarityTerm()] }) })
		);

		expect(box.bonus).toEqual({ label: '+6% quant · +12% rarity', amount: '+6c' });
	});

	it('withholds the bonus amount where nothing priced the rates', () => {
		// The fallback path nulls every term's chaos. `+0c` there would claim
		// the percentages are worth nothing rather than that nothing priced
		// them.
		const box = only(
			offer({
				value: value({
					priced: 'fallback',
					drivers: [quantityTerm({ chaos: null }), rarityTerm({ chaos: null })]
				})
			})
		);

		expect(box.bonus?.amount).toBeNull();
		expect(box.bonus?.label).toBe('+6% quant · +12% rarity');
	});

	it('shows the headline number the advisor ranked on, never the sum of the rows', () => {
		// POE-257 D6 and epic lock L2 together. This row is a tier-1 kill: its
		// drivers are the TIER-3 room's terms, copied unscaled, and adding them
		// up gives 68 + 41 = 109 against a total of 676.8. A box that summed its
		// own rows would print one of the two numbers the design exists to keep
		// apart.
		const box = only(
			offer({
				builtTier: 1,
				value: value({
					total: 676.8,
					scaledFromTier3: 846,
					drivers: [uniqueTerm(), vialTerm(), fractionTerm()]
				})
			})
		);

		expect(box.value).toBe(676.8);
		expect(box.valueText).toBe('677');
	});

	it('draws three driver rows and folds the rest into one line', () => {
		// The height is designed against the 316 px the panel's own diagonal
		// admits, so a fourth row cannot grow the box — it folds, with the chaos
		// it contributed named so the fold is not a silent omission.
		const box = only(
			offer({
				value: value({
					drivers: [saleTerm({ chaos: 186 }), uniqueTerm(), vialTerm(), uniqueTerm({ name: 'Fate of the Vaal' })]
				})
			})
		);

		expect(box.drivers).toHaveLength(3);
		expect(box.driverCount).toBe(4);
		expect(box.fold).toBe('+1 more item · 17c');
	});

	it('withholds the folded chaos on a scaled row, where it would be tier 3\'s', () => {
		// Same trap as the headline: summing copied tier-3 terms under a tier-1
		// total would print a number from the wrong tier. The COUNT is still
		// true at every tier, so it stays.
		const box = only(
			offer({
				builtTier: 1,
				value: value({
					total: 676.8,
					scaledFromTier3: 846,
					drivers: [saleTerm({ chaos: 186 }), uniqueTerm(), vialTerm(), uniqueTerm({ name: 'Fate of the Vaal' }), fractionTerm()]
				})
			})
		);

		expect(box.fold).toBe('+1 more item');
	});

	it('withholds the BONUS chaos on a scaled row, where it is tier 3\'s too', () => {
		// The rule the fold states has to reach every amount on the box: the
		// bonus driver carries the tier-3 room's rates against the tier-3
		// room's value, so `+6c` under a tier-1 headline is a number from the
		// wrong tier — printed two lines below a fold that has just refused to
		// print one. The percentages are the room's at every tier and stay.
		const box = only(
			offer({
				builtTier: 1,
				value: value({
					total: 676.8,
					scaledFromTier3: 846,
					drivers: [uniqueTerm(), quantityTerm(), rarityTerm(), fractionTerm()]
				})
			})
		);

		expect(box.bonus).toEqual({ label: '+6% quant · +12% rarity', amount: null });
	});

	it('still prints a row\'s UNIT price on a scaled box, the one amount scaling does not touch', () => {
		// The carve-out, and the reason the rule is about DERIVED amounts. 68 c
		// is what one Story of the Vaal goes for at every tier and is the
		// number a player checks against poe.ninja; withholding it would leave
		// the row with an icon and nothing to look up.
		const box = only(
			offer({
				builtTier: 1,
				value: value({
					total: 676.8,
					scaledFromTier3: 846,
					drivers: [uniqueTerm(), fractionTerm()]
				})
			})
		);

		expect(box.drivers[0].price).toBe('68c');
	});

	it('shows the tier-3 sale PRICE on a scaled box, never the delta that is tier 3\'s', () => {
		// The sale row is the one whose ordinary cell is itself a derived
		// amount — what the room ADDED above the floor — so it is the one that
		// leaks a tier-3 number past the rule the fold and the bonus already
		// follow. The committed sample is the proof: a 12.5 delta on a row
		// whose scaled total is 10, printing `+13c` under `10`, which reads as
		// the box's own number and is larger than it. A scaled box shows the
		// tier-3 room's PRICE instead, unsigned and named for its tier, which
		// is the treatment every drop row on that box already gets. The
		// unscaled delta is pinned above, in the gain-form case.
		const box = only(
			offer({
				builtTier: 1,
				value: value({
					total: 676.8,
					scaledFromTier3: 846,
					drivers: [saleTerm(), uniqueTerm(), fractionTerm()]
				})
			})
		);

		expect(box.drivers[0].name).toBe('tier 3 sale price');
		expect(box.drivers[0].price).toBe('196c');
		// The compact strip reads the same number, because it reads the same
		// field — a second rule there is a second place to leak from.
		expect(box.stripPrices).toBe('196 · 68c');
	});

	it('folds nothing at three rows', () => {
		const box = only(
			offer({ value: value({ drivers: [saleTerm({ chaos: 186 }), uniqueTerm(), vialTerm()] }) })
		);

		expect(box.fold).toBeNull();
	});

	it('carries the vial upgrade with a price for each of its three members', () => {
		const box = only(
			offer({
				value: value({ drivers: [uniqueTerm()] }),
				recipe: {
					base: { name: 'Story of the Vaal', chaos: 5 },
					vial: { name: 'Vial of Fate', chaos: 1 },
					upgraded: { name: 'Fate of the Vaal', chaos: 39.2 }
				}
			})
		);

		expect(box.recipe?.base).toEqual({
			name: 'Story of the Vaal',
			iconName: 'Story of the Vaal',
			price: '5c',
			priced: true
		});
		expect(box.recipe?.upgraded.price).toBe('39c');
	});

	it('draws no recipe line for a line no vial upgrades', () => {
		// A line whose unique is nobody's recipe base: the box prints no recipe
		// row rather than inventing one, which would name two items the room
		// never drops.
		expect(only(offer({ value: value({ drivers: [uniqueTerm()] }) })).recipe).toBeNull();
	});

	it('draws an em dash for a recipe member this read could not price', () => {
		const box = only(
			offer({
				value: value({ drivers: [uniqueTerm()] }),
				recipe: {
					base: { name: 'Story of the Vaal', chaos: 5 },
					vial: { name: 'Vial of Fate', chaos: null },
					upgraded: { name: 'Fate of the Vaal', chaos: null }
				}
			})
		);

		expect(box.recipe?.vial.price).toBe('—');
	});

	it('marks the row that was priced from a window, and the one that is thin', () => {
		// POE-252 and POE-131, on the row each is about. Both muted, because the
		// price WAS measured — a yellow mark there would say nobody measured it,
		// which is what `G` means and these do not.
		const box = only(
			offer({
				value: value({
					drivers: [uniqueTerm({ windowPriced: true, guessed: false }), vialTerm({ lowConfidence: true })]
				})
			})
		);

		expect(box.drivers[0].marks).toEqual(['W']);
		expect(box.drivers[1].marks).toEqual(['L', 'G']);
	});

	it('rolls any guessed term up to the single box value mark', () => {
		// The value mark is the one provenance signal on the compact header. It
		// remains true whether the guessed term has a visible row or is folded.
		const withRows = only(offer({ value: value({ guessed: false, drivers: [uniqueTerm()] }) }));
		const without = only(
			offer({ value: value({ priced: 'instrumental', guessed: true, drivers: [] }) })
		);

		expect(withRows.marks).toEqual(['G']);
		expect(without.marks).toEqual(['G']);
	});

	it('does not mark the box when no value term is guessed', () => {
		const box = only(
			offer({
				value: value({
					guessed: false,
					drivers: [uniqueTerm({ guessed: false }), vialTerm({ guessed: false })]
				})
			})
		);

		expect(box.marks).toEqual([]);
	});

	it('prints the header\'s estimate word off that mark and nothing else', () => {
		// POE-277 v5 retired the boxed `G` and spent it on a word: the second
		// header line reads `per run · est.` or `floor · 1 unpriced · est.`.
		// The CONDITION is the two tests above — `marks` is still where a
		// guessed term rolls up to — so what is left to pin is that the word
		// hangs off that array rather than off a second reading of
		// `value.guessed`, which would let the letter and the word disagree
		// about the same box. Both links are asserted because the word on
		// screen is wrong if either moves.
		//
		// Read off the source because this app has no DOM harness for a
		// `.svelte` file — the same seam the mod-price cell above uses.
		const headSub = offerBoxesSource.slice(
			offerBoxesSource.indexOf('<div class="head-sub">'),
			offerBoxesSource.indexOf('<div class="rule">')
		);

		expect(headSub).toContain('{#if guessed(box)}<span class="est">· est.</span>{/if}');
		expect(offerBoxesSource).toContain('return box.marks.includes(\'G\');');
	});

	it('keeps the fallback F mark when its terms are not guessed', () => {
		const box = only(
			offer({
				grade: 'A',
				value: value({
					priced: 'fallback',
					guessed: false,
					drivers: [uniqueTerm({ guessed: false, unitPrice: null, chaos: null })]
				})
			})
		);

		expect(box.marks).toEqual(['F']);
	});

	it('reports the market\'s own staleness, not the value\'s withheld age', () => {
		// `RoomValueView.asOf` is null on a stale read by construction — a stale
		// read priced nothing — so the field that withholds the age cannot be
		// the one that reports it. Reading it there would leave a stale board
		// looking fresh.
		const stale = {
			asOf: NOW - 3 * 60 * MINUTE,
			stale: true,
			staleAfterMs: STALE_AFTER,
			unavailable: true
		};

		expect(only(offer({ value: value() }), stale).stale).toBe(true);
		expect(only(offer({ value: value() })).stale).toBe(false);
	});

	it('draws no value row at all for an offer that resolved to no room', () => {
		// Nothing to price, so nothing is priced. A zero or a dash here would be
		// a claim about a room the app could not name.
		const box = only(offer({ displayName: null, builtTier: null, grade: null, lineTop: null }));

		expect(box.valueText).toBeNull();
		expect(box.state).toBeNull();
		expect(box.drivers).toEqual([]);
	});

	it('keeps the bonus line when no tier ladder is available', () => {
		const box = only(
			offer({
				builtTier: null,
				line: {
					kind: 'chest',
					content: null,
					modArchitect: null,
					modHint: null,
					modSlots: [],
					modWorth: null,
					quantityPct: null,
					rarityPct: null
				},
				value: value({ drivers: [quantityTerm()] })
			})
		);

		expect(box.ladder).toBeNull();
		expect(box.bonus).toEqual({ label: '+6% quant', amount: '+3c' });
	});

	it('omits retired explanation fields from a scaled offer box', () => {
		const box = only(
			offer({
				builtTier: 1,
				value: value({
					scaledFromTier3: 846,
					drivers: [uniqueTerm(), fractionTerm()]
				})
			})
		);

		expect(box).not.toHaveProperty('scaleNote');
		expect(box).not.toHaveProperty('rating');
		expect(box).not.toHaveProperty('reason');
		expect(box).not.toHaveProperty('foot');
	});

	it('gives the compact strip its prices', () => {
		// The compact form drops the counts, the bonus, the recipe and the
		// mod block. Only the PRICED rows reach the strip: a bare `no price` in a
		// run of numerals
		// reads as one of them.
		const box = only(
			offer({
				value: value({
					priced: 'partial',
					drivers: [uniqueTerm(), vialTerm(), modTerm()]
				})
			})
		);

		expect(box.stripPrices).toBe('68 · 41c');
	});

	it('leaves the strip empty where nothing priced', () => {
		const box = only(
			offer({ value: value({ priced: 'fallback', drivers: [uniqueTerm({ unitPrice: null, chaos: null })] }) })
		);

		expect(box.stripPrices).toBeNull();
	});
});

describe('offerChaos', () => {
	it('rounds to whole chaos once the number is big enough to read', () => {
		expect(offerChaos(676.8)).toBe('677');
		expect(offerChaos(96)).toBe('96');
	});

	it('keeps one decimal between 1 and 10, where rounding would lose a fifth of it', () => {
		expect(offerChaos(5)).toBe('5');
		expect(offerChaos(5.44)).toBe('5.4');
	});

	it('never rounds a small real value down to a bare zero', () => {
		// 0.4 c is worth something and `0` says it is worth nothing, which is
		// the one claim this file may not make by accident.
		expect(offerChaos(0.4)).toBe('0.40');
	});
});

describe('offerBoxSignature', () => {
	const box = (over: Partial<OfferBox>): OfferBox => ({
		...offerBoxes(
			{
				...templeSliceDefault(),
				advice: advice(),
				panel: {
					room: null,
					roomRect: null,
					offers: [offer({ value: value({ drivers: [uniqueTerm()] }) })],
					incursionsRemaining: 6
				},
				market: LIVE_MARKET
			},
			NOW
		)[0],
		...over
	});

	/** A mod block to vary the slot list on, so the chip tests differ in the
	 *  one field they are about. */
	const MOD = {
		name: 'Puhuarte',
		hint: 'temple gloves',
		slots: [] as ItemSlotId[],
		price: '30c',
		perRun: '×2',
		worth: 'neutral' as ModWorth,
		marks: []
	};

	it('does not change when only the rendered TEXT changes', () => {
		// The POE-258 regression this replaces: the market-age line was in the
		// measurement signature, so `prices 12 min old` becoming `prices 13 min
		// old` re-measured the pair and hid it for a frame, once a minute, for
		// as long as a board was up. The box is fixed-width now, so nothing a
		// string says can move it — and this is the assertion that keeps a
		// future field from being added back in.
		const first = box({ market: 'prices 12 min old', headline: 'Torment Cells' });
		const second = box({ market: 'prices 13 min old', headline: 'Locus of Corruption' });

		expect(offerBoxSignature(second, false)).toBe(offerBoxSignature(first, false));
	});

	it('does not read the stale FLAG, whose drawing costs no height', () => {
		// All the flag itself draws is a dotted underline and a yellow age
		// line, neither of which is a row. What a real stale board changes is
		// `state` — Rust prices nothing off a stale snapshot, so the box comes
		// back on the cold ladder with no rows — and `state` is in the
		// signature, so that transition re-measures on the field that names the
		// form rather than on a second spelling of it. Hence a box differing in
		// the flag ALONE, which is what this asserts.
		expect(offerBoxSignature(box({ stale: true }), false)).toBe(
			offerBoxSignature(box({ stale: false }), false)
		);
	});

	it('changes when the value state does, which is what a stale board changes', () => {
		// The other half, and the one that makes the flag's absence safe: a
		// board going stale takes every box from `market` to `fallback`, and
		// the fallback form has no driver rows and no bonus line. A signature
		// blind to that would leave the pair measured for a box that is no
		// longer on screen.
		expect(offerBoxSignature(box({ state: 'fallback' }), false)).not.toBe(
			offerBoxSignature(box({ state: 'market' }), false)
		);
	});

	it('changes when a driver row appears', () => {
		const one = box({});
		const two = box({ drivers: [...one.drivers, one.drivers[0]] });

		expect(offerBoxSignature(two, false)).not.toBe(offerBoxSignature(one, false));
	});

	it('changes when the unique/vial row changes from one cell to two', () => {
		const one = box({});
		const vial = { ...one.drivers[0], kind: 'vial' as const };
		const paired = box({ dropPair: [one.drivers[0], vial] });

		expect(offerBoxSignature(paired, false)).not.toBe(offerBoxSignature(one, false));
	});

	it('changes when the content cell appears in place of the recipe row', () => {
		// A content box draws prose where a chest box draws `A + B → C`, and the
		// two are not the same height. A signature blind to it would leave the
		// pair measured for the box the offer used to be.
		expect(offerBoxSignature(box({ content: 'Queen Atziri' }), false)).not.toBe(
			offerBoxSignature(box({ content: null }), false)
		);
	});

	it('changes when a drop cell gains its rare-chance caption', () => {
		// The caption takes the item row from 57 px to 69, and the rate it keys
		// on is not fixed: the vials-per-run knob scales every derived rate, so
		// a player moving it can carry a cell across the 5% line without
		// touching the board.
		const one = box({});
		const captioned = box({
			dropPair: [{ ...one.drivers[0], caption: '4.8% / run' }, null]
		});

		expect(offerBoxSignature(captioned, false)).not.toBe(offerBoxSignature(one, false));
	});

	it('ignores what the caption SAYS once a cell has one', () => {
		// The same rule the market-age line is held to: `4.8% / run` and
		// `1.2% / run` are one line of the same 12 px either way, so a signature
		// keyed on the text would re-measure the pair — and hide the stack for a
		// frame — every time a price move nudged a rate.
		const one = box({});
		const captioned = box({
			dropPair: [{ ...one.drivers[0], caption: '4.8% / run' }, null]
		});
		const otherText = box({
			dropPair: [{ ...one.drivers[0], caption: '1.2% / run' }, null]
		});

		expect(offerBoxSignature(otherText, false)).toBe(offerBoxSignature(captioned, false));
	});

	it('changes when the form does', () => {
		expect(offerBoxSignature(box({}), true)).not.toBe(offerBoxSignature(box({}), false));
	});

	it('does not change when the advisor moves its pick to the other block', () => {
		// `pick` left the string with POE-277's v5: the frame is 2 px on BOTH
		// boxes now and faint is a muted frame with dimmer text, never an
		// `opacity`, so the pick buys no height. A field that decides nothing
		// about the geometry only re-measures the pair — and a re-measure hides
		// the stack for a frame — for a change the eye already has. This is the
		// assertion that keeps it out.
		expect(offerBoxSignature(box({ pick: true }), false)).toBe(
			offerBoxSignature(box({ pick: false }), false)
		);
	});

	it('changes when a fifth slot chip wraps the list to a second row', () => {
		// The threshold the component's own `.slots` rule states: the chips are
		// a 4 px-gapped wrap row inside the 300 px box, and they wrap once the
		// row passes 272 px — five (Xopec) always, and four or even three when
		// BODY ARMOUR is among them (Guatelitzi, Tacati). Two rows is the worst
		// case at any count, which the budget carries, and `slots.length` is in
		// the string as the cheap proxy for that shape: it is not the wrap rule,
		// but nothing changes the wrap without changing it. 19 px of box against
		// 37. A signature blind to the count would leave the pair measured for a
		// box one chip row shorter than the one on screen.
		const threeChips = box({ mod: { ...MOD, slots: ['helmet', 'gloves', 'amulet'] } });
		const fiveChips = box({
			mod: { ...MOD, slots: ['helmet', 'gloves', 'boots', 'amulet', 'ring'] }
		});

		expect(offerBoxSignature(fiveChips, false)).not.toBe(offerBoxSignature(threeChips, false));
	});

	it('changes when a row appears that was not there', () => {
		// Every optional row is in the signature because every one of them is
		// height. The recipe is the tallest of them at 39 px.
		const without = box({ recipe: null });
		const withOne = box({
			recipe: {
				base: { name: 'Story of the Vaal', iconName: 'Story of the Vaal', price: '5c', priced: true },
				vial: { name: 'Vial of Fate', iconName: 'Vial of Fate', price: '1c', priced: true },
				upgraded: {
					name: 'Fate of the Vaal',
					iconName: 'Fate of the Vaal',
					price: '39c',
					priced: true
				}
			}
		});

		expect(offerBoxSignature(withOne, false)).not.toBe(offerBoxSignature(without, false));
	});

	it('keys on the warning age row being there, not on its wording', () => {
		const fresh = box({ ageLine: null });
		const warning = box({ ageLine: 'prices stale (2 h)' });
		const otherWarning = box({ ageLine: 'prices stale (3 h)' });

		expect(offerBoxSignature(warning, false)).not.toBe(offerBoxSignature(fresh, false));
		expect(offerBoxSignature(otherWarning, false)).toBe(offerBoxSignature(warning, false));
	});

	it('changes when the ladder shape changes', () => {
		const quant = {
			quant: [2, 4, 6] as [number, number, number],
			rarity: null,
			quantText: ['2', '4', '6'] as [string, string, string],
			rarityText: null,
			tier: 2
		};
		const withQuant = box({ ladder: quant });
		const emptyLadder = box({
			ladder: {
				quant: null,
				rarity: null,
				quantText: null,
				rarityText: null,
				tier: 2
			}
		});
		const withRarity = box({
			ladder: {
				...quant,
				rarity: [4, 8, 12] as [number, number, number],
				rarityText: ['4', '8', '12'] as [string, string, string]
			}
		});

		expect(offerBoxSignature(emptyLadder, false)).not.toBe(offerBoxSignature(box({ ladder: null }), false));
		expect(offerBoxSignature(withQuant, false)).not.toBe(offerBoxSignature(box({ ladder: null }), false));
		expect(offerBoxSignature(withQuant, false)).not.toBe(offerBoxSignature(emptyLadder, false));
		expect(offerBoxSignature(withRarity, false)).not.toBe(offerBoxSignature(withQuant, false));
	});

	it('ignores ladder text changes', () => {
		const quant = {
			quant: [2, 4, 6] as [number, number, number],
			rarity: null,
			quantText: ['2', '4', '6'] as [string, string, string],
			rarityText: null,
			tier: 2
		};
		const withQuant = box({ ladder: quant });
		const withDifferentText = box({ ladder: { ...quant, quantText: ['20', '40', '60'] as [string, string, string] } });

		expect(offerBoxSignature(withDifferentText, false)).toBe(offerBoxSignature(withQuant, false));
	});

	it('changes for a mod block and its slot count', () => {
		const withoutMod = box({ mod: null });
		const emptyMod = box({
			mod: {
				name: 'Puhuarte',
				hint: 'temple gloves',
				slots: [],
				price: '30c',
				perRun: '×2',
				worth: 'neutral',
				marks: ['G']
			}
		});
		const oneSlot = box({
			mod: {
				name: 'Puhuarte',
				hint: 'temple gloves',
				slots: ['gloves'],
				price: '30c',
				perRun: '×2',
				worth: 'neutral',
				marks: ['G']
			}
		});
		const threeSlots = box({
			mod: {
				name: 'Puhuarte',
				hint: 'temple gloves',
				slots: ['ring', 'gloves', 'boots'],
				price: '30c',
				perRun: '×2',
				worth: 'neutral',
				marks: ['G']
			}
		});

		expect(offerBoxSignature(emptyMod, false)).not.toBe(offerBoxSignature(withoutMod, false));
		expect(offerBoxSignature(oneSlot, false)).not.toBe(offerBoxSignature(withoutMod, false));
		expect(offerBoxSignature(threeSlots, false)).not.toBe(offerBoxSignature(oneSlot, false));
	});
});

describe('marketNote', () => {
	it('states the age of the prices the board was valued at', () => {
		const market: MarketView = {
			asOf: NOW - 12 * MINUTE,
			stale: false,
			staleAfterMs: STALE_AFTER,
			unavailable: false
		};

		expect(marketNote(market, NOW)).toBe('prices 12 min old');
	});

	it('says a stale read is stale AND how old it is', () => {
		// The age is what tells a feed that has stopped from a server that was
		// never reached — both leave the board on base values, and only one is
		// worth waiting out. Fails if the stale branch drops the age, or if it
		// stops saying base values are in force.
		const market: MarketView = {
			asOf: NOW - 3 * HOUR,
			stale: true,
			staleAfterMs: STALE_AFTER,
			unavailable: true
		};

		expect(marketNote(market, NOW)).toBe('prices stale (3 h) — base values');
	});

	it('drops the base-values suffix when the read priced and only the clock aged it', () => {
		// The pair that makes the suffix mean something. Same age, same stale
		// verdict, and the numbers beside the line are different in kind: the
		// test above is a read that was ALREADY too old when it was valued, so
		// its board came off the cold grade ladder and `— base values` describes
		// it. This one priced off a live market and has since gone old, so the
		// figures on the box are real prices — the caution belongs on the age
		// and the suffix would be a false claim about the numbers. Fails if the
		// stale branch appends the suffix unconditionally.
		const aged: MarketView = {
			asOf: NOW - 3 * HOUR,
			stale: false,
			staleAfterMs: STALE_AFTER,
			unavailable: false
		};

		expect(marketNote(aged, NOW)).toBe('prices stale (3 h)');
	});

	it('holds the staleness line to the millisecond', () => {
		// The boundary gets its own case so a drifted comparison names itself,
		// and it is the mirror of Rust's own
		// `the_stale_boundary_is_exclusive_to_the_millisecond`: a read exactly
		// `staleAfterMs` old is the last live one and one millisecond older is
		// the first stale one. Fails if `marketStale` relaxes `>` to `>=`.
		const at: MarketView = {
			asOf: NOW - STALE_AFTER,
			stale: false,
			staleAfterMs: STALE_AFTER,
			unavailable: false
		};
		const past: MarketView = { ...at, asOf: NOW - STALE_AFTER - 1 };

		expect(marketStale(at, NOW)).toBe(false);
		expect(marketStale(past, NOW)).toBe(true);
	});

	it('keeps the suffix for a read that priced nothing for a reason other than age', () => {
		// The third way into the stale branch, and the one that would slip
		// through a guard reading `stale` alone: an unusable floor. The read is
		// not flagged stale, it prices nothing all the same (`prices_anything`
		// is false), and the clock has since taken it past two hours — so the
		// board IS on base values and must say so. Fails if the suffix is gated
		// on `stale` instead of on both flags.
		const noFloor: MarketView = {
			asOf: NOW - 3 * HOUR,
			stale: false,
			staleAfterMs: STALE_AFTER,
			unavailable: true
		};

		expect(marketNote(noFloor, NOW)).toBe('prices stale (3 h) — base values');
	});

	it('says prices are unavailable when nothing has been read', () => {
		expect(marketNote(NO_MARKET, NOW)).toBe('prices unavailable — base values');
	});

	it('says prices are unavailable for a read that priced nothing', () => {
		// Reachable server, real observation, and still no usable price — an
		// unusable floor, or a league the payload did not price. `asOf` alone is
		// not the test: fails if the note reads the timestamp and skips
		// `unavailable`, which would print an age beside base-value numbers.
		const market: MarketView = {
			asOf: NOW - 5 * MINUTE,
			stale: false,
			staleAfterMs: STALE_AFTER,
			unavailable: true
		};

		expect(marketNote(market, NOW)).toBe('prices unavailable — base values');
	});

	it('switches from minutes to hours at the hour and not before', () => {
		// The boundary gets its own test so a drifted threshold names itself:
		// 59 minutes is still minutes, 60 is one hour.
		const at59: MarketView = {
			asOf: NOW - 59 * MINUTE,
			stale: false,
			staleAfterMs: STALE_AFTER,
			unavailable: false
		};
		const at60: MarketView = {
			asOf: NOW - HOUR,
			stale: false,
			staleAfterMs: STALE_AFTER,
			unavailable: false
		};

		expect(marketNote(at59, NOW)).toBe('prices 59 min old');
		expect(marketNote(at60, NOW)).toBe('prices 1 h old');
	});

	it('floors the age rather than rounding it up', () => {
		// A 59-minute read must not be announced as an hour old at exactly the
		// moment the next server recompute is due. Fails if `marketAge` rounds.
		const nearlyAnHour: MarketView = {
			asOf: NOW - (59 * MINUTE + 59_000),
			stale: false,
			staleAfterMs: STALE_AFTER,
			unavailable: false
		};

		expect(marketNote(nearlyAnHour, NOW)).toBe('prices 59 min old');
	});

	it('reads a server clock that runs ahead as no age at all', () => {
		// Clamped rather than printed: "prices -3 min old" is not a state, and
		// the two clocks are independent. Fails if the subtraction is left
		// unguarded.
		const ahead: MarketView = {
			asOf: NOW + 3 * MINUTE,
			stale: false,
			staleAfterMs: STALE_AFTER,
			unavailable: false
		};

		expect(marketNote(ahead, NOW)).toBe('prices 0 min old');
	});
});

describe('suggestedDoors', () => {
	it('takes the doors of the top recommendation only', () => {
		const doors = suggestedDoors(
			advice({
				recommendations: [ranked({ doors: ['C1-C2'] }), ranked({ doors: ['B0-C1'] })]
			})
		);
		expect(doors).toEqual(['C1-C2']);
	});

	it('is empty for a kill the advisor wants no door with', () => {
		// R3 can rank a kill with no corridor to open, and an empty list is that
		// answer — the widget then prints no door line rather than a blank one.
		expect(suggestedDoors(advice({ recommendations: [ranked({ doors: [] })] }))).toEqual([]);
		expect(suggestedDoors(null)).toEqual([]);
	});

	it('leaves the conditional door out of what to open now', () => {
		// The two answers must not merge: with one key in hand `suggestedDoors`
		// is the whole instruction, and appending the door a SECOND stone would
		// buy would tell the player to spend a key they do not have.
		const both = advice({
			recommendations: [ranked({ doors: ['B0-C1'] })],
			secondaryDoor: 'B1-C1'
		});
		expect(suggestedDoors(both)).toEqual(['B0-C1']);
		expect(secondDoor(both)).toBe('B1-C1');
	});
});

describe('secondDoor', () => {
	it('reads the corridor a second Stone of Passage would buy', () => {
		expect(secondDoor(advice({ secondaryDoor: 'B1-C1' }))).toBe('B1-C1');
	});

	it('is null when Rust published no conditional answer', () => {
		// Every reason lives on the Rust side — no second corridor, a two-key
		// primary, an RV-only pair. All of them arrive here as one null, which
		// the widget draws as no faint seal.
		expect(secondDoor(advice({ secondaryDoor: null }))).toBeNull();
		expect(secondDoor(null)).toBeNull();
	});

	it('is null for a payload from a build before the field existed', () => {
		// The field is optional on the wire, and `undefined` reaching an SVG
		// attribute inside an overlay window fails with no devtools to see it.
		expect(secondDoor(advice())).toBeNull();
	});
});

describe('convenienceDoor', () => {
	const convenience = {
		door: 'B0-C1',
		reason: 'convenience door B0-C1: shortens the Entrance → Apex walk, 5 → 4 hops'
	};

	it('reads the door to spend the key on when the move opens nothing', () => {
		expect(convenienceDoor(advice({ convenience }))).toBe('B0-C1');
		expect(convenienceNote(advice({ convenience }))).toBe(convenience.reason);
	});

	it('is null when Rust published none, and for a payload before the field existed', () => {
		// Every reason lives on the Rust side — a merge corridor, RU's veto, no
		// key, a move that already opens a door. All arrive here as one null.
		expect(convenienceDoor(advice({ convenience: null }))).toBeNull();
		expect(convenienceNote(advice({ convenience: null }))).toBeNull();
		expect(convenienceDoor(advice())).toBeNull();
		expect(convenienceDoor(null)).toBeNull();
	});

	it('stays out of the doors to open now', () => {
		// The move opens nothing, and the faint seal must not be promoted into
		// the bright one: `suggestedDoors` is the MOVE.
		const both = advice({ recommendations: [ranked({ doors: [] })], convenience });
		expect(suggestedDoors(both)).toEqual([]);
		expect(convenienceDoor(both)).toBe('B0-C1');
	});
});

describe('recommendedExit', () => {
	const exit = { door: 'C1-C2', name: 'Chamber of Iron' };

	it('reads the name Rust put on the door the move opens', () => {
		// Both halves are Rust's: which corridor, and what the plate behind it
		// read as AT ITS OWN TIER. `Chamber of Iron` is the tier-3 room of the
		// line whose tier-1 is `Armourer's Workshop`, so a reader that resolved
		// the family instead of the tier would show the player a room they are
		// not walking into.
		expect(recommendedExit(advice({ recommendedExit: exit }))).toEqual(exit);
	});

	it('is null when Rust named nothing', () => {
		// Every reason lives on the Rust side — the move opens no door, or the
		// plate behind it did not resolve. Both arrive as one null, and the
		// widget then draws the purple seal with no name rather than a guess.
		expect(recommendedExit(advice({ recommendedExit: null }))).toBeNull();
		expect(recommendedExit(null)).toBeNull();
	});

	it('is null for a payload from a build before the field existed', () => {
		// The field is optional on the wire, and `undefined` reaching the
		// widget inside an overlay window fails with no devtools to see it.
		expect(recommendedExit(advice())).toBeNull();
	});

	it('does not name the faint door, whichever answer is standing beside it', () => {
		// One label, and on the door to open NOW. A reader that fell back to
		// `secondaryDoor` or to `convenience` would put a name on the seal that
		// says *what a key the move has no use for would buy*.
		const both = advice({
			recommendations: [ranked({ doors: ['C1-C2'] })],
			recommendedExit: exit,
			secondaryDoor: 'B0-C1'
		});
		expect(recommendedExit(both)?.door).toBe('C1-C2');
		expect(faintDoor(both)).toBe('B0-C1');
		expect(recommendedExit(advice({ secondaryDoor: 'B0-C1' }))).toBeNull();
	});
});

describe('faintDoor', () => {
	it('is the second stone\'s door when there is one', () => {
		expect(faintDoor(advice({ secondaryDoor: 'B1-C1' }))).toBe('B1-C1');
	});

	it('is the convenience door when the move opens nothing', () => {
		const convenience = { door: 'B0-C1', reason: 'convenience door B0-C1: …' };
		expect(faintDoor(advice({ convenience }))).toBe('B0-C1');
	});

	it('is null with neither', () => {
		expect(faintDoor(advice())).toBeNull();
		expect(faintDoor(null)).toBeNull();
	});
});


describe('doorWarning', () => {
	it('says nothing about a read that settled the doors', () => {
		expect(doorWarning(layout())).toBeNull();
	});

	it('says do not act on the doors when the panel read was low-confidence', () => {
		// The overlay lost its warning list to POE-244's callout, and this is
		// the one line that did not move to the page: it says do not act on the
		// widget that is still on screen inside the room.
		expect(doorWarning(layout({ confidence: 'low' }))).toBe(
			'low-confidence read — do not act on these doors'
		);
	});

	it('names the beam fallback when the seals were unread', () => {
		expect(doorWarning(layout({ markerError: 'the diamond rect fell outside' }))).toBe(
			'seals unread — doors are a beam-read fallback'
		);
	});

	it('prefers the low-confidence line when both are true', () => {
		// One line, so a precedence is needed, and the stronger statement wins:
		// `Confidence::Low` is the beam read itself being a best effort, which
		// the narrower "the seals were unread" sits inside.
		expect(
			doorWarning(layout({ confidence: 'low', markerError: 'the diamond rect fell outside' }))
		).toBe('low-confidence read — do not act on these doors');
	});

	it('says nothing with no board at all', () => {
		expect(doorWarning(null)).toBeNull();
	});
});


describe('the door widget through a whole incursion', () => {
	// POE-244's core regression, from the review: `panel_not_visible` is reached
	// ONLY through the capture loop's retire, which is what the incursion itself
	// looks like — the player stepped through the door and the panel closed. The
	// Rust side keeps `advice` alive across that (`run::apply_status` on
	// `NoPanel`); this is the webview half, that the three things the door widget
	// draws are all still derivable from the slice it is left holding.
	const inRoom = () => ({
		...templeSliceDefault(),
		status: 'panel_not_visible' as const,
		layout: layout({ current: 'C1', doors: ['C1-C2'], diamond: diamond() }),
		advice: advice({ recommendations: [ranked({ doors: ['C1-C2'] })] }),
		panel: {
			room: 'Chamber of Iron',
			roomRect: null,
			offers: [offer({ architectName: 'Atmohua', displayName: 'Armoury' })],
			incursionsRemaining: 6
		}
	});

	it('still shows the widget once the panel has closed behind the player', () => {
		expect(overlayShowsDoors(inRoom())).toBe(true);
	});

	it('still resolves the architect the widget marks with its kill glyph', () => {
		// The offer boxes are gone by now — they live with the PANEL, and the
		// panel closed behind the player — so the room widget's cyan glyph is
		// the last thing on screen naming the kill, and `chosenOffer` is what
		// picks the block it is drawn on.
		expect(chosenOffer(inRoom())?.architectName).toBe('Atmohua');
		expect(offerBuilds(chosenOffer(inRoom())!)).toBe('Armoury (tier 2)');
	});

	it('still marks the door the advisor wants opened', () => {
		// The purple seal. Empty here would mean the widget draws a room with no
		// recommendation in it at the moment the door is actually opened.
		expect(suggestedDoors(inRoom().advice)).toEqual(['C1-C2']);
	});
});
