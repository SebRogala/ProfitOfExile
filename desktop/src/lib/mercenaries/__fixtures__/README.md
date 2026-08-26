# Mercenary saved-search fixtures

Committed ground truth for `lib/mercenaries/rulesets.ts`. The typed rulesets are a
transcription of these files; `rulesets.test.ts` asserts the transcription against
them, so a typo in the data module fails against raw GGG JSON rather than against
itself.

**Decoded:** 2026-08-17. **League:** `Allflame`.

Every file here except `capture-query.expected.json` is a verbatim copy of an API
response body — no reformatting, no pretty-printing, no key reordering. Re-fetch output
is byte-comparable with the committed file. `capture-query.expected.json` is the one the
app BUILDS rather than fetches; see "Captured-mercenary query parity" below.

## Saved searches

Each `<hash>.json` is the body of `GET /api/trade/search/Allflame/<hash>` and carries
its own hash in the top-level `id` field. The human-facing page for the same search is
`https://www.pathofexile.com/trade/search/Allflame/<hash>`.

| File | Source | Ruleset |
|---|---|---|
| `WvKGjV8Kfm.json` | guide-a | Manyshot |
| `LgkKKmllTn.json` | guide-a | Kinetist v1 |
| `5nd22GvKCa.json` | guide-a | Combatant |
| `7nRvBzl2S5.json` | guide-b | Kinetist ladder — MV |
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

**Decoded:** the four Manyshot files and the nine Combatant files, 2026-08-26. The four
Manyshot files are the only ones here that carry no `filters` block at all — no
item-level floor — so `rulesets.ts` leaves `ilvlMin` absent on those rungs and the
fidelity test reads the key as optional. The nine Combatant files set `ilvl` 83 like
everything else.

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

Each video's description is the only place its trade links exist; the rung-level
`guideUrl` in `rulesets.ts` names which video published which link. It is one video
per LADDER for the first two and one video for BOTH Combatant ladders, so the guide
URL cannot be derived from the ladder key.

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
