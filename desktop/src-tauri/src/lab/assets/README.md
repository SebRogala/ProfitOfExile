# Divine Font probe assets

Current production inputs, derived 2026-11-16 from the committed real capture
`desktop/src-tauri/tests/fixtures/lab/font-gems-1080p.png` (1920×1080,
ui_scale 0.90). Both PNGs are lossless crops at source scale, with no repainting:

| Asset | Source rectangle [x,y,w,h] |
| --- | --- |
| divine-font-title.png | [540,216,176,40] |
| divine-font-confirm-lit.png | [542,466,170,36] |

`ffmpeg -i <source> -vf crop=<w>:<h>:<x>:<y> -frames:v 1 <asset>`
produced the crops. The title's offset from the middle gem slot and the
CONFIRM offset from the title are measured on this frame. The probe scales
them from projected slot geometry and searches a small rounding margin.

Evidence is limited to this one real frame. Masked/darkened negatives and
resized 4K/16:10 frames in tests are derived evidence, not independent captures.
The error tolerance is an appearance comparison against these pixels, not a
probability or a measured confidence across machines.
