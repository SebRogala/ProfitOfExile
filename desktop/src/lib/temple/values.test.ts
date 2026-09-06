import { describe, it, expect } from 'vitest';
import {
	KNOBS,
	PRESET_NOTE,
	PRESET_OPTIONS,
	TIERS,
	VALUE_MARK_LEGEND,
	copyValuesInto,
	formatChaos,
	markTitle,
	overrideCount,
	overrideOf,
	parseCell,
	parseKnob,
	parsePreset,
	valueMark,
	valueRows,
	valueTableKey,
	withCell,
	withoutOverrides,
	type ValueMark
} from './values';
import type { MarketView, RoomValueView, TempleCustom, TempleValueRow } from './slice';

/** A cell as Rust publishes one. Every field is overridable so a test states
 *  only the one it is about. */
function value(over: Partial<RoomValueView> = {}): RoomValueView {
	return {
		total: 846,
		priced: 'market',
		guessed: false,
		league: 'Allflame',
		asOf: 1_788_665_199_649,
		scaledFromTier3: null,
		drivers: [],
		...over
	};
}

/** One line's three tiers, tier 1 first, as `temple_value_table` returns them. */
function row(key: string, name: string, tiers: Partial<RoomValueView>[]): TempleValueRow {
	return {
		key,
		name,
		grade: 'A++',
		tiers: [value(tiers[0]), value(tiers[1]), value(tiers[2])]
	};
}

/** A live, fully priced market read, as the slice publishes one. */
function market(over: Partial<MarketView> = {}): MarketView {
	return {
		asOf: 1_788_665_199_649,
		stale: false,
		unavailable: false,
		...over
	};
}

/** The shipped rates and no overrides — `TempleCustomSettings::default`. */
function custom(over: Partial<TempleCustom> = {}): TempleCustom {
	return {
		tierFraction: 0.8,
		cPerQuantity: 0.5,
		cPerRarity: 0.25,
		dropsWeight: 1,
		comboPremium: 0,
		rooms: {},
		...over
	};
}

describe('valueMark', () => {
	it('marks a fully priced sum as measured', () => {
		expect(valueMark(value({ priced: 'market', guessed: false }))).toBe('M');
	});

	it('marks a fully priced sum that rests on an estimate as guessed', () => {
		// Same `priced`, opposite `guessed` — so the letter really is reading
		// both fields and not just the first.
		expect(valueMark(value({ priced: 'market', guessed: true }))).toBe('G');
	});

	it('marks an incomplete sum as partial even when nothing in it is guessed', () => {
		// P outranks G by design: an undercount is a worse thing to know about
		// a number than an estimate. Fails if the precedence is inverted.
		expect(valueMark(value({ priced: 'partial', guessed: false }))).toBe('P');
		expect(valueMark(value({ priced: 'partial', guessed: true }))).toBe('P');
	});

	it('marks a grade-ladder value as a fallback whichever way `guessed` reads', () => {
		// A cold rung reads guessed:false and a live one true (ADR-022 §3). A
		// cell that flipped F to G as the market came and went would be
		// describing the market, not the number.
		expect(valueMark(value({ priced: 'fallback', guessed: false }))).toBe('F');
		expect(valueMark(value({ priced: 'fallback', guessed: true }))).toBe('F');
	});

	it('marks an instrumental line as instrumental, not as a fallback', () => {
		// No price is MISSING for these two — summing is the wrong question —
		// so collapsing them into F would tell the player to go looking for a
		// price that was never owed.
		expect(valueMark(value({ priced: 'instrumental', guessed: true }))).toBe('I');
	});

	it("marks the player's own number as theirs, above every other provenance", () => {
		// An override replaces the formula outright, so nothing else about the
		// cell is news. Fails if `priced` is read in the wrong order.
		expect(valueMark(value({ priced: 'override', guessed: true }))).toBe('O');
	});
});

describe('markTitle', () => {
	it('states an estimate the letter could not carry', () => {
		// P won the letter, so `guessed` would otherwise be lost entirely.
		expect(markTitle(value({ priced: 'partial', guessed: true }))).toContain(
			'rests on an estimate'
		);
	});

	it('does not repeat the estimate when the letter already says it', () => {
		// `G`'s own label already reads "rests on an estimate", so an
		// unconditional second clause would print it twice in one title.
		const title = markTitle(value({ priced: 'market', guessed: true }));

		expect(title.split('rests on an estimate')).toHaveLength(2);
	});

	it('names the tier-3 total a scaled row came from', () => {
		expect(markTitle(value({ total: 676.8, scaledFromTier3: 846 }))).toContain('846');
	});

	it('names the league the prices came from', () => {
		expect(markTitle(value({ league: 'Mirage' }))).toContain('Mirage');
	});

	it('says nothing about a league on a cold read', () => {
		// `league` is "" with nothing priced; "priced in " would be a claim
		// about a market that was never read.
		expect(markTitle(value({ league: '' }))).not.toContain('priced in');
	});
});

describe('VALUE_MARK_LEGEND', () => {
	it('spells out every letter the table can print', () => {
		// The letters are the whole accessibility story — they are what a
		// colour cannot carry — so a mark with no legend entry is a mark
		// nobody can read. Fails if a `ValueMark` is added without one.
		const marks: ValueMark[] = ['M', 'G', 'P', 'F', 'I', 'O'];
		for (const mark of marks) expect(VALUE_MARK_LEGEND).toContain(`${mark} = `);
	});
});

describe('valueRows', () => {
	const rows = [
		row('corruption', 'Locus of Corruption', [
			{ total: 676.8, scaledFromTier3: 846 },
			{ total: 676.8, scaledFromTier3: 846 },
			{ total: 846 }
		]),
		row('gem', 'Doryani’s Institute', [
			{ total: 312, scaledFromTier3: 390, priced: 'partial' },
			{ total: 312, scaledFromTier3: 390, priced: 'partial' },
			{ total: 390, priced: 'partial' }
		])
	];

	it('derives one cell per tier for every line the table states', () => {
		const derived = valueRows(rows, null);

		expect(derived).toHaveLength(rows.length);
		expect(derived.map((r) => r.cells.map((c) => c.tier))).toEqual([
			[1, 2, 3],
			[1, 2, 3]
		]);
	});

	it('reads each cell off its own tier rather than off the row', () => {
		// The three cells carry three different numbers, so a projection that
		// indexed the wrong tier would show tier 1 at the tier-3 price.
		expect(valueRows(rows, null)[0].cells.map((c) => c.total)).toEqual([676.8, 676.8, 846]);
	});

	it('carries the line name and grade through for the row heading', () => {
		expect(valueRows(rows, null)[0]).toMatchObject({
			key: 'corruption',
			name: 'Locus of Corruption',
			grade: 'A++'
		});
	});

	it('marks each cell from its own provenance', () => {
		const derived = valueRows(rows, null);

		expect(derived[0].cells.map((c) => c.mark)).toEqual(['M', 'M', 'M']);
		expect(derived[1].cells.map((c) => c.mark)).toEqual(['P', 'P', 'P']);
	});

	it('reports no override under Default, which states none', () => {
		expect(valueRows(rows, null).flatMap((r) => r.cells.map((c) => c.override))).toEqual([
			null,
			null,
			null,
			null,
			null,
			null
		]);
	});

	it("reports the Custom table's own number on the cell that states one", () => {
		const table = custom({ rooms: { corruption: [null, null, 500] } });

		const derived = valueRows(rows, table);

		expect(derived[0].cells.map((c) => c.override)).toEqual([null, null, 500]);
		expect(derived[1].cells.map((c) => c.override)).toEqual([null, null, null]);
	});

	it('shows the total the preset produced, not the override, as the value', () => {
		// The two are the same number once Rust has been told, and different
		// while an edit is in flight. The cell must show what is IN FORCE —
		// a table that echoed the typed number would claim a write landed
		// before it did.
		const table = custom({ rooms: { corruption: [null, null, 12] } });

		expect(valueRows(rows, table)[0].cells[2].total).toBe(846);
	});
});

describe('overrideOf', () => {
	it('answers null for a line the table does not name', () => {
		expect(overrideOf(custom(), 'corruption', 3)).toBeNull();
	});

	it("answers the tier's own slot, tier 1 first", () => {
		const table = custom({ rooms: { corruption: [7, null, 500] } });

		expect(overrideOf(table, 'corruption', 1)).toBe(7);
		expect(overrideOf(table, 'corruption', 2)).toBeNull();
		expect(overrideOf(table, 'corruption', 3)).toBe(500);
	});

	it('answers 0 rather than null for a room priced at nothing', () => {
		// "This room is worth nothing to me" is a position a rusher holds, and
		// `?? null` on a stored 0 would silently put it back on the formula.
		expect(overrideOf(custom({ rooms: { gem: [null, null, 0] } }), 'gem', 3)).toBe(0);
	});
});

describe('parseCell', () => {
	it('reads an empty box as "use the formula", not as zero', () => {
		// `Number('')` is 0, which is exactly how the two would collapse.
		expect(parseCell('')).toEqual({ kind: 'clear' });
		expect(parseCell('   ')).toEqual({ kind: 'clear' });
	});

	it('reads a typed zero as the position it is', () => {
		expect(parseCell('0')).toEqual({ kind: 'value', chaos: 0 });
	});

	it('reads a chaos amount', () => {
		expect(parseCell('12.5')).toEqual({ kind: 'value', chaos: 12.5 });
	});

	it('refuses text rather than storing NaN', () => {
		// One NaN in the table makes the whole ranking's float ordering
		// arbitrary, and `settings.json` cannot even represent it.
		expect(parseCell('quite a lot')).toEqual({ kind: 'invalid', reason: 'not a number' });
	});

	it('refuses a negative amount, which is not a chaos value', () => {
		expect(parseCell('-5')).toEqual({ kind: 'invalid', reason: 'not a chaos amount' });
	});

	it('refuses an infinity', () => {
		expect(parseCell('Infinity').kind).toBe('invalid');
	});
});

describe('parseKnob', () => {
	const dropsWeight = KNOBS.find((k) => k.field === 'dropsWeight')!;
	const tierFraction = KNOBS.find((k) => k.field === 'tierFraction')!;

	it('reads the rusher’s zero drops weight', () => {
		expect(parseKnob('0', dropsWeight)).toEqual({ kind: 'value', chaos: 0 });
	});

	it('refuses an empty rate instead of clearing it', () => {
		// A rate has no "use the formula" state — the formula IS the rate — so
		// a blank box is a half-typed number and not an instruction.
		expect(parseKnob('', dropsWeight).kind).toBe('invalid');
	});

	it('refuses a tier fraction above 1 rather than letting Rust clamp it', () => {
		// `valuation::tier_fraction` caps at 1, so a stored 2.5 would show a
		// number the app never applies.
		expect(parseKnob('2.5', tierFraction)).toEqual({ kind: 'invalid', reason: 'at most 1' });
	});

	it('accepts the fraction’s own ceiling', () => {
		expect(parseKnob('1', tierFraction)).toEqual({ kind: 'value', chaos: 1 });
	});

	it('does not cap a knob that has no ceiling', () => {
		expect(parseKnob('2.5', dropsWeight)).toEqual({ kind: 'value', chaos: 2.5 });
	});
});

describe('withCell', () => {
	it('states a number for a line the table had never named', () => {
		const next = withCell(custom(), 'corruption', 3, 500);

		expect(next.rooms).toEqual({ corruption: [null, null, 500] });
	});

	it('leaves the other two tiers of the row alone', () => {
		const table = custom({ rooms: { corruption: [7, null, 500] } });

		expect(withCell(table, 'corruption', 2, 9).rooms.corruption).toEqual([7, 9, 500]);
	});

	it('leaves the rates alone', () => {
		// The cell editor and the rate editor are separate controls; a cell
		// write that reset the rusher's drops weight would undo his one
		// setting behind his back.
		const table = custom({ dropsWeight: 0 });

		expect(withCell(table, 'gem', 3, 1).dropsWeight).toBe(0);
	});

	it('does not mutate the table it was given', () => {
		// The caller's copy is the slice's own echo; mutating it would show a
		// value the command has not accepted yet.
		const table = custom({ rooms: { corruption: [null, null, 500] } });

		withCell(table, 'corruption', 3, 1);

		expect(table.rooms.corruption).toEqual([null, null, 500]);
	});

	it('drops the row entirely once its last tier is cleared', () => {
		// Rust ignores an all-null row, so keeping it would write a line
		// claiming an opinion the player has just withdrawn.
		const table = custom({ rooms: { corruption: [null, null, 500] } });

		expect(withCell(table, 'corruption', 3, null).rooms).toEqual({});
	});

	it('keeps the row when clearing one tier of several', () => {
		const table = custom({ rooms: { corruption: [7, null, 500] } });

		expect(withCell(table, 'corruption', 3, null).rooms).toEqual({ corruption: [7, null, null] });
	});
});

describe('copyValuesInto', () => {
	const rows = [
		row('corruption', 'Locus of Corruption', [{ total: 676.8 }, { total: 676.8 }, { total: 846 }])
	];

	it('states every tier of every line the table valued', () => {
		expect(copyValuesInto(custom(), rows).rooms).toEqual({ corruption: [676.8, 676.8, 846] });
	});

	it('leaves the rates alone', () => {
		// The button copies VALUES. Resetting the drops weight with them would
		// undo the one setting a rusher came for.
		expect(copyValuesInto(custom({ dropsWeight: 0 }), rows).dropsWeight).toBe(0);
	});

	it('replaces whatever the table said before, rather than merging', () => {
		const table = custom({ rooms: { gem: [null, null, 1] } });

		expect(copyValuesInto(table, rows).rooms.gem).toBeUndefined();
	});
});

describe('withoutOverrides', () => {
	it('puts every room back on the formula', () => {
		const table = custom({ rooms: { corruption: [null, null, 500] } });

		expect(withoutOverrides(table).rooms).toEqual({});
	});

	it('leaves the rates alone', () => {
		expect(withoutOverrides(custom({ dropsWeight: 0 })).dropsWeight).toBe(0);
	});
});

describe('overrideCount', () => {
	it('counts stated tiers, not rows', () => {
		const table = custom({ rooms: { corruption: [7, null, 500], gem: [null, null, 0] } });

		expect(overrideCount(table)).toBe(3);
	});

	it('counts a zero, which is a stated number', () => {
		expect(overrideCount(custom({ rooms: { gem: [null, null, 0] } }))).toBe(1);
	});

	it('counts nothing on a table that states nothing', () => {
		expect(overrideCount(custom())).toBe(0);
	});
});

describe('formatChaos', () => {
	it('keeps two decimals on a small value, where they are the difference', () => {
		expect(formatChaos(6.0)).toBe('6.00');
	});

	it('rounds a three-figure value, where they are noise', () => {
		expect(formatChaos(676.8)).toBe('677');
	});
});

describe('TIERS', () => {
	it('is the three tiers a room line has, tier 1 first', () => {
		expect(TIERS).toEqual([1, 2, 3]);
	});
});

describe('parsePreset', () => {
	it('narrows the two wire strings the picker can report', () => {
		expect(parsePreset('default')).toBe('default');
		expect(parsePreset('custom')).toBe('custom');
	});

	it('refuses anything else rather than handing Rust a third variant', () => {
		// The dropped Basic preset is the concrete case: a stale option in the
		// list would otherwise reach a Rust enum that has no branch for it.
		expect(parsePreset('basic')).toBeNull();
		expect(parsePreset('Custom')).toBeNull();
	});
});

describe('valueTableKey', () => {
	it('holds still across a poll that changed nothing', () => {
		// The key's whole job. The slice is whole-replaced every three
		// seconds, so a key that moved with the poll would re-fetch 75 values
		// on a timer.
		expect(valueTableKey('custom', market(), custom())).toBe(
			valueTableKey('custom', market(), custom())
		);
	});

	it('moves when a read that kept its timestamp stopped pricing anything', () => {
		// The stale transition: `asOf` is published on a stale read as well as
		// a live one, so nothing but `unavailable` says that every room just
		// dropped onto the base ladder. Fails if the flag leaves the key —
		// the board would re-value while the editor kept showing the market
		// numbers and their letters.
		const priced = valueTableKey('custom', market(), custom());
		const onBaseValues = valueTableKey('custom', market({ unavailable: true }), custom());

		expect(onBaseValues).not.toBe(priced);
	});

	it('moves when an already-unpriced read crosses two hours', () => {
		// The one way `stale` rises without `unavailable` rising with it: a
		// read the floor made unusable, ageing past the window afterwards.
		const fresh = valueTableKey('custom', market({ unavailable: true }), custom());
		const aged = valueTableKey('custom', market({ unavailable: true, stale: true }), custom());

		expect(aged).not.toBe(fresh);
	});

	it('moves when the server observed new prices', () => {
		const before = valueTableKey('custom', market(), custom());
		const after = valueTableKey('custom', market({ asOf: 1_788_665_299_649 }), custom());

		expect(after).not.toBe(before);
	});

	it('moves when the preset in force changes', () => {
		expect(valueTableKey('custom', market(), custom())).not.toBe(
			valueTableKey('default', market(), custom())
		);
	});

	it('moves when the Custom table states a different number', () => {
		// Under Default the table changes no value on screen, but the fetch is
		// keyed on it either way: the alternative is a key that has to know
		// which preset reads which field.
		const before = valueTableKey('custom', market(), custom());
		const after = valueTableKey(
			'custom',
			market(),
			custom({ rooms: { corruption: [null, null, 1234] } })
		);

		expect(after).not.toBe(before);
	});

	it('moves when a rate changes, not only when a room does', () => {
		// A rate re-prices every room the table does not name, which is most
		// of the board. Fails if the key digests `rooms` alone.
		const before = valueTableKey('custom', market(), custom());
		const after = valueTableKey('custom', market(), custom({ dropsWeight: 0 }));

		expect(after).not.toBe(before);
	});
});

describe('PRESET_OPTIONS', () => {
	it('offers the two presets and no third', () => {
		expect(PRESET_OPTIONS.map((o) => o.value)).toEqual(['default', 'custom']);
	});

	it('gives each preset a line saying where its numbers come from', () => {
		for (const option of PRESET_OPTIONS) expect(PRESET_NOTE[option.value].length).toBeGreaterThan(0);
	});
});

describe('KNOBS', () => {
	it('names all five rates the Custom preset carries', () => {
		// One missing spec is one rate with no control, which is a setting the
		// player can only reach by hand-editing settings.json.
		expect(KNOBS.map((k) => k.field)).toEqual([
			'tierFraction',
			'cPerQuantity',
			'cPerRarity',
			'dropsWeight',
			'comboPremium'
		]);
	});

	it('gives every rate a unit hint, because none of them is self-evident', () => {
		for (const knob of KNOBS) expect(knob.hint.length).toBeGreaterThan(0);
	});
});
