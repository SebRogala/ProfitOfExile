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

The floor of 2 is what puts `sup_chain` in row 1's group: loosening adds the family's ids
at every tier from the floor up to, but not including, the tier read.

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
