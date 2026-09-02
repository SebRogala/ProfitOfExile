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
	leadReason,
	leaveMapBanner,
	markerFallbackNotice,
	modeLabel,
	moveLine,
	offerBuilds,
	offerHeadline,
	overlayShowsBoard,
	plateGlyph,
	topGamble,
	topRecommendation,
	unknownRoomsBadge
} from './view';
import { templeSliceDefault, type AdviceView, type LayoutView, type OfferView, type RankedView, type SlotId, type SlotView, type TempleStatus } from './slice';

/** Every wire status, listed once so the totality checks below cannot drift. */
const ALL_STATUSES: TempleStatus[] = [
	'off',
	'idle',
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
		...over
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
		for (const status of ['off', 'idle', 'panel_not_visible', 'unavailable', 'error'] as const) {
			expect(overlayShowsBoard(status), status).toBe(false);
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

	it('calls a door the selection frame covers uncertain, not open', () => {
		// Both sets carry it: reported open, but not settled. Drawn dashed.
		expect(edgeState('C1-C2', layout({ doors: ['C1-C2'], uncertain: ['C1-C2'] }))).toBe(
			'uncertain'
		);
	});

	it('marks an unresolved corridor even though it is also uncertain', () => {
		// `unresolvedIncident` is a SUBSET of `uncertain` — the fallback flags a
		// corridor uncertain and then reports it as unresolved when nothing
		// settled it. "We could not see it" must not render as "it is shut".
		const l = layout({ doors: [], uncertain: ['B0-C1'], unresolvedIncident: ['B0-C1'] });
		expect(edgeState('B0-C1', l)).toBe('unresolved');
	});

	it('marks unresolved ahead of uncertain even when the edge is also in doors', () => {
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

	it('words all four states', () => {
		for (const state of ['open', 'uncertain', 'unresolved', 'closed'] as const) {
			expect(EDGE_STATE_LABEL[state], state).toBeTruthy();
		}
		expect(new Set(Object.values(EDGE_STATE_LABEL)).size).toBe(4);
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
