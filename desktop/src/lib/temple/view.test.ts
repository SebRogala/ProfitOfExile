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
	secondDoor,
	suggestedDoors,
	topGamble,
	topRecommendation,
	unknownRoomsBadge,
	chosenOffer,
	doorWarning,
	offerBoxes
} from './view';
import { templeSliceDefault, type AdviceView, type LayoutView, type OfferView, type RankedView, type SlotId, type SlotView, type TempleStatus } from './slice';

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
