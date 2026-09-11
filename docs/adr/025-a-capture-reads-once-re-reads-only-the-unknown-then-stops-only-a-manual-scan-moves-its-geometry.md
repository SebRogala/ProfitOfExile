---
uid: fd420ce4-f287-4c52-a598-23bd20dea0d7
---

# ADR-025: A Capture Reads Once, Re-reads Only the Unknown, Then Stops; Only a Manual Scan Moves Its Geometry

**The rule in full**: a module capture is read in full once; while that read
is incomplete it buys at most `RETRIES` = 2 more rounds, each re-reading only
what the kept read still leaves unknown; after that, or once the read is
complete, only the presence/liveness check runs. A live capture's geometry is
not moved by a later detect: the full-frame locate runs only on a cold start
with no placement or on a manual scan whose placed read missed, once per key,
and only
the manual scan may replace a standing placement. The contract is shared by the
temple and merc modules; the code is not (POE-278).

## Status

Accepted (owner acceptance criteria stated on POE-278, 2026-09-10).

Temple: shipped — every clause is current behaviour, with the homes named in
the table below (`cdd67ba` partial rounds, POE-249 WI-2, 2026-09-07; `54e7ccd`
POE-269 keyed fallback; `05a51a8` Manual-only placed-miss sweep, 2026-09-09;
`332e40b` "placed" means anchored, POE-278).

Merc: clause 1 shipped; clause 4 shipped (`POE-278 WI-B`, `f78786e`); clauses 2, 3 and 5 shipped (`POE-278 WI-C`).

## Context

The owner's acceptance criteria on POE-278, as stated:

1. One full scan locates and reads the panel.
2. While the read is incomplete, re-scan ONLY what is still unknown (cells, header fields), on the crop.
3. Once fully read, OCR stops (liveness check only).
4. A live capture's geometry is not moved by a later detect. Anchor overwrite only on a Manual scan (Scan now / Recalibrate).

**The incident** (app.log 2026-09-10, as recorded on POE-278). 17:25:51: the
merc crop detect found 6 rows at scale 0.896; some icons stayed unknown, so the
capture never became complete. 17:25:57: the player hovered a gem (template
confirmed at row 2 slot 0) and its tooltip covered the panel. 17:25:58: the crop
detect missed and the full-frame fallback found 4 rows, logging
`merc: placed panel [725,552,498,428] contradicted by detect [724,642,497,340]`
— two row pitches lower and 88 px shorter. That origin was remembered through
`ssot::remember_anchor`, and every later read was "6 rows on screen, 4 read"
until a manual Recalibrate at 17:27:55. On a live capture the fallback was
bought by the `geometry::PanelAnchor::ColumnMoved` door in
`mercenary/run.rs::detect_tick`: that condition takes the keyed fallback even
when the gate step (`fallback_allowed`: `GateStep::Probe | GateStep::FullDetect`)
does not allow one, so the live re-detect, which carries no voice or Scan-now
gate, can spend it.

**What merc does today** (verified 2026-09-11, before WI-B and WI-C). While a live capture is
incomplete, `run::LoopState::detect_interval` re-detects every
`REDETECT_INTERVAL` (2 s), and every re-detect re-OCRs every layout row in
`read::pass2_texts` (bounded by `pass2_row_budget`) and re-runs
`read::build_capture`'s icon pass over every row, with no round cap. The only
unknown-gated re-read is the hover tick's `run::HoverBudget`. The full-screen
locate (`geometry::detect_reason`) is spent once per `FallbackKey { refit, gate }`
(`merc_refit`, trigger generation) by a missing placement, a voice-line probe
miss, a placed-crop miss under `Probe` or `FullDetect`, or a `ColumnMoved`
placed layout on any tick; `run::fallback_panel` turns a located panel that
`geometry::placed_panel_contradicted` judges moved into an origin, and
`run::publish_then_remember` hands it to `ssot::remember_anchor` after the
read publishes.

**What temple already does.** The temple loop implemented all four criteria
before POE-278: one full read per board identity, up to two partial retry
rounds decided from the kept read, OCR stopped after that, and a placed-miss
sweep spent under the Manual arm only ([Temple Lifecycle](../TEMPLE-LIFECYCLE.md),
row 2, row 3 and "Owner decisions this encodes"). ADR-024 and its amendments
describe the placement and the keyed fallbacks of both modules, but no document
stated the read budget and the geometry rule as one contract, so the merc path
was built without them.

## Decision

1. **One full read.** One full read locates and reads the panel.

2. **At most two more rounds, each on the unknown only.** While the read is
   incomplete, at most `RETRIES` = 2 more rounds run — three reads in all. Each
   later round re-reads ONLY what the kept read still leaves unknown, decided
   from the kept read BEFORE the round's OCR. Whatever the round did not look at
   rides through from the kept read unchanged, and a confident read is never
   replaced by a later round.

   Merc's "unknown" is: a row whose skill read is not confident; a support cell
   that is not confident (`ReadState::Matched | ReadState::Confirmed` are
   confident — `read::confident`); a header field not resolved — name missing or
   not name-shaped (`geometry::is_name_shaped`), class missing, level missing,
   which are `read::header_complete`'s fields. The wager is excluded there and
   here.

3. **Then stop.** Once the read is complete, or the rounds are spent,
   re-reading stops and only the presence/liveness check continues. Temple: the
   650 ms cheap presence tick. Merc: the placed-crop detect at
   `LIVENESS_INTERVAL` (10 s), which OCRs the crop to know the window is still
   there (and to notice a REMATCH) and re-reads nothing. Merc's hover tick is
   NOT a read round: it is the player-driven per-cell correction with its own
   budget (`HoverBudget`), and it keeps running over a complete or round-spent
   capture.

4. **A live capture's geometry is not moved by a later detect.** The full-frame
   locate — temple: the pyramid sweep; merc: the full-screen
   `geometry::detect_reason` path — runs only
   (a) on a cold start with no placement, once per key, or
   (b) on a MANUAL scan whose placed read missed, once per key.
   Only (b) may REPLACE a standing placement (`ssot::remember_anchor`, after a
   successful read). Manual means temple Re-arm, and merc Scan now and
   Recalibrate. A placed miss under an automatic trigger — temple AlvaStart and
   TempleArea; merc the voice-line probe, the live re-detect and a
   `ColumnMoved` placed layout — trusts the placement and re-locates nothing.

5. **What starts round 1 again.** A new identity or a Manual scan starts round 1
   with a fresh budget. Temple's identity is the board key
   `(temple_epoch, temple_rearm, BoardFrame)`; merc's is a new capture, or a
   panel judged REPLACED by `read::panel_replaced`.

The contract is shared; the code is not. Each module keeps its own
implementation, and no abstraction between them is introduced (owner scope,
POE-278).

### Where each clause lives

| Clause | Temple (shipped) | Merc |
|---|---|---|
| 1 one full read | row 2; `run::LoopState::gate` → `GateAnswer::Read`, `run::full_read` | shipped: `run::detect_tick` — placed crop (`geometry::placed_panel_crop`, `geometry::placed_layout`), then `read::pass2_texts` and `read::build_capture` |
| 2 two partial rounds | row 2 (WI-2) and "Where each rule lives"; `slice::plan_read` / `retry_plan` → `ReadPlan`, `slice::merge_reads` over `KeptRead`, `slice::unclean`, `run::RETRIES`, `run::kept_for` | shipped (`POE-278 WI-C`): `read::plan_read` → `ReadPlan` / `RowPlan` from the kept capture, `read::pass2_planned`, `read::build_planned` (copies confident cells), `read::fold_unresolved_header`, `read::lines_up`; `run::RETRIES`, `run::LoopState::rounds`, `run::round_plan` |
| 3 then stop | row 2 ("After round 3 all OCR stops"); `DETECT_INTERVAL` 650 ms, `GateAnswer::Reshow` | shipped (`POE-278 WI-C`): `read::capture_complete` or `LoopState::rounds_spent` → `LoopState::detect_interval` returns `LIVENESS_INTERVAL`; `ReadPlan::Nothing` → `read::carry_capture` re-reads nothing, and `run::replaced_on_sight` checks a REMATCH on pass 1 |
| 4 geometry moves only on a manual scan | row 3, residual "Placed-origin verification", "Owner decisions" 2026-09-09; `run::cold_sweep_reason` → `ColdSweepReason::{NullSlice, PlacedMiss}`, `run::cold_sweep`, `placed_origin_contradiction`, `remember_fallback_anchor` → `ssot::remember_anchor` | shipped (`POE-278 WI-B`, `f78786e`): `run::locate_decision` (`LocateReason::{ColdStart, ManualMiss}`, `PlacedRead`), asked from `run::detect_tick`; `run::manual_tick` / `refit_requested`; `MERC_COLUMN_TRUSTED_LINE` |
| 5 fresh budget | row 3 (board identity); `run::board_key`, `slice::BoardFrame`, `LoopState::note_read`; Re-arm bumps `temple_rearm` | shipped (`POE-278 WI-C`): `LoopState::refill_rounds` — from `run::round_plan` on a new capture or one `read::panel_replaced` dropped, `LoopState::resume` on Scan now, `run::refills_budget` on a Recalibrate `consume_refit` acted on |

Row numbers and section names are those of [Temple Lifecycle](../TEMPLE-LIFECYCLE.md).

Temple clause 4, as shipped: (a) is `ColdSweepReason::NullSlice` — no screen
slice, or no ANCHORED Entrance origin (`332e40b`: a seed is not a placement) —
for any arm source, once per `(temple_epoch, temple_rearm)` key, with one
release when the sweep found an anchor whose slice was withheld
(`null_sweep_key_after_publish`). It cannot replace an anchor, because (a)
runs only when no anchored origin stands. It fills the empty anchor only when
the swept origin lies outside `FRAME_ORIGIN_TOLERANCE` of the hint's origin
(the seed, on a non-null slice); a sweep that confirms the seed leaves the
anchor empty, so the next key's below-floor tick is (a) again.
(b) is `ColdSweepReason::PlacedMiss`, spent only under `ArmReason::Manual`.

Merc clause 4, as specified: merc's placement is `run::merc_placement` — the
screen slice's `ssot::placements` merc panel, seed or remembered (a remembered
`anchors.merc_panel` replaces the seed's origin). Case (a) is `merc_placement`
returning `None`, which happens only with no screen slice. Temple's "a seed is
not a placement" (`332e40b`) does not carry over: a merc placed miss with no
remembered anchor is a placed miss, not a cold start. A merc cold start
remembers nothing, because `LocateReason::ColdStart` carries no placement and
`run::fallback_panel` yields no origin for it; only
`LocateReason::ManualMiss { placed }` can become a remembered origin.

Merc clause 3, history: until `POE-278 WI-C` only the cadence was shipped —
the liveness detect still ran `read::pass2_texts` and `read::build_capture` on
the crop every `LIVENESS_INTERVAL`, and a capture that never became complete
re-read every row and cell at `REDETECT_INTERVAL` with no round cap. Since
`POE-278 WI-C` the liveness detect re-reads nothing (`ReadPlan::Nothing`), and
a capture whose rounds are spent stays on `LIVENESS_INTERVAL` while it is
incomplete.

## Consequences

- An incomplete read after three rounds stays incomplete until a hover (merc)
  or a Manual scan / Re-arm. That is the accepted cost of stopping; the rounds
  are not extended to buy it back.
- A wrong placement is not self-corrected by automatic triggers. Scan now,
  Recalibrate and Re-arm are the answer, and the geometry notice (POE-271,
  backlog) is its eventual surface.
- **Open residual, not addressed by POE-278:** merc
  `geometry::placed_panel_contradicted` compares origins only, so a Manual scan
  taken while a tooltip hides the top rows can still judge the occluded
  (strict-subset) locate a move and remember it.
- The two modules implement the same five clauses separately. A change to the
  contract is made here and then in each module; neither module's code is the
  reference for the other.

## Supersession pointers

- For merc, ADR-024's POE-270 amendment sentence "a placed crop whose
  Wager/Recruit/button anchors are absent while the voice gate is active, spends
  one full-screen locate" becomes Manual-only (clause 4). Its first half — a
  missing or unplaced screen slice — is clause 4(a) and stands.
- For merc, ADR-024's POE-270 fix-round sentence that the placed path "rejects
  a pass-1 name-column median that moves beyond the half-cell band, then spends
  the keyed full-screen fallback" becomes Manual-only: the rejection is a placed
  miss, and outside a Manual scan it re-locates nothing.
- ADR-024 Decision 3's "one fallback locate" on a placed miss is, for both
  modules, the clause 4 locate; temple has followed it since ADR-024's
  2026-09-09 amendment.
- ADR-024 carries the matching amendment, "POE-278 — merc placed misses buy no
  locate outside a manual scan".

## Amended 2026-09-11 (POE-275)

**Temple clause 4(a) is amended; merc's is unchanged.** For the temple, the
full-frame locate on a cold start with no placement — `ColdSweepReason::NullSlice`,
no screen slice or no ANCHORED Entrance origin — no longer runs "once per key"
on the first miss. Owner, 2026-09-11 (POE-275 WI-2): the first miss after the
start line is the sheet not being open yet, and a sweep spent there left the
rest of the key with no hint, so the player needed Re-arm or Recalibrate. The
temple's 4(a) now reads: not on the first miss; on every `NULL_SWEEP_EVERY` = 3
consecutive clean misses, up to `NULL_SWEEP_CAP` = 10 sweeps per
`(temple_epoch, temple_rearm)` key (both provisional); never over a live panel;
and off the loop thread, with the 650 ms placed recheck running while it
searches and cancelling it when it anchors. The found-but-withheld release
(`null_sweep_key_after_publish`) rides on top: a second withheld result ends
that key's null sweeps. A found origin is read only after a later capture
confirms it. Homes: `run::cold_sweep_reason` and `run::SweepBudget` (when),
`run::SweepSlot` and `run::confirm_swept` (off the loop), `run::sweep_line`
(one measured line per sweep); normative write-up in
[Temple Lifecycle](../TEMPLE-LIFECYCLE.md), "Cadences and budgets".

Merc's clause 4(a) — one full-screen locate per key when there is no screen
slice — is unchanged. Clause 4(b) and "only (b) may REPLACE a standing
placement" are untouched for both modules: a null-slice sweep still runs only
when no anchored origin stands, so it cannot replace one, and the temple's
placed-miss sweep is still `ColdSweepReason::PlacedMiss` under
`ArmReason::Manual`, once per key.
