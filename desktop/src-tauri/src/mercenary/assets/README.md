# Merc module assets

Files committed here are `include_str!`/`include_bytes!`-embedded by the module —
they ship inside the binary, so a change needs a rebuild.

| File | Provenance |
|---|---|
| `merc-voicelines.tsv` | Extracted from the owner-saved poedb.tw mercenary dialogue table on 2026-09-14: 2,602 source rows reduced to 1,058 unique normalized texts. `inspect` marks a text when any poedb row with that text is `Inspect` or `InspectNotable` (89 inspect texts); `line` is everything else. A GGG patch adding or rewording lines requires regenerating this embedded asset; unknown texts have no fallback and do not arm (owner decision 2026-09-14). |
