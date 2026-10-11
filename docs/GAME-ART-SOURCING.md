# Game Art Sourcing

Status: current. Last verified: 2026-10-11. This is the procedure for finding
and extracting Path of Exile art for static UI assets that ship with the
desktop app, for example sidebar nav icons in `desktop/static/`. Runtime item
icons served by the server (`/api/icon/...`) follow [Icons](ICONS.md) instead.

## Sources, in the order to try them

| Source | What it has | How to address it |
| --- | --- | --- |
| GGG CDN, by art path | Every inventory item's art (`Art/2DItems/...`) | `https://web.poecdn.com/image/<ArtPath>.png` |
| RePoE `base_items.json` | Item name → art path (`visual_identity.dds_file`) | `https://repoe-fork.github.io/base_items.json` |
| `grindinggear/atlastree-export` | Atlas passive and mastery icons, as sprite sheets with coords | `data.json` → `sprites.<kind>.<zoom>.{filename,coords}` |
| ggpk.exposed | Any file in the game's bundles: minimap, quest icons, UI art, `.dat` tables | raw: `https://ggpk.exposed/<path>`; DDS→PNG: `https://image.ggpk.exposed/<path>` |
| poe.ninja | Cross-check: the live icon URL for anything it prices | `https://poe.ninja/poe1/api/economy/...` (see `internal/collector/ninja.go`) |

The art can come out of the CDN or the GGPK at higher quality than a screenshot,
so a screenshot is only useful for recognising which icon you want.

poewiki.net sits behind a bot wall (Anubis) and returns an HTML challenge to
`curl` and to its API, so it cannot be scripted.

## Item art (CDN)

1. Find the art path. Search `base_items.json` by item name. The `dds_file`
   value maps directly: `Art/2DItems/Maps/TempleMap.dds` →
   `https://web.poecdn.com/image/Art/2DItems/Maps/TempleMap.png`.
2. Download it. The CDN serves transparent RGBA PNGs, most at 78×78.

The CDN serves current art. Currency was redrawn at some point: the Divine Orb
is a golden face, not the old orb. Check the CDN output against poe.ninja
rather than against memory.

The CDN only covers `Art/2DItems`. Atlas tree, minimap and other UI art return
404 there, so use the two sources below.

## Atlas tree icons (sprite sheets)

`data.json` from `https://raw.githubusercontent.com/grindinggear/atlastree-export/master/data.json`
lists sprite sheets per kind (`mastery`, `normalActive`, `notableActive`,
`masteryOverlay`, ...) and per zoom level. Each entry has the sheet `filename`
(a `web.poecdn.com/image/passive-skill/...` URL) and `coords`, keyed by art
path, of `{x, y, w, h}`. To get an icon, crop the highest zoom level's sheet to
those coords.

The mastery icons sit on a blue disc and have thin gold strokes, which
disappear at sidebar size.

## Any game file (ggpk.exposed)

ggpk.exposed indexes the live game bundles. Paths are lowercase.

- List a directory: `https://ggpk.exposed/files/ls?path=poe1://art/2dart/minimap`
- Search by substring: `https://ggpk.exposed/files/search?path=poe1://art&filter=alva&deep=1`
- Raw file: `https://ggpk.exposed/art/2dart/minimap/player.dds`
- Same file as PNG: `https://image.ggpk.exposed/art/2dart/minimap/player.dds`

Directories worth knowing under `art/2dart/`:

| Directory | Contents |
| --- | --- |
| `minimap/player.dds` | The minimap icon sheet: every in-map marker (encounters, league mechanics, chests, doors) |
| `questicons/` | Quest-tracker icons, about 340 files, including NPCs such as `alva.dds` (128×128) |
| `skillicons/passives/atlastrees/` | Atlas passive icons as single files (also a `4k/` set) |
| `atlas/` | Atlas map quadrant art (Maven, Shaper/Elder, Searing Exarch/Eater) |

### Minimap icons: name → position

`player.dds` is a grid of 64×64 cells. On 2026-10-11 it was 896×3328, which
is 14 columns. Recompute the column count from the current width before
cropping.

The row order of `data/minimapicons.datc64` is the cell order: row `i` is cell
`(i // cols, i % cols)`. Each row's first column is the icon's Id string. This
was verified for this table on 2026-10-11: row 668 is `MercenaryEncounter`,
cell (47, 10), the crossed swords.

```python
# python3 -I lookup.py  (minimapicons.datc64 and player.png in the cwd)
import struct
from PIL import Image
b = open('minimapicons.datc64', 'rb').read()
n = struct.unpack('<I', b[:4])[0]
mk = b.find(b'\xbb' * 8)            # start of the variable-data section
rs = (mk - 4) // n                  # fixed row size
data = b[mk:]
def s(off):                          # UTF-16LE string, NUL-terminated
    o = off; out = bytearray()
    while data[o:o + 4] != b'\0\0\0\0':
        out += data[o:o + 2]; o += 2
    return out.decode('utf-16le')
names = [s(struct.unpack('<Q', b[4 + i * rs:12 + i * rs])[0]) for i in range(n)]
sheet = Image.open('player.png').convert('RGBA')
cols = sheet.width // 64
i = names.index('MercenaryEncounter')
r, c = divmod(i, cols)
sheet.crop((c * 64, r * 64, c * 64 + 64, r * 64 + 64)).save('MercenaryEncounter.png')
```

Not every NPC has a minimap icon. Alva has none among the 699 entries. Her
temple icons are the `Incursion*` rows, and she has a quest icon in
`questicons/alva.dds`.

## Fitting an icon to the sidebar

The sidebar draws square icons at 22×22 in the collapsed rail (`.rail-icon`;
only the wide Lab skull uses `.lab-icon`, 26×21), 18×18 in expanded nav rows
and 14×14 in expanded overlay and module rows; see
`desktop/src/lib/components/Sidebar.svelte`. A rail switch shows a 6 px status
dot in its top-right corner, so keep that corner of a switch icon free of the
part that identifies it. Judge candidates at that size
on the app's `--surface` colour, not at 64 px. Minimap icons are designed to
be read small and survive the downscale; thin-stroked art (atlas masteries)
does not.

Save shipped icons as 64×64 RGBA PNG in `desktop/static/`, named
`<page>-icon.png`.

Replacing an icon under the same filename showed the old image in the dev
app (WebView2) until a hard reload (Ctrl+Shift+R), observed 2026-10-11.

## Shipped icons and their sources

| File | Source |
| --- | --- |
| `lab-icon.png` | Labyrinth skull (predates this guide) |
| `harvest-flipping-icon.png` | POE-283 design handoff |
| `mercenaries-icon.png` | `art/2dart/minimap/player.dds`, `MercenaryEncounter` |
| `temple-icon.png` | `art/2dart/minimap/player.dds`, `IncursionCraftingBench` |
| `currency-exchange-icon.png` | CDN `Currency/CurrencyModValues` (Divine Orb, top-right) and `Currency/CurrencyRerollRare` (Chaos Orb, bottom-left), each 44 px, with the green swap badge from `harvest-flipping-icon.png` at its original size and position |
| `compare-icon.png` | poe.ninja gem icons (CDN gen images with the transfigured `gd` layer) for Frostblink of Wintry Blast, Summon Skeletons of Mages and Kinetic Blast of Clustering, 36 px in a triangle, under a bold gold "VS" with a dark stroke |
| `compass-icon.png` | CDN `Currency/SurveyorsCompass` resized to 64 |
| `pathstrip-icon.png` | Drawn, not game art: three room nodes on a zigzag green path into an Izaro ring at the bottom right (clear of the status dot), in the `LabGraph.svelte` colours |
| `timer-icon.png` | Drawn, not game art: gold stopwatch ring with a progress wedge and "24" |

The Lab Overlays group switch reuses `lab-icon.png`, so a switch carries its
page's icon. That art is the minimap `Labyrinth` icon.

Transfigured gems share their base gem's art. The CDN adds the transfigured
glow through a `gd` field in the gen-image URL's base64 JSON, so take their
icons from poe.ninja (or another live source) rather than from the
`Art/2DItems/Gems/...` path, which gives the plain gem.

Text drawn into an icon has to be large to survive the downscale: a "vs" at
about 15 px of a 64 px canvas disappears at 18 px. "VS" at 30 px with a 3–4 px
dark stroke still reads.
