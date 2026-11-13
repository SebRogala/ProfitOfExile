# Lab reader fixtures (POE-397)

## `font-gems-1080p.png`

A real Divine Font gem-choice screen, 1920×1080 at ui_scale 0.90 (the POE-397
corpus frame `screenshot-0012`, 2026-11-13). Its three slots hold Blade Vortex
of the Scythe, Reave of Refraction and Blade Flurry of Incision: a transfigured
set. `lab::gem_icons` tests read it as is, upscaled to 4K, and pasted at its
16:10 place. A screenshot of a running game, so it is committed (unlike the
icon art in `../lab-gem-art/`).

## `gem-sets.json`

Names only: `{normal, transfigured, iconless}`.

- `transfigured`: `GET /api/analysis/gems/dictionary?transfigured=true`.
- `normal`: `GET /api/analysis/gems/dictionary?transfigured=false` minus the
  `transfigured` names (that half carries 52 transfigured names too), the same
  partition `lab::commands::fetch_gem_dictionary` makes.
- `iconless`: the names of either half that `GET /api/icon/gems/{name}`
  answers 404 for. They are excluded by design: 47 `Vaal <Base> (<Transfigured>)`
  names, 7 normal Vaal names and 33 other normal names (unique-granted or
  retired gems).

Snapshot of prod (`https://profitofexile.top`) on **2026-11-13**: normal 378,
transfigured 249, iconless 87 (47 of them transfigured); eligible
378 + 249 − 87 = **540** (normal 338, transfigured 202). Every icon request
answered 200 or 404; none failed in transport.

### Refreshing it

When `make lab-gem-art` fails on an eligible name or a league moves the
dictionary: fetch both halves again, request every name's icon, rebuild the
three lists the same way, update the date and counts above, and rerun
`cargo test --lib lab::gem_icon_refs`.
