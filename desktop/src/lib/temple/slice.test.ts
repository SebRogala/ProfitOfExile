import { describe, it, expect } from 'vitest';
import { isTempleStatus, templeSliceDefault, type TempleSlice } from './slice';

/**
 * The wire contract between `src-tauri/src/temple/slice.rs` and this mirror.
 *
 * Two mirrors of one shape cannot be checked against each other at build time,
 * so ONE pinned string is checked from both sides. The two literals below are
 * copied verbatim out of the Rust tests
 * `the_default_slice_json_is_pinned_for_the_typescript_mirror` and
 * `a_populated_slice_json_is_pinned_for_the_typescript_mirror`, which assert
 * `serde_json::to_string(...)` against exactly these characters. A `rename_all`
 * dropped on either side, a field renamed, a default moved — all of them fail a
 * test here AND a test there, instead of rendering `undefined` on the page.
 *
 * Re-pin by running `cargo test temple::slice::tests::` and copying the string
 * the failure prints; never by editing this file until the Rust side agrees.
 */

/** Rust: `serde_json::to_string(&TempleSlice::default())`. */
const RUST_DEFAULT_JSON =
	'{"status":"idle","layout":null,"panel":null,"advice":null,"mode":null,"keys":0,"config":{"artefactsOfTheVaal":true,"scarabOfTimelines":false},"profile":{"apexScore":2.0,"pathCost":0.0,"rerollUntilFavourable":false,"r4KeepUpgradeTargets":true},"unknownRooms":[],"lastReadAt":null,"calibration":null,"lastError":null}';

/** Rust: the fully populated sample, `SAMPLE_SLICE_JSON` in `slice.rs`. */
const RUST_SAMPLE_JSON =
	'{"status":"read","layout":{"slots":[{"slot":"A0","name":"Apex of Atzoatl","tier":0,"exact":true,"known":true,"current":false}],"doors":["C1-C2"],"uncertain":["B0-C1"],"unresolvedIncident":["B0-C1"],"markerError":"the diamond rect fell outside the capture","current":"C1","scale":0.99,"ncc":0.94,"confidence":"high","origin":[900,900],"centres":[[900,465],[795,569],[1005,569],[690,673],[900,673],[1110,673],[585,777],[795,777],[1005,777],[1215,777],[690,881],[900,900],[1110,881]],"rois":[{"kind":"panel","of":null,"rect":[1100,40,500,400]},{"kind":"corridor","of":"C1-C2","rect":[991,659,27,27]}],"diamond":{"corners":[[1.457,0.0],[0.0,1.154],[-1.457,0.0],[0.0,-1.154]],"seals":[{"neighbour":"C2","edge":"C1-C2","pos":[0.74663,-0.66524]}]}},"panel":{"room":"Locus of Corruption","roomRect":[1300,100,152,20],"offers":[{"index":0,"architectName":"Guatelitzi","kind":"upgrade","printedTarget":"Sadist\'s Den","displayName":"Torment Cells","builtTier":2,"rect":[1300,140,280,43]}],"incursionsRemaining":6},"advice":{"recommendations":[{"headline":"upgrade → Locus of Corruption","doorsLabel":"C1-C2, B0-C1","doors":["C1-C2","B0-C1"],"architectIndex":0,"ev":12.5,"risk":null,"reasons":["R1: connects toward the top"]}],"gambles":[{"headline":"kill either","doorsLabel":"no door","doors":[],"architectIndex":null,"ev":14.0,"risk":0.31,"reasons":["RV: excluded above the risk threshold"]}],"mapAction":"leaveMap","warnings":["the incursion budget was not legible","1 of 2 architects read — the kill shown is forced, not chosen"],"forcedKill":true},"mode":"chase","keys":2,"config":{"artefactsOfTheVaal":false,"scarabOfTimelines":true},"profile":{"apexScore":3.5,"pathCost":1.25,"rerollUntilFavourable":true,"r4KeepUpgradeTargets":false},"unknownRooms":["D3"],"lastReadAt":1700000000000,"calibration":{"screen_w":2560,"screen_h":1440,"scale":0.99},"lastError":"Temple: OCR failed"}';

describe('templeSliceDefault', () => {
	it('is exactly what Rust sends for a slice nothing has written yet', () => {
		// Structural equality against the PARSED Rust default, so a key this
		// mirror invents (or misses) fails, not just a value that differs.
		expect(templeSliceDefault()).toEqual(JSON.parse(RUST_DEFAULT_JSON));
	});

	it('reports keys as 0, not the shipped 1', () => {
		// The derive default, which is what a window polling before
		// `apply_to_state` has seeded the echo actually receives. Claiming 1
		// here would show a key count Rust never sent.
		expect(templeSliceDefault().keys).toBe(0);
	});

	it('carries the settings echo, so a page can render its controls with the module off', () => {
		const slice = templeSliceDefault();
		expect(slice.config).toEqual({ artefactsOfTheVaal: true, scarabOfTimelines: false });
		expect(slice.profile.apexScore).toBe(2.0);
	});
});

describe('the Rust sample decodes into this mirror', () => {
	// Parsed once: every assertion below reads the same object a poll would
	// hand the store, not a hand-built one.
	const slice = JSON.parse(RUST_SAMPLE_JSON) as TempleSlice;

	it('leaves no field of the sample unnamed by the mirror', () => {
		// Round-trips the sample through the DECLARED keys. Anything Rust sends
		// that this file does not name would be dropped by the rebuild and the
		// comparison fails — which is the "no unknown/renamed field" check.
		const declared: TempleSlice = {
			status: slice.status,
			layout: slice.layout,
			panel: slice.panel,
			advice: slice.advice,
			mode: slice.mode,
			keys: slice.keys,
			config: slice.config,
			profile: slice.profile,
			unknownRooms: slice.unknownRooms,
			lastReadAt: slice.lastReadAt,
			calibration: slice.calibration,
			lastError: slice.lastError
		};
		expect(declared).toEqual(slice);
		expect(Object.keys(slice).sort()).toEqual(Object.keys(declared).sort());
	});

	it('reads the layout, including the honesty fields', () => {
		expect(slice.layout?.current).toBe('C1');
		expect(slice.layout?.doors).toEqual(['C1-C2']);
		expect(slice.layout?.unresolvedIncident).toEqual(['B0-C1']);
		expect(slice.layout?.markerError).toBe('the diamond rect fell outside the capture');
		expect(slice.layout?.slots[0].known).toBe(true);
	});

	it('reads the pixel geometry, with the Entrance sitting on the origin', () => {
		// POE-227: what a game-anchored surface places itself against. Capture
		// px, `Slot::ALL` order — so index 11 is E1, the Entrance, whose offset
		// from the origin is (0, 0) by construction. A mirror that renamed
		// either field, or landed the pairs transposed, reads undefined or the
		// wrong plate here rather than silently drawing in the wrong place.
		expect(slice.layout?.origin).toEqual([900, 900]);
		expect(slice.layout?.centres).toHaveLength(13);
		expect(slice.layout?.centres[11]).toEqual(slice.layout?.origin);
		expect(slice.layout?.centres[0]).toEqual([900, 465]);
	});

	it('reads the never-cover rects the module takes its input from', () => {
		// POE-244: the same capture px as `layout.origin` above, and the reason
		// the overlay does not compute them — five different Rust constants own
		// these rectangles, and a TypeScript copy of any one would drift with
		// nothing to fail. A mirror that renamed `rect` or `kind` places the
		// callout against an empty obstacle list, which is the failure this
		// pins: every position then looks legal.
		expect(slice.layout?.rois).toEqual([
			{ kind: 'panel', of: null, rect: [1100, 40, 500, 400] },
			{ kind: 'corridor', of: 'C1-C2', rect: [991, 659, 27, 27] }
		]);
	});

	it('reads the current room\'s diamond, shape and seals together', () => {
		// The door widget draws the outline and the seals in ONE space, so a
		// mirror that carried only one of the two fields would put every seal
		// off the shape.
		expect(slice.layout?.diamond?.corners).toHaveLength(4);
		expect(slice.layout?.diamond?.corners[0]).toEqual([1.457, 0]);
		// A seal is a UNIT vector — the panel draws every one at the same radius
		// (`markers::SEAL_RING_FRACTION`), so the ring is the unit and only the
		// direction differs. A mirror that dropped the pair would place every
		// seal at the diamond's centre.
		expect(slice.layout?.diamond?.seals).toEqual([
			{ neighbour: 'C2', edge: 'C1-C2', pos: [0.74663, -0.66524] }
		]);
	});

	it('reads the offer with BOTH the printed and the resolved name', () => {
		const offer = slice.panel?.offers[0];
		expect(offer?.printedTarget).toBe("Sadist's Den");
		expect(offer?.displayName).toBe('Torment Cells');
		expect(offer?.builtTier).toBe(2);
	});

	it('reads the screen rects the panel lines were OCR\'d at', () => {
		// POE-243: capture px — the same unit as `layout.origin` above, so a
		// surface drawing over the game needs no conversion. A mirror that
		// renamed either key reads undefined here instead of pointing at the
		// block the advice is about.
		expect(slice.panel?.roomRect).toEqual([1300, 100, 152, 20]);
		expect(slice.panel?.offers[0].rect).toEqual([1300, 140, 280, 43]);
	});

	it('reads the forced-kill flag next to the warning that explains it', () => {
		// The two halves of one fact: the flag a surface branches on, and the
		// prose it prints. A mirror carrying only the prose would leave every
		// surface matching on wording.
		expect(slice.advice?.forcedKill).toBe(true);
		expect(slice.advice?.warnings).toContain(
			'1 of 2 architects read — the kill shown is forced, not chosen'
		);
	});

	it('reads the advice, its reasons and its map action', () => {
		expect(slice.advice?.recommendations[0].reasons).toEqual(['R1: connects toward the top']);
		expect(slice.advice?.gambles[0].risk).toBe(0.31);
		expect(slice.advice?.mapAction).toBe('leaveMap');
		expect(slice.advice?.warnings).toHaveLength(2);
	});

	it('reads the calibration under its snake_case keys', () => {
		// The one struct with no `rename_all` in Rust. Renaming these to
		// `screenW` to match the parent would read undefined at runtime, and
		// TypeScript would not say a word about it.
		expect(slice.calibration?.screen_w).toBe(2560);
		expect(slice.calibration?.screen_h).toBe(1440);
		expect(slice.calibration?.scale).toBe(0.99);
	});
});

describe('isTempleStatus', () => {
	it('accepts a status Rust actually publishes', () => {
		expect(isTempleStatus('no_current_room')).toBe(true);
	});

	it('accepts the waiting status the arm gate publishes', () => {
		// POE-242's status reaches the store like any other, so an entry
		// missing from the membership table is a slice the store DROPS: the
		// page would keep showing the last board while the module sat idle.
		expect(isTempleStatus('waiting')).toBe(true);
	});

	it('rejects a status no Rust variant spells', () => {
		expect(isTempleStatus('panel_missing')).toBe(false);
	});

	it('rejects an inherited Object.prototype key', () => {
		// The membership table is a plain object, so a prototype-chain lookup
		// answers true for every key `Object.prototype` carries. A payload with
		// `"toString"` would then be published as a status no surface has a
		// branch for — which is precisely what this guard exists to stop.
		expect(isTempleStatus('toString')).toBe(false);
	});

	it('rejects a non-string the payload might carry instead', () => {
		expect(isTempleStatus(undefined)).toBe(false);
		expect(isTempleStatus(null)).toBe(false);
		expect(isTempleStatus(3)).toBe(false);
	});
});
