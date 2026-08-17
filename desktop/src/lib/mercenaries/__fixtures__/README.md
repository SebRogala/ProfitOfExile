# Mercenary saved-search fixtures

Committed ground truth for `lib/mercenaries/rulesets.ts`. The typed rulesets are a
transcription of these files; `rulesets.test.ts` asserts the transcription against
them, so a typo in the data module fails against raw GGG JSON rather than against
itself.

**Decoded:** 2026-08-17. **League:** `Allflame`.

Everything here is a verbatim copy of an API response body — no reformatting, no
pretty-printing, no key reordering. Re-fetch output is byte-comparable with the
committed file.

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
