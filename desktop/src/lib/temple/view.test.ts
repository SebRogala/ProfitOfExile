import { describe, it, expect } from 'vitest';
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
	leadReason,
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
	faintDoor,
	secondDoor,
	suggestedDoors,
	topGamble,
	topRecommendation,
	unknownRoomsBadge,
	chosenOffer,
	doorWarning,
	marketNote,
	offerBoxes,
	offerBoxSignature,
	offerChaos,
	type OfferBox
} from './view';
import { templeSliceDefault, type AdviceView, type DriverView, type LayoutView, type MarketView, type OfferView, type RankedView, type RoomValueView, type SlotId, type SlotView, type TempleStatus } from './slice';

/** A fixed clock, so every age below is the difference the test states. */
const NOW = 1_788_665_199_649;
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

/** The market a build that has never reached a server is on — Rust's own
 *  default, taken from the mirror rather than retyped. */
const NO_MARKET: MarketView = templeSliceDefault().market;

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

	it('leads with the first reason, and reports none rather than an empty string', () => {
		expect(leadReason(ranked())).toBe('R1: connects toward the top');
		expect(leadReason(ranked({ reasons: [] }))).toBeNull();
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
		expect(boxes.map((box) => box.headline)).toEqual([
			'Guatelitzi · change',
			'Atmohua · change'
		]);
	});

	it('names the room each kill BUILDS, not the one its block printed', () => {
		// POE-169 again, on the surface that has the room: Contested Development
		// prints one line and builds `currentTier + 1` of it.
		const boxes = offerBoxes(
			slice([offer({ printedTarget: "Sadist's Den", displayName: 'Torment Cells', builtTier: 2 })])
		);
		expect(boxes[0].builds).toBe('Torment Cells (tier 2)');
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

	it('gives each box the reason of the ranked entry that names ITS block', () => {
		// The lookup is by index, over recommendations and then gambles — so a
		// board whose recommendation is about block 1 and whose gamble is about
		// block 0 puts each reason on its own box. A positional read of the two
		// lists would swap them, which is the failure this arrangement exists to
		// catch.
		const boxes = offerBoxes(
			slice([offer({ index: 0 }), offer({ index: 1 })], {
				recommendations: [ranked({ architectIndex: 1, reasons: ['R1: connects toward the top'] })],
				gambles: [ranked({ architectIndex: 0, risk: 0.31, reasons: ['RV: above the risk threshold'] })]
			})
		);
		expect(boxes.map((box) => box.reason)).toEqual([
			'RV: above the risk threshold',
			'R1: connects toward the top'
		]);
	});

	it('leaves the reason off a block the ranking named nowhere', () => {
		// The advisor ranks MOVES, not architects: a ranking whose only entry is
		// about block 1 says nothing about block 0, and a borrowed reason would
		// attribute block 1's argument to it. This is an UNNAMED block and not
		// the `kill either` shape — that one names no index at all and is the
		// case below.
		const boxes = offerBoxes(
			slice([offer({ index: 0 }), offer({ index: 1 })], {
				recommendations: [ranked({ architectIndex: 1, reasons: ['R1: connects toward the top'] })]
			})
		);
		expect(boxes[0].reason).toBeNull();
	});

	it('puts the top recommendation\'s own reason on every box when the ranking names no architect', () => {
		// `kill either` names no index, so its lead reason is the DOOR
		// instruction — computed, still valid, and about neither block. Looked
		// up by index it would be dropped, and on a board where neither offer
		// resolved that leaves two boxes saying "does not resolve to a known
		// room" and nothing else while the one instruction there is goes
		// unsaid. The attribution is unambiguous precisely because no architect
		// is named: the advisor said either kill is fine.
		const boxes = offerBoxes(
			slice([offer({ index: 0 }), offer({ index: 1 })], {
				recommendations: [
					ranked({
						headline: 'kill either',
						architectIndex: null,
						reasons: ['R3: the doors are the whole board — open D3-C2']
					})
				]
			})
		);
		expect(boxes.map((box) => box.reason)).toEqual([
			'R3: the doors are the whole board — open D3-C2',
			'R3: the doors are the whole board — open D3-C2'
		]);
	});

	it('prints the line\'s grade with the tier-3 room it was given for', () => {
		// The grade is the LINE's. A kill landing on tier 2 carries the family's
		// letter, so the box names the room that letter is about — without it
		// the rating reads as a rating of `Torment Cells`.
		const boxes = offerBoxes(
			slice([
				offer({
					displayName: 'Torment Cells',
					builtTier: 2,
					grade: 'C',
					lineTop: "Sadist's Den"
				})
			])
		);
		expect(boxes[0].rating).toBe("Vertolka C · T3 Sadist's Den");
	});

	it('drops the tier-3 name when the kill lands on it', () => {
		// `builds` already names that exact room, and repeating it is noise on a
		// box read at arm's length over a game.
		const boxes = offerBoxes(
			slice([
				offer({
					displayName: 'Locus of Corruption',
					builtTier: 3,
					grade: 'A++',
					lineTop: 'Locus of Corruption'
				})
			])
		);
		expect(boxes[0].rating).toBe('Vertolka A++');
	});

	it('prints no rating for an offer that resolved to no line', () => {
		// No line, nothing graded. A blank rating line is better than a letter
		// invented for a room the app could not name.
		const boxes = offerBoxes(
			slice([offer({ displayName: null, builtTier: null, grade: null, lineTop: null })])
		);
		expect(boxes[0].rating).toBeNull();
		expect(boxes[0].builds).toBe('does not resolve to a known room');
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
			market: { asOf: NOW - 12 * MINUTE, stale: false, unavailable: false }
		};

		const boxes = offerBoxes(priced, NOW);

		expect(boxes.map((box) => box.market)).toEqual([
			'prices 12 min old',
			'prices 12 min old'
		]);
	});

	it('says on the box when the ranking is running on base values', () => {
		// The failure this catches is the one the player cannot see: a board
		// ranked off the cold grade ladder prints the same kind of chaos number
		// as a board ranked off the feed. Fails if the box stops carrying the
		// note, or carries the priced wording for an unavailable market.
		const cold = { ...slice([offer({ index: 0 })]), market: NO_MARKET };

		const boxes = offerBoxes(cold, NOW);

		expect(boxes[0].market).toBe('prices unavailable — base values');
	});
});


// ------------------------------------------- the value fixtures (POE-260) --

/** A live read, twelve minutes old — the age the design's artboards print. */
const LIVE_MARKET: MarketView = { asOf: NOW - 12 * MINUTE, stale: false, unavailable: false };

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

	it('gives a temple-mod row no icon, because its name is a hint and not an item', () => {
		// `drops.rs` prices the architect's signature rare off a prose hint
		// (`temple gloves`), which is not a poe.ninja name — so
		// `/api/gem-icon/temple%20gloves` is a request that cannot succeed. The
		// row draws the unresolved glyph either way; naming the item there only
		// buys a guaranteed 404 per row.
		const box = only(offer({ value: value({ drivers: [modTerm()] }) }));

		expect(box.drivers[0].kind).toBe('mod');
		expect(box.drivers[0].name).toBe('temple gloves');
		expect(box.drivers[0].iconName).toBeNull();
	});

	it('shows the LETTER and an F mark where the ladder priced the room', () => {
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
		expect(box.marks).toEqual(['F']);
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

	it('says on a scaled box that the rows under it are the tier-3 room\'s', () => {
		// Without this line the box is a sum that does not sum, which is worse
		// than no explanation: the rows are the LINE's terms and the number is a
		// fraction of the line.
		const box = only(
			offer({
				builtTier: 1,
				value: value({ total: 676.8, scaledFromTier3: 846, drivers: [uniqueTerm(), fractionTerm()] })
			})
		);

		expect(box.scaleNote).toBe('tier 1 = 80% of tier 3 · the rows below are tier 3\'s');
	});

	it('leaves the scale line off a row that was not scaled, whose rows DO add up', () => {
		expect(only(offer({ value: value({ drivers: [uniqueTerm()] }) })).scaleNote).toBeNull();
	});

	it('draws three driver rows and folds the rest into one line', () => {
		// The height is designed against the 316 px the panel's own diagonal
		// admits, so a fourth row cannot grow the box — it folds, with the chaos
		// it contributed named so the fold is not a silent omission.
		const box = only(
			offer({
				value: value({
					drivers: [saleTerm({ chaos: 186 }), uniqueTerm(), vialTerm(), modTerm()]
				})
			})
		);

		expect(box.drivers).toHaveLength(3);
		expect(box.driverCount).toBe(4);
		expect(box.fold).toBe('+1 more item · 60c');
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
					drivers: [saleTerm({ chaos: 186 }), uniqueTerm(), vialTerm(), modTerm(), fractionTerm()]
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
			price: '5c'
		});
		expect(box.recipe?.upgraded.price).toBe('39c');
	});

	it('draws no recipe line for a line no vial upgrades', () => {
		// Locus of Corruption: it drops Shadowstitch, which nothing transforms.
		// An invented recipe there would name two items the room never drops.
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

	it('rolls the guess up to the box only when there is no row to carry it', () => {
		// With rows on screen the estimate is attributable — this count, that
		// price — and a box-level mark saying the same thing again is noise. An
		// instrumental line has no rows at all, so its `G` has nowhere else to
		// go.
		const withRows = only(offer({ value: value({ guessed: true, drivers: [uniqueTerm()] }) }));
		const without = only(
			offer({ value: value({ priced: 'instrumental', guessed: true, drivers: [] }) })
		);

		expect(withRows.marks).toEqual([]);
		expect(without.marks).toEqual(['G']);
	});

	it('reports the market\'s own staleness, not the value\'s withheld age', () => {
		// `RoomValueView.asOf` is null on a stale read by construction — a stale
		// read priced nothing — so the field that withholds the age cannot be
		// the one that reports it. Reading it there would leave a stale board
		// looking fresh.
		const stale = { asOf: NOW - 3 * 60 * MINUTE, stale: true, unavailable: true };

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

	it('gives the compact strip its prices and one foot line', () => {
		// The compact form drops the counts, the bonus, the recipe and the
		// reason, and merges the rating and the age into one line. Only the
		// PRICED rows reach the strip: a bare `no price` in a run of numerals
		// reads as one of them.
		const box = only(
			offer({
				value: value({
					priced: 'partial',
					drivers: [uniqueTerm(), vialTerm(), modTerm({ unitPrice: null, chaos: null })]
				})
			})
		);

		expect(box.stripPrices).toBe('68 · 41c');
		expect(box.foot).toBe('Vertolka C · T3 Sadist\'s Den · prices 12 min old');
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

	it('does not change when only the rendered TEXT changes', () => {
		// The POE-258 regression this replaces: the market-age line was in the
		// measurement signature, so `prices 12 min old` becoming `prices 13 min
		// old` re-measured the pair and hid it for a frame, once a minute, for
		// as long as a board was up. The box is fixed-width now, so nothing a
		// string says can move it — and this is the assertion that keeps a
		// future field from being added back in.
		const first = box({ market: 'prices 12 min old', reason: 'R1: connects toward the top' });
		const second = box({ market: 'prices 13 min old', reason: 'R4: below the risk threshold' });

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

	it('changes when the form does', () => {
		expect(offerBoxSignature(box({}), true)).not.toBe(offerBoxSignature(box({}), false));
	});

	it('changes when a row appears that was not there', () => {
		// Every optional row is in the signature because every one of them is
		// height. The recipe is the tallest of them at 39 px.
		const without = box({ recipe: null });
		const withOne = box({
			recipe: {
				base: { name: 'Story of the Vaal', iconName: 'Story of the Vaal', price: '5c' },
				vial: { name: 'Vial of Fate', iconName: 'Vial of Fate', price: '1c' },
				upgraded: { name: 'Fate of the Vaal', iconName: 'Fate of the Vaal', price: '39c' }
			}
		});

		expect(offerBoxSignature(withOne, false)).not.toBe(offerBoxSignature(without, false));
	});
});

describe('marketNote', () => {
	it('states the age of the prices the board was valued at', () => {
		const market: MarketView = {
			asOf: NOW - 12 * MINUTE,
			stale: false,
			unavailable: false
		};

		expect(marketNote(market, NOW)).toBe('prices 12 min old');
	});

	it('says a stale read is stale AND how old it is', () => {
		// The age is what tells a feed that has stopped from a server that was
		// never reached — both leave the board on base values, and only one is
		// worth waiting out. Fails if the stale branch drops the age, or if it
		// stops saying base values are in force.
		const market: MarketView = { asOf: NOW - 3 * HOUR, stale: true, unavailable: true };

		expect(marketNote(market, NOW)).toBe('prices stale (3 h) — base values');
	});

	it('says prices are unavailable when nothing has been read', () => {
		expect(marketNote(NO_MARKET, NOW)).toBe('prices unavailable — base values');
	});

	it('says prices are unavailable for a read that priced nothing', () => {
		// Reachable server, real observation, and still no usable price — an
		// unusable floor, or a league the payload did not price. `asOf` alone is
		// not the test: fails if the note reads the timestamp and skips
		// `unavailable`, which would print an age beside base-value numbers.
		const market: MarketView = { asOf: NOW - 5 * MINUTE, stale: false, unavailable: true };

		expect(marketNote(market, NOW)).toBe('prices unavailable — base values');
	});

	it('switches from minutes to hours at the hour and not before', () => {
		// The boundary gets its own test so a drifted threshold names itself:
		// 59 minutes is still minutes, 60 is one hour.
		const at59: MarketView = { asOf: NOW - 59 * MINUTE, stale: false, unavailable: false };
		const at60: MarketView = { asOf: NOW - HOUR, stale: false, unavailable: false };

		expect(marketNote(at59, NOW)).toBe('prices 59 min old');
		expect(marketNote(at60, NOW)).toBe('prices 1 h old');
	});

	it('floors the age rather than rounding it up', () => {
		// A 59-minute read must not be announced as an hour old at exactly the
		// moment the next server recompute is due. Fails if `marketAge` rounds.
		const nearlyAnHour: MarketView = {
			asOf: NOW - (59 * MINUTE + 59_000),
			stale: false,
			unavailable: false
		};

		expect(marketNote(nearlyAnHour, NOW)).toBe('prices 59 min old');
	});

	it('reads a server clock that runs ahead as no age at all', () => {
		// Clamped rather than printed: "prices -3 min old" is not a state, and
		// the two clocks are independent. Fails if the subtraction is left
		// unguarded.
		const ahead: MarketView = { asOf: NOW + 3 * MINUTE, stale: false, unavailable: false };

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
