/**
 * The preset editor's own reading of the value table (POE-259).
 *
 * Pure, like `view.ts` beside it, and separate from it for what each is about:
 * `view.ts` words a BOARD — the plates, the offers, the ranked moves — while
 * this file words the SETTINGS surface that decides what a board is worth. The
 * two share a slice and nothing else.
 *
 * Nothing here computes a chaos value. Rust owns the formula
 * (`temple/valuation.rs`, ADR-022 §1) and hands the whole 25 x 3 table over
 * through `temple_value_table`; this file turns those rows into cells, says
 * where each number came from in one letter, and validates what a player types
 * into an override BEFORE it is written — which is the job `valuation::rate`
 * and `valuation::tier_fraction` name POE-259 as the owner of.
 */

import type {
	MarketView,
	RoomValueView,
	TempleCustom,
	TemplePreset,
	TempleValueRow
} from './slice';

/** The three tiers a room line has. */
export const TIERS = [1, 2, 3] as const;

// --- the picker ------------------------------------------------------------

/** The two presets, as the segmented picker lists them. Two and no third:
 *  epic lock L3 dropped Basic. */
export const PRESET_OPTIONS: { value: TemplePreset; label: string }[] = [
	{ value: 'default', label: 'Default' },
	{ value: 'custom', label: 'Custom' }
];

/** One line saying what the preset in force actually does, printed under the
 *  picker — the two differ in where their numbers come from, which is not
 *  something two button labels can say. */
export const PRESET_NOTE: Record<TemplePreset, string> = {
	default:
		'Vertolka’s drop table priced against the live market with the shipped rates. Read-only: every value below says where it came from.',
	custom:
		'Your own rates, and your own number for any room you disagree with. An empty cell keeps the formula for that tier.'
};

/**
 * The preset a picker reported, or null.
 *
 * `SegmentedButtons` reports a plain `string`, and the value goes straight to
 * a Rust command whose enum has exactly two variants — so the narrowing has to
 * happen somewhere, and a guard is cheaper than a cast that would send Rust a
 * third string the day a third option is added to the list and nowhere else.
 */
export function parsePreset(raw: string): TemplePreset | null {
	return raw === 'default' || raw === 'custom' ? raw : null;
}

// --- when the table is re-fetched ------------------------------------------

/**
 * Everything the 75 values depend on, in one string.
 *
 * The editor fetches the table rather than reading it off the SSOT snapshot,
 * so something has to say WHEN to fetch again. The slice is whole-replaced on
 * every poll, so an effect that simply read the preset would re-fetch three
 * quarters of a hundred values every three seconds; this key is what turns
 * that into a fetch per CHANGE.
 *
 * The market's two flags are in the key and not just its timestamp, and that
 * is the part worth stating: `asOf` is published on a stale read as well as a
 * live one — the age IS the message there (`MarketView.asOf`) — so a read that
 * simply grows older than the two hours Rust judges it against re-prices every
 * room onto the base ladder while its timestamp never moves. A key without
 * `unavailable` would hold the pre-stale numbers on screen, with the wrong
 * provenance letter beside each one.
 *
 * `stale` rides along even though today it cannot rise without `unavailable`
 * rising with it (a stale read prices nothing). It costs at most one redundant
 * fetch, in the case where the read was already unpriced for another reason,
 * and it means the key describes the whole market state rather than the one
 * flag that happens to imply the rest.
 */
export function valueTableKey(
	preset: TemplePreset,
	market: MarketView,
	custom: TempleCustom
): string {
	return [
		preset,
		market.asOf ?? 'cold',
		market.unavailable ? 'base' : 'priced',
		market.stale ? 'stale' : 'fresh',
		JSON.stringify(custom)
	].join('|');
}

// --- provenance ------------------------------------------------------------

/**
 * One letter saying where a cell's number came from.
 *
 * Letters and not colours: a mark that only a hue carries is a mark a
 * colour-blind player does not have. The legend below spells all six out, and
 * `markTitle` gives the cell a full sentence on hover.
 */
export type ValueMark = 'O' | 'I' | 'F' | 'P' | 'G' | 'M';

/** What each letter means, in the legend's own words. */
export const VALUE_MARK_LABEL: Record<ValueMark, string> = {
	O: 'your own number',
	I: 'instrumental — priced at its letter',
	F: 'grade fallback — nothing priced it',
	P: 'partial — some terms could not be priced',
	G: 'guessed — rests on an estimate',
	M: 'measured — every term came from the market'
};

/** The legend, as one line under the table. */
export const VALUE_MARK_LEGEND: string = (
	['M', 'G', 'P', 'F', 'I', 'O'] as ValueMark[]
)
	.map((mark) => `${mark} = ${VALUE_MARK_LABEL[mark]}`)
	.join(' · ');

/**
 * The one letter a cell wears.
 *
 * A cell can be true of two of these at once — a partial sum whose surviving
 * terms are also estimates is both P and G — so the order below is a
 * precedence and not a switch over disjoint states, worst-provenance first:
 *
 * 1. `O` — the player stated the number, so nothing else about it is news.
 * 2. `I` — one of the two instrumental lines, priced at its letter because
 *    summing is the wrong question for it. Not a missing price.
 * 3. `F` — nothing at all was priced and the grade ladder stood in.
 * 4. `P` — the sum is incomplete, so the number is an UNDERCOUNT. That outranks
 *    `G`, which says the number rests on an estimate but is whole.
 * 5. `G` / `M` — a complete sum, resting on an estimate or not.
 *
 * `guessed` is deliberately not consulted for `F` and `I`: a cold grade rung
 * reads `guessed: false` and a live one `true` (ADR-022 §3), and a cell that
 * flipped between F and G as the market came and went would be describing the
 * market rather than the number.
 */
export function valueMark(value: RoomValueView): ValueMark {
	if (value.priced === 'override') return 'O';
	if (value.priced === 'instrumental') return 'I';
	if (value.priced === 'fallback') return 'F';
	if (value.priced === 'partial') return 'P';
	return value.guessed ? 'G' : 'M';
}

/**
 * The whole provenance of a cell, for its `title`.
 *
 * The letter is one bit of a fact with several; a player deciding whether to
 * override a room wants the rest without leaving the table. `guessed` is
 * stated even where it did not pick the letter, which is the case the mark
 * cannot carry.
 */
export function markTitle(value: RoomValueView): string {
	const parts = [VALUE_MARK_LABEL[valueMark(value)]];
	if (value.guessed && valueMark(value) !== 'G') parts.push('rests on an estimate');
	if (value.scaledFromTier3 !== null) {
		parts.push(`scaled from ${formatChaos(value.scaledFromTier3)} at tier 3`);
	}
	if (value.league !== '') parts.push(`priced in ${value.league}`);
	return parts.join(' · ');
}

/**
 * A chaos amount, short enough for a table cell.
 *
 * Three bands, and the smallest one is POE-262's. At 100 c and up the decimals
 * are noise and the figure is rounded; from 1 c up two decimals are the
 * difference between two rooms. BELOW 1 c two decimals stop being a rendering
 * and start being a claim: since a room nothing priced is now a fraction of the
 * cheapest room something priced, a D-grade rung is 0.0066 c on the committed
 * capture, and `toFixed(2)` prints that as `0.01` — or, once the vial rate
 * lands, as `0.00`, which reads as ZERO and is the one thing epic lock L4
 * forbids a fallback from saying. Under 1 c the value keeps two SIGNIFICANT
 * digits instead, so 0.015 prints as `0.015` and 0.0051 as `0.0051`.
 *
 * Zero, negatives and `NaN` keep the two-decimal form they had: zero is a
 * value a player can state outright ("this room is worth nothing to me") and
 * `0.00` is the honest rendering of it, unlike a rung that was rounded there.
 *
 * Two bands the significant-digit rule would otherwise reach, and both are
 * about the CELL rather than about the number:
 *
 * - **`[0.995, 1)` takes the two-decimal form.** `toPrecision(2)` rounds
 *   0.999 up and prints it as `1.0`, a third format in a column that has only
 *   ever had two. The band hands it to `toFixed(2)` instead, so it reads
 *   `1.00` beside every other near-one figure.
 * - **Under `0.0001` the literal `<0.0001` is printed.** `toPrecision(2)`
 *   switches to exponential below 1e-6 — `1.0e-7` in a table cell — and the
 *   digits it would print above that are past anything a player acts on. The
 *   `<` form keeps the one claim that matters: not zero. It is non-zero by
 *   construction, since this branch is only reached for `chaos > 0`, which is
 *   what epic lock L4 forbids a fallback from contradicting.
 */
export function formatChaos(chaos: number): string {
	if (chaos >= 100) return Math.round(chaos).toString();
	if (chaos >= 0.995 || !(chaos > 0)) return chaos.toFixed(2);
	if (chaos < 0.0001) return '<0.0001';
	return chaos.toPrecision(2);
}

/**
 * A per-run count, short enough for the `×N` cell beside a drop row.
 *
 * A sibling of [`formatChaos`] and deliberately not the same rule: a chaos
 * amount is money and always reads with its decimals, a count is an expected
 * NUMBER of items and the whole ones are the readable case. `×2` is two
 * gloves a run; `×2.00` would read as a price.
 *
 * Three bands:
 *
 * - **Two SIGNIFICANT digits, trailing zeros trimmed**, which is the whole
 *   rule and the reason a whole number comes out whole: `2` and not `2.00`,
 *   `0` and not `0.00`. POE-262 is why it is significant digits rather than
 *   decimals: a vial rate is derived per line off poedb's chance stat, so the
 *   counts a box prints are now `0.1 × 2815/1689` and `0.1 × 20/1689` —
 *   raw, they render as `×0.16666666666666666` and `×0.0011841326228537595`,
 *   seventeen digits of float noise in a cell. Two significant digits keep
 *   the magnitude at every scale a rate reaches (`0.25`, `0.17`, `0.06`,
 *   `0.048`, `0.012`, `0.0012`) where a fixed two decimals would round the
 *   small ones to `0.00`. The trim is confined to the fraction — see the
 *   comment on it.
 * - **At 99.5 and up the count is rounded whole.** Only the knob reaches here
 *   — the shipped table's largest count is 2 — but `vialsPerRun` has no
 *   maximum, and `toPrecision(2)` switches to exponential the moment two
 *   significant digits cannot hold the integer part, so a rate of 100 would
 *   print `×1.7e+2`.
 * - **Below 0.0001 the literal `<0.0001` is printed**, for the same reason
 *   [`formatChaos`] does it: `toPrecision(2)` goes exponential under 1e-6, and
 *   `×1.2e-7` is not a cell. The `<` form keeps the one claim that
 *   matters — the room does roll for the item. Zero itself is NOT in this
 *   band: at a `vialsPerRun` of 0 the row is still listed, and `×0` is the
 *   honest rendering of what it is now worth.
 */
export function formatCount(count: number): string {
	if (count >= 99.5) return Math.round(count).toString();
	if (count > 0 && count < 0.0001) return '<0.0001';
	const digits = count.toPrecision(2);
	// Only a FRACTION is trimmed. `toPrecision(2)` prints 19.6 as `20`, and a
	// blanket trailing-zero strip would turn that into `2` — an order of
	// magnitude, silently.
	return digits.includes('.') ? digits.replace(/0+$/, '').replace(/\.$/, '') : digits;
}

// --- the rows the editor renders -------------------------------------------

/** One room-tier as the editor shows it. */
export interface ValueCell {
	/** 1, 2 or 3. */
	tier: number;
	/** What the preset in force values this room-tier at, in chaos. */
	total: number;
	/** Where that number came from, in one letter. */
	mark: ValueMark;
	/** The whole provenance, for the cell's `title`. */
	title: string;
	/** What the Custom table states for this cell, or null for "use the
	 *  formula". Always null under Default, which states nothing. */
	override: number | null;
}

/** One room line as the editor shows it. */
export interface ValueRow {
	key: string;
	name: string;
	grade: string;
	cells: ValueCell[];
}

/**
 * The table's rows, with each cell's provenance and the override behind it.
 *
 * `custom` is the table whose overrides the cells report — `null` under
 * Default, which has none and is read-only. It is NOT used to compute
 * anything: the totals are Rust's, from the preset the rows were fetched for.
 */
export function valueRows(rows: TempleValueRow[], custom: TempleCustom | null): ValueRow[] {
	return rows.map((row) => ({
		key: row.key,
		name: row.name,
		grade: row.grade,
		cells: TIERS.map((tier) => {
			const value = row.tiers[tier - 1];
			return {
				tier,
				total: value.total,
				mark: valueMark(value),
				title: markTitle(value),
				override: custom === null ? null : overrideOf(custom, row.key, tier)
			};
		})
	}));
}

/** What the Custom table states for one room-tier, or null for "use the
 *  formula". A key it does not name states nothing about any of its tiers. */
export function overrideOf(custom: TempleCustom, key: string, tier: number): number | null {
	const row = custom.rooms[key];
	if (row === undefined) return null;
	return row[tier - 1] ?? null;
}

// --- editing ---------------------------------------------------------------

/** What a player typed into one override cell. */
export type CellEdit =
	| { kind: 'clear' }
	| { kind: 'value'; chaos: number }
	| { kind: 'invalid'; reason: string };

/** What a player typed into one RATE box. A rate has no "use the formula"
 *  state, so `clear` is not one of the answers — see `parseKnob`. */
export type KnobEdit = Exclude<CellEdit, { kind: 'clear' }>;

/**
 * Read one override cell, refusing what Rust would refuse.
 *
 * Empty is not zero. An empty cell means "use the formula for this tier" and a
 * typed `0` means "this room is worth nothing to me" — a position a rusher
 * really does hold about most of the board — so the two must never collapse
 * into one another. `Number('')` is `0`, which is exactly how they would.
 *
 * The bar is `temple/preset.rs`'s: a finite chaos amount that is not negative.
 * Rust refuses the write too and is the authority; this exists so the player
 * sees WHICH cell it was while they are still looking at it, rather than
 * getting one rejection line for a table of 75.
 */
export function parseCell(raw: string): CellEdit {
	const trimmed = raw.trim();
	if (trimmed === '') return { kind: 'clear' };
	const chaos = Number(trimmed);
	if (!Number.isFinite(chaos)) return { kind: 'invalid', reason: 'not a number' };
	if (chaos < 0) return { kind: 'invalid', reason: 'not a chaos amount' };
	return { kind: 'value', chaos };
}

/**
 * The table with one cell set, or cleared when `chaos` is null.
 *
 * A new object every time — the caller hands the result straight to the Rust
 * setter, and mutating the slice's own echo would leave the page showing a
 * value the command had not accepted yet.
 *
 * A row whose three tiers are all cleared is REMOVED rather than left as
 * `[null, null, null]`. Rust already ignores such an entry
 * (`TempleCustomSettings::overrides` inserts nothing for an all-`None` row),
 * so keeping it would only write a line to `settings.json` that says the
 * player has an opinion about a room they have just said they have none about.
 */
export function withCell(
	custom: TempleCustom,
	key: string,
	tier: number,
	chaos: number | null
): TempleCustom {
	const row = [...(custom.rooms[key] ?? [null, null, null])];
	row[tier - 1] = chaos;
	const rooms = { ...custom.rooms };
	if (row.every((slot) => slot === null)) {
		delete rooms[key];
	} else {
		rooms[key] = row;
	}
	return { ...custom, rooms };
}

/**
 * The table with every room-tier set to what `rows` values it at (D3's "Copy
 * Default into Custom").
 *
 * The ONLY way Default's numbers enter Custom, and it is one explicit click:
 * switching preset must never copy, or the pair stops being two tables and the
 * player's own numbers are gone the first time they look at Default.
 *
 * The rates are left exactly as they are. `rows` are values, not knobs, and a
 * button labelled "copy the values" that also reset the drops weight would
 * undo the rusher's one setting.
 *
 * All THREE tiers are copied, and the consequence is worth stating: a stated
 * tier-1 value no longer follows its line's tier-3 one, because the fraction
 * only applies to a tier the table leaves empty. That is what copying values
 * means; clearing a cell puts it back on the formula.
 */
export function copyValuesInto(custom: TempleCustom, rows: TempleValueRow[]): TempleCustom {
	const rooms: Record<string, (number | null)[]> = {};
	for (const row of rows) {
		rooms[row.key] = row.tiers.map((value) => value.total);
	}
	return { ...custom, rooms };
}

/** The table with every override dropped, the rates untouched. The undo for
 *  `copyValuesInto`, and the way back to "every room on the formula". */
export function withoutOverrides(custom: TempleCustom): TempleCustom {
	return { ...custom, rooms: {} };
}

/** How many room-tiers the table states a number for. Printed beside the Clear
 *  button so a player can see they have overrides at all — 75 cells is more
 *  than fits on a screen. */
export function overrideCount(custom: TempleCustom): number {
	return Object.values(custom.rooms).reduce(
		(count, row) => count + row.filter((slot) => slot !== null).length,
		0
	);
}

// --- the rates -------------------------------------------------------------

/** One editable rate of the Custom preset. */
export interface KnobSpec {
	field: 'tierFraction' | 'cPerQuantity' | 'cPerRarity' | 'vialsPerRun' | 'dropsWeight' | 'comboPremium';
	label: string;
	/** The unit, spelled out — every one of these is chaos or a fraction, and
	 *  a control that does not say which is a control nobody can set. */
	hint: string;
	step: number;
	/** `tierFraction` is the one knob with a ceiling: Rust clamps it to 1, so a
	 *  stored 2.5 would show a number the app never applies. */
	max: number | null;
}

/** The six rates, in the order the editor lays them out. */
export const KNOBS: KnobSpec[] = [
	{
		field: 'tierFraction',
		label: 'Tier 1 / 2 fraction',
		hint: 'What a tier-1 or tier-2 room is worth as a fraction of its line’s tier-3 total. 0 to 1.',
		step: 0.05,
		max: 1
	},
	{
		field: 'cPerQuantity',
		label: 'Chaos per quantity %',
		hint: 'Chaos per point of increased Quantity. Unmeasured — Vertolka’s proposed rate.',
		step: 0.05,
		max: null
	},
	{
		field: 'cPerRarity',
		label: 'Chaos per rarity %',
		hint: 'Chaos per point of increased Rarity. Unmeasured — Vertolka’s proposed rate.',
		step: 0.05,
		max: null
	},
	{
		field: 'vialsPerRun',
		label: 'Vials per run',
		hint: 'Expected vials per run at tier 3 on a standard vial room (Conduit, Crucible, Sanctum); other rooms scale by poedb’s chance stat — Glittering Halls ×1.67, Locus and Throne ×0.012. Vertolka’s proposed 0.1.',
		step: 0.05,
		max: null
	},
	{
		field: 'dropsWeight',
		label: 'Drops weight',
		hint: 'A multiplier on the whole drops term. Set it to 0 for the rusher: the ranking falls back to sale value alone.',
		step: 0.1,
		max: null
	},
	{
		field: 'comboPremium',
		label: 'Combination premium',
		hint: 'Chaos the Locus + Doryani pair is worth ON TOP of the two rooms. 0 means the pair is the sum of its parts.',
		step: 5,
		max: null
	}
];

/**
 * Read one rate, refusing what Rust would refuse.
 *
 * Empty is invalid here and not a clear: a rate has no "use the formula"
 * state — the formula IS the rate — so a blank box is a half-typed number and
 * not an instruction. The return type says so, which is what saves every
 * caller a branch it could only answer with a guess.
 */
export function parseKnob(raw: string, spec: KnobSpec): KnobEdit {
	const parsed = parseCell(raw);
	if (parsed.kind === 'clear') return { kind: 'invalid', reason: 'not a number' };
	if (parsed.kind === 'invalid') return parsed;
	if (spec.max !== null && parsed.chaos > spec.max) {
		return { kind: 'invalid', reason: `at most ${spec.max}` };
	}
	return parsed;
}
