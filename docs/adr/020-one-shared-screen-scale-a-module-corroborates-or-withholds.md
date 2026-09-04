---
uid: dde1055c-50bb-4c47-b310-7382da3dbd87
---

# ADR-020: One Shared Screen Scale; a Module Corroborates or Withholds

**The rule in full**: a module converts into the shared scale through its own
stated coefficient `k`, publishes only what something other than itself
corroborates, and never teaches the slice a new number.

## Status

Accepted (POE-234, commits `29ac1b9` + `0691be5`, 2026-09-03). Written up in the
POE-223 follow-up audit, 2026-09-04. Extends the module contract in
[ADR-014](014-desktop-features-are-modules-with-a-work-toggle-and-a-view-page.md):
that ADR made shared state flow only through SSOT slices; this one says what a
module may do to the ONE slice that describes the screen.

## Context

Three desktop consumers need to know how big the game's UI is drawn: the merc
recruit-window matcher, the Lab gem and font OCR rects (POE-233), and the temple
Entrance-plate anchor. Each of them can measure it from something on screen, and
each of them was, at one point, remembering its own answer.

The temple's private `Settings.temple_calibration` was the clearest case. It was
a second answer to a question the app already had one answer to, keyed
differently, persisted separately, and re-derived by a module that had no reason
to be the authority on the screen. A user with both a merc measurement and a
temple calibration had two numbers describing one monitor, with nothing that
compared them and nothing that could say which was right.

The reason a single store is not sufficient on its own is the temple's own
measurement chain. `anchor::sweep_range`'s ceiling is SOFT: the fine pass
refines one nominate step past the top nominee, so a capture whose true scale is
above the ceiling does not FAIL, it anchors approximately. Measured 2026-09-03
on a synthetic plate at true scale 2.10 against a 2.00 ceiling, the sweep
answered **2.05 at NCC 0.9390** — above `anchor::NCC_FLOOR`, and therefore a
"successful" anchor by every check the module applies to itself. Published, that
2.5%-wrong number becomes the geometry the Lab OCR rects are placed from, in a
module that never opened a temple, and it persists across restarts.

So the question this ADR answers is not "where is the scale kept" but "what
gives a module the right to move it".

## Decision

**`ssot::ScreenSlice` is the one store. A module converts into it through its own
stated coefficient, publishes only what something other than itself
corroborates, and otherwise withholds.**

### 1. One store, one unit

- `AppState.screen` (`ssot::ScreenSlice`) is the only remembered screen
  geometry. `Settings.temple_calibration` and `TempleSettings.calibration` are
  DELETED (POE-234 WI-2); an old `settings.json` carrying them still loads and
  the keys are dropped on the next save.
- **The unit is game-UI px per px of the reference fixture**: a 1920×1200 screen
  is `1.0` by definition, and 1080p measures `0.90 = 1080/1200`, because the
  game's UI scales with screen HEIGHT and not width. Any seed, hint or fallback
  derived from a capture's WIDTH is out of every decision path — that is the bug
  POE-234 WI-1 was opened on (a `1920/1374 = 1.397 ±15%` width seed missed a
  1920×1080 capture that anchors at exactly 1.000).
- **`null` means nothing has measured a screen, and no consumer may read it as
  1.0.**

### 2. A module converts through its own `k`, and `k` carries its provenance

A module whose cue measures something else may keep a stated conversion
coefficient. The temple's is `anchor::TEMPLE_SCALE_PER_UI_SCALE = 1.1111`, from
the temple's own 1374-px reference WIDTH into the slice's height-tied unit.

What that number is, exactly, because a coefficient without a provenance is a
magic constant: the **numerator is measured** (temple scale 1.000 at NCC 0.99999
on a 1920×1080 capture, laptop, 2026-09-03) and the **denominator is nominal**
(`1080/1200`), not a `ui_scale` any module measured on that machine. It is good
to about **one per cent**, which is `K_TOLERANCE`, and it stands on both units
being linear in the capture height.

WI-2 recomputed it against the one merc-measured 1080p reading the repository
holds (`tests/fixtures/merc-recruit-pc-1080p.png` fits at 0.8985, giving
`k = 1.1130`) and **did not adopt it**: a 0.17% correction is half of the fit's
own pitch-grid quantisation, so it is not evidence the nominal value is wrong.
Both numbers live in the constant's doc. Recording the rejected refinement is
part of the rule — the next person to measure `k` needs to know what has already
been measured and why it was not taken.

A validated conversion over the shared number is the OPPOSITE of a module owning
its own calibration: it consumes the one measurement instead of competing with
it.

### 3. The offer gate: corroborate or withhold, failing closed

`temple::run::screen_from_anchor` decides whether an anchor may be published at
all, BEFORE `ssot::accepts` decides whether it replaces:

- With a standing measurement, the anchor must agree with it to within one
  `anchor::SCALE_STEP` — the finest disagreement the module's scale grid can
  express.
- With an EMPTY slice, the anchor must agree with the capture's own HEIGHT to
  within `K_TOLERANCE` (1%).
- Otherwise the measurement is **withheld** and the reason is logged, naming
  which of the two checks refused. The shared scale is left to whatever else
  measures this screen.

**Fail closed is the accepted trade, and it has a named cost**: the gate also
refuses an off-nominal in-game UI-scale slider on an otherwise unmeasured
machine. A user who has moved that slider and never opens the merc recruit
window gets no shared measurement from the temple. That is preferred over a
soft-ceiling approximation becoming the geometry every other module places rects
from.

**A module corroborates and persists a verified seed; it does not teach the
slice a new number.** Persistence follows the VERIFYING set rather than the
replacing one — a temple anchor is written to `settings.json` like a merc frame
fit, because on a machine whose recruit window never opens it is the only
measurement there will ever be. The visible consequence is correct and should
not be read as a bug: a temple anchor that agrees with a `remembered` startup
seed inside the band is REFUSED, the seed stands, and the Settings card keeps
saying "trusted from last session" — the temple confirmed the number, it did not
replace it.

### 4. Keying, and what is out of scope

The slice is keyed by **monitor id + CAPTURE size**, not client size.
`ssot::screen_matches` compares the monitor id first and falls back to
dimensions alone only when either side is the unknown `0` (a scale persisted
before POE-237).

**Windowed play is out of scope.** The capture is the whole game monitor, so a
windowed client's UI scale is not a function of the capture size and this keying
cannot express it. The task text was amended to say so rather than leaving the
gap implicit.

### 5. Lifecycle: exactly three re-measures

A measurement is remembered and TRUSTED at start — `settings::apply_to_state`
loads it back as `remembered`, and no module runs a blind sweep to re-derive it.
It is VERIFIED by the consuming module on first use, and `verifiedThisSession`
(POE-240) is that made readable; it is never persisted, so every launch starts
unverified. It is dropped and re-measured on exactly three events:

1. the capture's display or dimensions differ from the remembered ones
   (`ssot::drop_if_mismatched`, first thing on every merc and temple detect
   tick, which on the temple's tick means BEFORE the hint is derived from it);
2. the consuming module's verification fails;
3. the user presses **Recalibrate** (`ssot::geometry_recalibrate`).

**Nothing else re-measures.** And Recalibrate is not merely an emptying: a
module holds session-local memory that would republish the forgotten number on
its very next tick, so the command also bumps `temple_rearm` and `merc_refit`,
and `temple::run::cheap_hint_for` makes the slice the authority over the
temple's remembered plate position — a hint that disagrees by more than one
`SCALE_STEP` is dropped, and so is one that WAS there and is gone. That last
clause is an EMPTYING, not an emptiness: a slice this session never filled
leaves the plate alone, because a screen nothing has measured is not a decision
to forget one.

### 6. Read regions are keyed on the layout anchor, never on the screen edge

The same discipline one level down, and the clause that makes the shared scale
worth having (POE-230, commit `71df527`).

A module's OCR rects are placed from `(origin, scale)` — the anchor it just
measured — and never from an edge of the capture. Measured 2026-09-03 on the
laptop (1920×1080, anchor scale 1.000, origin (960, 713)): the temple's side-panel
crop and diamond crop were keyed on the capture's right and top edges, but the
panel sits at a FIXED offset from the layout (x +211..+695, y −669..−295 ref px
from the Entrance origin) — the same to the pixel on the 1374-px reference
board. The edge-keyed crops cut the panel in half: the title read
`NG WORKSHOP`, the lower-left architect block fell outside the crop, and the
diamond read 5 of 6 seals and fell back to the beam read.

An edge-keyed rect is a second, worse answer to the same question the anchor
already answered. The measurement instrument for it is one `app.log` line per
distinct geometry (`Temple: rois panel […] diamond […] remaining […]`), said
once per `(origin, scale)` and not once per read — `app_log` keeps 50 entries
and the read runs at up to 1 Hz.

## Consequences

- **A module's private calibration is a defect, not a design option.** Adding a
  second remembered geometry re-opens this ADR. Adding a conversion coefficient
  over the shared one does not, provided it carries its provenance and its
  accuracy.
- **`source` is a label, not a rank.** `merc-frame` / `merc-ocr` /
  `temple-anchor` / `remembered` say what LOOKED. `ssot::accepts` reads the
  label to refuse a band-limited cue that only re-states the standing value —
  unless it comes off a DIFFERENT display, which no drift band can explain
  (POE-237). There is no precedence table to consult and none to add.
- **A machine can end up with no measurement at all, and that is correct.** The
  slice stays `null`, every consumer says "not measured yet", and nothing
  substitutes 1.0.
- **The temple's anchor chain is a hint → table → sweep ladder, and the
  exhaustive sweep is unreachable from the loop.** `anchor_for_loop` tries the
  slice-derived hint, then the `MEASURED_SCALES` table, then the pyramid sweep
  (5.3 s against 28.4 s exhaustive in the release container, same answer);
  `full_sweep` is a Debug command only. The measurement that opened POE-234 was
  the exhaustive sweep taking **347 779 ms** on the reporting laptop.
- **The `k` log line survives as a CHECK, not as instrumentation.**
  `temple anchor not corroborated by the capture: unit ratio k=…` fires only
  when a board's own ratio is more than `K_TOLERANCE` from the constant, which is
  the one thing a user's `app.log` can add that the repository's fixtures cannot.
- **An accuracy row is open, not closed.** The 1539-px board implies scale 1.111
  from its panel border where the anchor recorded 1.13 — a ~1.7% ANCHOR error,
  recorded against POE-247 rather than absorbed into `k`. A coefficient is not
  the place to hide a measurement error in the thing it converts.
- **The published read regions this feeds are governed by
  [ADR-019](019-nothing-a-module-draws-may-cover-what-that-module-reads.md)**:
  they are also the set nothing the module draws may cover, so a rect that is
  wrong here is wrong twice — a bad read, and a surface placed clear of a
  rectangle that is not where it thinks it is.
