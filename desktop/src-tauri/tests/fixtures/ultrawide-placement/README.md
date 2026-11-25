# Offline capture placement fixtures (POE-501)

Current test-fixture behavior. These are synthetic reconstructions from two
player-supplied rasters, not screenshots. They contain no original pixels,
text, icons, names, chat, accounts or app windows. Tests compare production
placement with source measurements; the drawn boxes are only illustrations
of that evidence and are never read back as expected geometry.

Coordinates are capture pixels, with exclusive right/bottom edges. All raw
clients use origin (0, 0); monitor sizes below are explicit test assumptions,
not observations of the original physical monitor. No scaling or translation
is applied to the measured source coordinates.

## `pillarbox.png`

Observed source: 3742x1076 RGBA. At unobscured row 1000 the game starts at
x=624 and ends after x=3215; the side bars are 624/526 pixels. Under the
coordinator's option-(b) ruling, restore 98 black pixels at the right:
3742+98=3840 and 526+98=624. Inferred: the source lost that part of a symmetric
3840-wide raster. Retain the observed 1076 height. The assumed monitor and
raw client are 3840x1076 and [0, 0, 3840, 1076]; the rendered client is
[624, 0, 2592, 1076].

Reconstruct on an opaque black RGBA canvas. Fill the game rectangle
[624, 0, 3216, 1076) with RGB (48, 64, 80). In particular, normalize its
top row, y=0, x=624..3215 inclusive, to that non-dark colour: the source's
sampled row was dark, producing top=1 and a both-axes refusal. This is the
expressly ruled fixture setup, with no vertical shift. The 98 new rightmost
columns stay black.

Removing overlay-preview models its exclusion from real grabs. Removing the
main app window is a separate scenario choice: capture does not exclude it.
The unmodified source probe samples L/T/R/B=298/1/116/0; padding alone gives
298/1/214/0, so neither can trim. These outcomes must not be presented as
successful trim of an unmasked real grab.

Observed panel frame sides: x=1245/1927; straight body interior x=1255/1917.
Outer panel top y=161; title gold rule bottom y=214; gem-choice dialog top y=338 (first gem-frame bevel y=355). No Crafts Remaining
box is drawn: the lowest panel text is CRAFT, ending near y=642.
Icon-square edges, left/top/right/bottom: (1463,371,1513,421),
(1564,371,1614,421), (1664,371,1714,421).

## `letterbox.png`

Observed source: 2556x1440 RGBA. At unobscured x=100, row 1079 still has
game pixels; rows 1080 onward are black until overlays. Assume a 2556x1440
monitor and inside raw client [0, 0, 2556, 1080]. This is top-aligned game
content, not symmetric letterboxing. Reconstruct the game rectangle
[0,0,2556,1080) in RGB (48,64,80) on an opaque black canvas.

Remove overlay-preview as excluded-window setup. Removing the main app
window and taskbar is a disclosed scenario choice. In the unmasked source,
the app/taskbar make all four sampled edge extents zero; after their removal
the asymmetric bottom bar still cannot trim. A whole-monitor raw client
therefore stays [0,0,2556,1440]; only the assumed inside raw client supplies
the measured game height. This test does not prove automatic asymmetric trim.

Observed panel frame sides: x=603/1283; straight body interior x=613/1273.
Outer panel top y=190; title gold rule bottom y=244; gem-choice dialog top y=343 (first gem-frame bevel y=360). No Crafts Remaining
box is drawn: the lowest panel text is CRAFT, ending near y=619.
Icon-square edges: (819,374,869,424), (919,374,969,424),
(1019,374,1069,424).

## Measurements and reconstruction

Measurements were taken by opening each original, inspecting native-coordinate
panel crops with 50px grids and enlarged icon crops with 5px grids. Icon
edges follow the black square inside the metallic bevel, not the outer frame
or cyan app preview. Use 5px tolerance on each edge and Font landmark: the
blurred bevel transitions span about 4-6px, making a sharper boundary claim
unjustified. This tolerance was chosen before evaluating resolver output.
Only the drawn three-gem choice dialog is covered; no six-option state is
represented.

For reproducibility, draw a 2px RGB (140,115,65) border with RGB (18,18,18)
interior at [1245,161,1927,468) / [603,190,1283,446). Draw the title rule
across those sides at y=214 / 244. Fill the measured icon squares RGB
(40,100,80), with a 1px RGB (190,160,90) outline. Replace the lowest-text
landmark with a horizontal RGB (140,115,65) line from panel-left+260 to
panel-left+360 at y=642 / 619. All alpha values are 255. No text is drawn.
The rest of the game is flat colour to keep native-sized PNGs compressible
and preserve the trim algorithm's minimum dimensions and edge sampling.

## Font containment control

Observed on the read-only accepted 1920x1080 `../lab/font-gems-1080p.png`:
outer frame x=287..968, top y=214; body x=297..958; first content (gem-choice
dialog) y=343; lowest CRAFT text ends at y=592. No Crafts Remaining is drawn.
These measurements use the same native-coordinate grid method and 5px tolerance.
The Font crop starts strictly below the outer top and at/before the first
content row within tolerance, covers the inner column, stays within the outer
sides, and reaches the lowest text. The title-band bottom (about y=265 on this
reference) is not a lower bound. The identical oracle must accept all three
resolved frames and reject each deliberately content-cutting crop (top six
pixels below the first content row, beyond the five-pixel measurement band).
Reference slots are not asserted or reconstructed.
