# Lab gem art — fetched, not committed

The PNGs this directory holds at test time are the reference icons the lab gem
reader (`lab::gem_icons`, POE-397) matches the Divine Font's three gem slots
against: player skill-gem inventory icons, GGG's art, published on the PoE Wiki
under CC-BY-NC-SA. They are deliberately **not in git**, and `.gitignore` keeps
them out.

## Why not

The same reason as [`../merc-seed-art/README.md`](../merc-seed-art/README.md):
this repository is GPL-3.0 and CC-BY-NC-SA is not compatible with it, so
committing whole icons would purport to relicense art nobody involved may
relicense. The committed frame next door (`../lab/font-gems-1080p.png`) is a
screenshot of a running game, evidence of what the screen looked like, like
`../merc-icon-crops`.

## Getting them

```sh
make lab-gem-art                                          # local dev server
make lab-gem-art POE_SERVER_URL=https://profitofexile.top # prod cache
```

`make merc-seed-art` runs it first, so CI's "Fetch merc seed art" step fetches
both. The target reads `../lab/gem-sets.json` and fetches every **eligible**
name — `normal` + `transfigured` minus `iconless` — from
`$POE_SERVER_URL/api/icon/gems/<name>` (the route the app uses, ADR-012).
`iconless` names are never requested. Existing files are skipped. Any non-200
for an eligible name, 404 included, fails the target with the name: either the
server lost an icon or the dictionary moved, and the fix is a refreshed
`gem-sets.json` (see `../lab/README.md`). From empty against prod it took
97 s on 2026-11-13 (540 serial requests).

## When they are missing

`lab::gem_icon_refs::tests::every_eligible_reference_decodes` fails naming the
missing names and `make lab-gem-art`; every icon test in `lab::gem_icons` and
`lab::gem_reader` that needs the references fails with it. None of them skip.

## What is here

One file per eligible name, named by its slug (lowercase, every non-ASCII-
alphanumeric character becomes `-`): `Reave of Refraction` →
`reave-of-refraction.png`. 540 files on 2026-11-13, 78×78 RGBA.
