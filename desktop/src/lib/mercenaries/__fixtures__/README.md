# Mercenary saved-search fixtures

Committed ground truth for `lib/mercenaries/rulesets.ts`. The typed rulesets are a
transcription of these files; `rulesets.test.ts` asserts the transcription against
them, so a typo in the data module fails against raw GGG JSON rather than against
itself — for the 21 GGG-saved files. The four authored `guide-c-*.json` files are the
builder's own output, and what that check is worth for them is stated under "Authored
queries (guide-c)".

**Decoded:** 2026-08-17. **League:** `Allflame`.

Two kinds of file here are NOT GGG's, and neither is ground truth in the same sense:
`capture-query.expected.json` is the one the app BUILDS rather than fetches (see
"Captured-mercenary query parity"), and the four `guide-c-*.json` files are queries this
app AUTHORED from a guide's prose (see "Authored queries (guide-c)"). Every other file is
a verbatim copy of an API response body — no reformatting, no pretty-printing, no key
reordering. Re-fetch output is byte-comparable with the committed file.

## Saved searches

Each `<hash>.json` is the body of `GET /api/trade/search/Allflame/<hash>` and carries
its own hash in the top-level `id` field. The human-facing page for the same search is
`https://www.pathofexile.com/trade/search/Allflame/<hash>`.

| File | Source | Ruleset |
|---|---|---|
| `WvKGjV8Kfm.json` | guide-a | Manyshot |
| `LgkKKmllTn.json` | guide-a | Kinetist v1 |
| `5nd22GvKCa.json` | guide-a | Combatant |
| `7nRvBzl2S5.json` | guide-b, guide-d | Kinetist ladder — MV / 20D |
| `BgzkZKGQF8.json` | guide-b | Kinetist ladder — Mid |
| `LgkGrPO5Fn.json` | guide-b | Kinetist ladder — End |
| `zbrQyEqah4.json` | guide-b | Kinetist ladder — GG |
| `4mP3V2jQT9.json` | guide-b | Manyshot ladder — Early |
| `Z6Em09GmHQ.json` | guide-b | Manyshot ladder — Mid |
| `JBnK2YKRFl.json` | guide-b | Manyshot ladder — End |
| `d86ymvXRsJ.json` | guide-b | Manyshot ladder — GG |
| `Kld4gv0Pi5.json` | guide-b | Frost Blades ladder — Minimum |
| `Kld4gM7yi5.json` | guide-b | Frost Blades ladder — Midgame |
| `q9l6yK0psg.json` | guide-b | Frost Blades ladder — Endgame (no return) |
| `OglBJZoQIE.json` | guide-b | Frost Blades ladder — Endgame (return) |
| `PPaX7lLqUL.json` | guide-b | Frost Blades ladder — GG Merc |
| `3q6awYZPc5.json` | guide-b | Wild Strike ladder — Minimum |
| `mkgR2DbeS6.json` | guide-b | Wild Strike ladder — Midgame |
| `jWRDpypkCX.json` | guide-b | Wild Strike ladder — Endgame |
| `bGDrZYZaCL.json` | guide-b | Wild Strike ladder — GG Merc |
| `4mKr0Jbwh9.json` | guide-d | Kinetist ladder — budget |

**Decoded:** the four Manyshot files and the nine Combatant files, 2026-08-26;
`4mKr0Jbwh9.json`, 2026-08-28. The four Manyshot files are the only ones here that carry
no `filters` block at all — no item-level floor — so `rulesets.ts` leaves `ilvlMin`
absent on those rungs and the fidelity test reads the key as optional. The nine
Combatant files set `ilvl` 83 like everything else, and so does `4mKr0Jbwh9.json`.

**One file, two rulesets.** `7nRvBzl2S5.json` is the oracle of BOTH
`guide-b-kinetist-mv` and `guide-d-kinetist-20d`: XTheFarmerX's sheet republishes
Nerotox's own Kinetist MV link as his "20D KB Merc" rung, so the two sources transcribe
one saved search and there is nothing to commit twice. A fixture belongs to a RULESET,
not to a source, and `rulesets.test.ts` names this pair rather than leaving the sharing
implied — anything else sharing an oracle is a ruleset pointed at the wrong file.

**`4mKr0Jbwh9` is a TYPED hash.** It is not in XTheFarmerX's linked sheet — Sebastian
read it off the video's own trade tab (Merc Skills / Trade Filters chapters, 9:48–18:56)
and re-fetched it. What the sheet publishes instead is a sibling, "Cheap Starter KB Merc"
under `G6PdveWBib`, and the two differ only in the last min-2 damage group:
`G6PdveWBib` lacks Faster Attacks (Tier 2) `mercenary.support_987` and Greater Faster
Attacks (Tier 3) `mercenary.support_50485`. The typed hash therefore resolves to a
coherent KB search of its own rather than to a mistyped sibling. `G6PdveWBib` is NOT
committed: no ruleset transcribes it, and a fixture no test reads is a file that rots.

### Guide URLs

`rulesets.ts` records these; they are repeated here because a fixture is only
re-fetchable if you can find the search it came from again.

- guide-a — one page, all three searches:
  <https://wealthyexile.com/strategies/7062/alchgo_astrolabe__merc_boss_rushing>
- guide-b — Nerotox's channel, <https://www.youtube.com/channel/UCqIRIXItoDOlET2oeFn6WKA>,
  one video per ladder, except the Combatant video which publishes two:
  - Kinetist ladder (2026-08-08): <https://www.youtube.com/watch?v=HKTVN4sENvg>
  - Manyshot ladder (2026-07-29): <https://www.youtube.com/watch?v=ljaXlGLdyxM>
  - Frost Blades AND Wild Strike ladders, one video (2026-08-08):
    <https://www.youtube.com/watch?v=45aM9242Umo>
- guide-c — one page, no searches at all:
  <https://mobalytics.gg/poe/builds/captainlance9-luminary-merc-bot>, section
  "Ideal Merc Options". See "Authored queries (guide-c)" below.
- guide-d — one video, both searches: XTheFarmerX, "5 DIVINE BUDGET LIFE STACKING KB
  MERC BUILD | Trade Links - Crafting - Merc Warrants" (2026-08-14, league Allflame),
  <https://www.youtube.com/watch?v=LXoJCRmUaJI>, channel
  <https://www.youtube.com/@XTheFarmerX_POE2>. Its description links a Google Sheet of
  Better-Trading folders, which is where the 20D hash comes from:
  <https://docs.google.com/spreadsheets/d/1c-9qyowK9jp8OIR0bwh8G0V3qjY8U6lEDAxA6xOUMdU/edit?gid=586502310>.
  The budget hash is not in that sheet — see "One file, two rulesets" and
  "`4mKr0Jbwh9` is a TYPED hash" above.

Each guide-b video's description is the only place its trade links exist; the rung-level
`guideUrl` in `rulesets.ts` names which of those videos published which link. It is one
video per LADDER for the first two and one video for BOTH Combatant ladders, so the
guide URL cannot be derived from the ladder key. Guide-d needs none of that: one video
publishes both its rungs, so they inherit the source URL the way guide-a's and guide-c's
rulesets do.

The Combatant description carries one prose note, about all nine searches rather than
any one of them, so no rung takes it as an `authorNote`: "Please play around yourself
with the trade filters as well to search for greater supports, these are only starting
points, you can definitely optimize the searches for whatever you are looking for still
(moveskill/auras)".

Re-fetch one (read-only GET; pace repeated calls at least 2s apart to stay inside GGG's
rate limits):

```sh
curl -H 'User-Agent: ProfitOfExile/0.7 (contact: sebrogala@gmail.com)' \
  https://www.pathofexile.com/api/trade/search/Allflame/WvKGjV8Kfm
```

A saved search is the only oracle for its own contents: to verify a fixture, re-fetch it
by hash and diff. Note that a saved search is mutable at its author's end — a diff means
the search changed upstream, not necessarily that the fixture was wrong when captured.

## Authored queries (guide-c)

`guide-c-kinetist.json`, `guide-c-manyshot.json`, `guide-c-blade-ambusher.json` and
`guide-c-combatant.json` are **not** GGG responses and are **not re-fetchable**. They are
THIS APP'S transcription of CaptainLance9's "Ideal Merc Options" section, which names
skills and support links in sentences and publishes no trade link for any of them.

They are written in the same body shape a saved search comes back in — `{"id": …,
"query": {…}}`, with the `id` naming the file rather than a hash and no `filters` block,
because the prose sets no item-level floor — so `rulesets.test.ts` reads them through the
same `fromFixture` reader as the other twenty-one and the fidelity sweep covers all
twenty-six rulesets.

**What that check is worth, and what it is not.** It fails on a typed-model edit nobody
meant to make. It cannot fail on a MISREADING of the guide: both sides of the comparison
come from this repository, so if `rulesets.ts` asks for the wrong stat id the fixture
asks for it too. The only oracle for the reading is the page itself, so each guide-c
group in `rulesets.ts` carries the sentence it was transcribed from — re-check against
those. The live page 403s to bots; a Wayback snapshot dated 2026-07-28 exists, and the
verbatim prose was pasted by the owner 2026-08-26.

Regenerate after a deliberate data-model change rather than hand-editing — the builder's
output IS the file:

```ts
// a throwaway vitest file anywhere under desktop/src
import { writeFileSync } from 'node:fs';
import { it } from 'vitest';
import { allRulesets } from '$lib/mercenaries/rulesets';
import { rulesetQuery } from '$lib/mercenaries/trade-links';

it('rewrites the authored fixtures', () => {
	for (const ruleset of allRulesets()) {
		if (ruleset.authored === undefined) continue;
		writeFileSync(
			`src/lib/mercenaries/__fixtures__/${ruleset.authored.file}.json`,
			JSON.stringify({ id: ruleset.authored.file, query: rulesetQuery(ruleset) })
		);
	}
});
```

Note what regenerating does NOT do: it makes the fidelity test green by construction, so
it is only ever correct after a change that was checked against the prose by a human.

## Captured-mercenary query parity

`capture-query.expected.json` is not a GGG response — it is the `query` object Rust's
`mercenary/search.rs::build_capture_query` produces for one fixed capture, committed so
both languages assert against one artifact instead of two literals that can drift apart.

**The capture it was built from** (`search.rs`'s `parity_capture()` test fixture, with
that module's `chain_vocab()` and a tier floor of **2**):

| Row | Skill | Support cells |
|---|---|---|
| 0 | `skill_a` | `sup_a`; `sup_b1` + `sup_b2` (one cell the icon read could not narrow) |
| 1 | `skill_b` | `sup_greater_chain`, family `Chain` tier 3, hover-confirmed |

The floor of 2 is what adds `sup_chain` to row 1's cell: loosening adds the family's ids
at every tier from the floor up to, but not including, the tier read.

**The shape the rows lower to** (amended 2026-08-26; the file previously carried one
`mercenary` group per row, which GGG answers with 400 "Query is too complex" — a query
may hold ONE `mercenary` group, and a captured panel is four or five rows):

- one `and` group with every cell that names exactly one support — `skill_a`, `sup_a`,
  `skill_b`, in read order, rows folded together;
- one `count` group of `value.min = 1` per cell that names several — `sup_b1`/`sup_b2`
  (the unnarrowed read) and `sup_greater_chain`/`sup_chain` (the loosened one).

Nothing in the query says which row a support sat on. That is deliberate and costs a
known false-positive class; `mercenary/search.rs`'s head doc carries the probe numbers.

Who reads it:

- `search.rs::the_link_carries_the_shared_fixture_query_under_a_bare_query_envelope` —
  builds the capture, links it with `capture_url`, and asserts the decoded `q` is
  `{"query": <this file>}`.
- `trade-links.test.ts` — types the file as `TradeQuery`, sends it through
  `derivedSearchUrl`, and asserts the same round trip.

To regenerate after a deliberate query-shape change, print the builder's output rather
than hand-editing the file:

```rust
// a throwaway #[test] in mercenary/search.rs
let q = build_capture_query(&parity_capture(), &chain_vocab(), 2).unwrap();
println!("{}", serde_json::to_string_pretty(q.body.get("query").unwrap()).unwrap());
```

## Stat vocabulary

`mercenary-stats.json` is the `Mercenary`-labelled element of GGG's stat vocabulary — the
one object out of `GET /api/trade/data/stats`, with its `entries` verbatim. It maps every
`mercenary.skill_*` / `mercenary.support_*` id to its display text, including the
`(Tier N)` suffix that support links carry. Entry `text` is display-critical and is NOT a
key: `Gilded Extra Targets (Tier 3)` exists under two different ids. Names in
`rulesets.ts` are copied from here and asserted against it — never hand-typed.

```sh
curl -H 'User-Agent: ProfitOfExile/0.7 (contact: sebrogala@gmail.com)' \
  https://www.pathofexile.com/api/trade/data/stats \
  | jq -c '.result[] | select(.label=="Mercenary")' > mercenary-stats.json
```
