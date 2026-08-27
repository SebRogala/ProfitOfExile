# Merc seed art — fetched, not committed

The PNGs this directory holds at test time are **player support-gem inventory
icons**: GGG's art, published on the PoE Wiki under CC-BY-NC-SA. They are
deliberately **not in git**, and `.gitignore` keeps them out.

## Why not, when `../merc-icon-crops` is committed

The corpus crops next door are 39×39 fragments cut out of a screenshot of a
running game — evidence of what a cell looked like. These are the complete,
reusable icons. This repository is GPL-3.0 (see `LICENSE`, relicensed for the
LabCompass room SVGs), and GPL-3.0 and CC-BY-NC-SA are not compatible:
committing whole icons here would purport to relicense art nobody involved may
relicense. The same rule already applies to `data/gem-icons-cache/` and
`gem-icons-cache/` — this is that rule reaching one more directory.

Ruling taken 2026-08-27 by the POE-208 orchestrator; reversible, and Sebastian
may override it.

## Getting them

```sh
make merc-seed-art                                        # local dev server
make merc-seed-art POE_SERVER_URL=https://profitofexile.top  # prod cache
```

The target reads every `gem` in `desktop/src-tauri/src/mercenary/seed-map.json`
and fetches it from `$POE_SERVER_URL/api/gem-icon/<name>` — the same route the
desktop app itself uses (ADR-012), so what the tests measure is what the app
will cache. Existing files are skipped, so it is cheap to re-run and safe to
interrupt. It prints how many were fetched, skipped and failed, and exits
non-zero if any failed.

A local dev server fetches each icon from the wiki on first request (10 s
timeout) and caches it, so the first run is slow and a later run is instant.
Production already holds all 223 support icons.

## When they are missing

Every test that needs the art fails with a message naming `make merc-seed-art`.
None of them skip: a silently-skipped calibration test would let the three
constants in `mercenary/seed.rs` drift with nothing to catch it.

## What is here

One file per `gem`, named by its slug (lowercase, every non-alphanumeric
character becomes `-`): `Added Chaos Damage Support` →
`added-chaos-damage-support.png`. 51 files, one per map row.

Square RGBA, 78×78 for all but two observed exceptions on 2026-08-27:
`sacred-wisps-support.png` is 80×80, and `trap-and-mine-damage-support.png` has
no alpha channel. Both belong to `name`-graded rows, so neither reaches a
shipped seed; `mercenary::seed::tests::art` pins the rule that actually
matters — square, and inside the inventory-icon size range.
