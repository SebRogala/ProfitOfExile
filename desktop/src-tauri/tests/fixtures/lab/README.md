# Lab reader fixtures (POE-397)

## `font-gems-1080p.png`

A real Divine Font gem-choice screen, 1920×1080 at ui_scale 0.90 (the POE-397
corpus frame `screenshot-0012`, 2026-11-13). Its three slots hold Blade Vortex
of the Scythe, Reave of Refraction and Blade Flurry of Incision: a transfigured
set. `lab::gem_icons` tests read it as is, upscaled to 4K, and pasted at its
16:10 place. A screenshot of a running game, so it is committed (unlike the
icon art in `../lab-gem-art/`).

## `font-slots-detonate-dead-chain-reaction.png`

Real Divine Font slot row from the Operator's Discord bug report (2026-11-21),
saved with the POE-455 patch. The tooltips named Toxic Rain of Sporeburst,
Fire Trap of Blasting and Detonate Dead of Chain Reaction, left to right.
The regression test requires all three exact names with certainty.

The missing-reference and washed-sibling tests extend this frame: a missing
reference or a washed sibling reads uncertain (held as its flagged best
candidate), never certain as another gem. The ignored Rust calibration report
`gem_icon_pair_rmse_report` also measures the 1080p/4K/16:10 frames and 13
sibling pairs under gold/white washes, offsets and scales. Run it with
`cargo test --lib gem_icon_pair_rmse_report -- --ignored --nocapture`. The
certainty margin and its measured derivation live in `lab::gem_icons`.

## `font-gems-cruel-1080p-crop.png`

The Divine Font window during a Cruel-lab gem pick (POE-503), cropped from an
Operator's 1920×1080 JPEG capture of 2026-11-26. The rect is x 282, y 138,
w 690, h 536: the whole Font window, title ornament to below CRAFT, plus a few
px. The JPEG was decoded by the `image` crate and the rect saved as PNG without
re-encode, so the PNG holds exactly the decoded JPEG pixels (other decoders
differ by a level or two). At this difficulty the window behind the gem popup
is taller than in `font-gems-1080p.png`: its title sits 26 px higher and its
CRAFT button lower, while the popup and CONFIRM stay within 1 px (CONFIRM
matches 1 px higher). `lab::panel_probe`'s regression test pastes the crop
back at (282, 138) on a black 1080p frame.

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
