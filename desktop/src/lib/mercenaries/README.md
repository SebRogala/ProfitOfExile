# Mercenary module — behaviour contracts

The Mercenaries module (ADR-014 module id `mercenary`) reads the in-game recruit
window and tells the player whether the mercenary is worth the wager. The Rust
half lives in `desktop/src-tauri/src/mercenary/` (trigger, capture, OCR, icon
matching, trade lookup); this directory holds the frontend half — the verdict
engine, the guide rulesets, and the wording of the page and the overlay strip.
Component-level entries are in `desktop/src/lib/README.md` → "Mercenary Data";
Windows overlay mechanics and smoke checks are in `docs/OVERLAY-GUIDE.md`.

This file records the behaviours that are owner decisions rather than
derivable from the code. Change the code and this file together.

## Overlay strip — when it is on screen

Owner decision, 2026-09-01.

The strip is on screen **for as long as a recruit window is being worked, and
four seconds longer**:

| Module status | Strip | What it shows |
|---|---|---|
| `scanning` | shown, for as long as the burst looks | `scanning for the recruit window…`, prefixed `heard <name> · ` when a voice line named the speaker — a `Scan now` burst has no speaker and shows the bare line |
| `live` | shown, for as long as the read lasts | `reading · N rows · …`, or `N rows on screen, M read` when the sensor is ahead, plus header, verdict line and per-row glyphs |
| `done` | shown, for as long as the window is on screen | `done · N rows · …`, or `N rows on screen, M read` when the sensor is ahead, plus header, verdict line and per-row glyphs |
| `idle` — window gone | shown for **4 s** after the retire, then cleared | `recruit window gone — last read` over the last verdict |
| `idle` — waiting | shown for **4 s** after going idle, then cleared | `waiting for a mercenary · Scan now on the page` |
| `off`, `unavailable` | never shown | — |

"Cleared" means the panel is not rendered at all — the transparent window stays
where Settings placed it, but nothing is drawn, so the overlay disappears.

Rules that follow from the table:

- A burst that fires during the linger (a voice line right after the window
  closed) shows the strip in full; the idle after it gets a fresh four seconds.
- Staying idle does not restart the clock. The route polls the slice every
  three seconds, and a clock restarted on each poll would never run out.
- The clear is driven by the linger, not by the next poll: the route arms one
  timeout for the remainder of the four seconds, plus 50 ms so a timer that
  fires on the boundary reads the clock as expired. The strip therefore goes
  within ~50 ms of the four seconds rather than up to a poll (3 s) later.
- The cost is known and accepted: an armed module with no strip on screen is
  indistinguishable from one that never triggered. The Mercenaries page carries
  the waiting state, and the triggering itself is to be reworked (open, no
  ticket yet).

Where it lives: `overlay-view.ts` — `LINGER_MS`, `OverlayLinger`,
`lingerAdvance`, `lingerRemainingMs`, `overlayShown` (all unit-tested in
`overlay-view.test.ts`); the clock and the single timeout are in
`desktop/src/routes/overlay/mercenary/+page.svelte`, which has no test harness
and owns nothing else about the decision.

History: 2026-08-25 the strip gained the always-present status line so an idle
module was not pixel-identical to an overlay that failed to build; 2026-09-01
that permanence was withdrawn in favour of the four-second linger, because a
panel that stays over the game after the decision is made costs more than it
tells.

### Placed crop and row counters

The normal detect reads the SSOT-placed panel crop. Its fixed padding is about
45% of the full-screen area on the reference geometry, so the accepted saving
is approximately 2× in OCR work, not an order of magnitude. The 4,504 ms
full-screen baseline quoted by ADR-024 is the original README/module baseline;
it is not a placed-crop timing claim.

Rust publishes `rowsOnScreen` and `rowsRead` on every capture. The first is the
skill-icon sensor, including one-pitch probes above and below the enumerated
geometry; the second is the number of published rows whose skill OCR resolved.
The strip prints both whenever the counters differ. A placed geometry seed is
trimmed at the last occupied skill icon when the button line is absent, so
phantom trailing rows do not reach the verdict engine.

## Capture contract (ADR-025)

Owner acceptance criteria, POE-278 (2026-09-10). The contract is stated once,
for merc and temple, in
[ADR-025](../../../../docs/adr/025-a-capture-reads-once-re-reads-only-the-unknown-then-stops-only-a-manual-scan-moves-its-geometry.md);
this is the merc mapping.

One placed-crop read locates and reads the panel: round 1 of a capture,
`run::detect_tick` — pass 1 on the crop, then pass 2 for every row
(`read::pass2_texts`) and the icon walk for every row (`read::build_planned`
with no plan, the form `read::build_capture` names). While the kept capture
(`Session.current`) is incomplete, at most `run::RETRIES` = 2 more rounds run
at `REDETECT_INTERVAL`, each re-reading only what the kept read leaves unknown:
a row whose skill is not confident, a support cell that is not `Matched` or
`Confirmed` (`read::confident`), and a header field `read::header_complete`
does not accept (`read::unresolved_header_fields`) — name missing or not
name-shaped, class, level; never the wager.

The round's plan is `read::plan_read` → `read::ReadPlan`, decided from the
kept capture before any of the round's OCR: `Full`; `Partial`, with a
`read::RowPlan` per layout row — `Read` (pass 2 and the whole icon walk: a row
whose skill is not confident, or one the kept capture has no row for),
`Cells` (the kept skill verbatim, the occupancy walk, each confident kept cell
copied and the rest matched) or `Unseen` (every kept cell confident and copied;
the walk matches only a slot past the last kept cell that reads occupied — a
slot no read has seen is unknown too); or `Nothing`. An empty slot stops the
walk, not the row: a panel's cells are a prefix, so a confident kept cell at or
past it proves the dark read is this frame's, and the kept cells up to the last
confident one ride verbatim; past it the empty read stands, as in a full read,
so an occlusion phantom does not outlive its tooltip. A row taken from the kept
read is never trimmed for a dark skill icon.
`read::pass2_planned` re-OCRs only the `Read` rows, and `read::build_planned`
copies a confident kept cell instead of matching it — at this layout's rect,
identity and read state from the kept read — so a confident read is never
replaced by a later round. A kept capture that no longer lines up with the
layout (`read::lines_up`: its panel rect differs, a kept row is missing, or a
kept cell's size differs or its origin leaves the half-cell band) reads `Full`
and spends a round, so a jittery geometry cannot read forever. An unresolved
header field is folded from pass 1 with `read::merge_header`'s rules and a
resolved one is kept verbatim (`read::fold_unresolved_header`). The round count
is `run::LoopState::rounds` (`note_round`, `rounds_left`, `refill_rounds`),
and `run::round_plan` is where the plan and the budget meet. Each reading
round logs `Merc: read round N of 3 — full | re-read R rows, C cells, header
<fields> — pass 2 N ms, icons N ms; complete | incomplete, K rounds left`; a
budget spent over an incomplete capture says once what is still unread.

Once `read::capture_complete` holds, or the rounds are spent, the plan is
`Nothing` and `LoopState::detect_interval` drops to `LIVENESS_INTERVAL`
(10 s). That detect OCRs the placed crop — is the window still there, is it a
REMATCH, is a header field still to fold — and re-reads nothing: no pass 2,
no icon walk (`read::carry_capture`). Copied cells keep their cached crops
across a read (`read::ReadResult::carried` → `run::merge_sigs`), so a hover
can still learn from a copied cell whose crop was cached; one with no cached
crop stays `NoCrop` (accepted), and no crop is carried across a registration
change (`run::carried_for_sigs`). The REMATCH check runs on pass 1 before the plan
(`run::replaced_on_sight` → `read::panel_replaced`, thresholds unchanged), so
a tick that reads nothing still sees a rematch at the same level. The hover
tick is not a read round: it is the player's per-cell correction under
`HoverBudget` and keeps running over a complete or round-spent capture. A
round-spent capture opens no trade session; a hover that completes it opens one
on the next liveness tick's complete edge.

The full-screen locate (`geometry::detect_reason`) runs only on a cold start
with no placement, or on a Scan now / Recalibrate whose placed read missed,
once per key; only the second may replace the remembered panel
(`ssot::remember_anchor`, after a successful read). The placement is
`run::merc_placement`, seed or remembered, and it is missing only with no
screen slice — a placed miss with no remembered anchor is still a placed miss,
not a cold start (ADR-025, "Merc clause 4, as specified"). The voice-line
probe, the live re-detect and a `ColumnMoved` placed layout trust the
placement. A Recalibrate counts as manual from the press until the merc loop
acts on it (`consume_refit` runs on the next tick that produces a layout);
until then every placed miss, a voice-probe miss included, is a manual miss and
may locate once per `FallbackKey`. Shipped in POE-278 WI-B (`f78786e`):
`run::locate_decision`, asked from `run::detect_tick`.

Round 1 again, with a fresh budget, on: a new capture (no kept capture — a
first look, including after a retire and `restore_retained`); a panel
`read::panel_replaced` judges a REMATCH; a Scan now (`LoopState::resume`,
whether or not it resumed a paused read, so Scan now over an incomplete
capture buys a fresh read too); a Recalibrate the loop acted on
(`consume_refit` answered `Dropped` or `NothingHeld`, `run::refills_budget`);
and — merc-specific, not in ADR-025 clause 5 — a template-store generation
change (`run::generation_changed`: a forget or reset disowned confirmations, or
a sync corpus landed), so a `Confirmed` cell whose confirmation was disowned is
read afresh rather than copied.

Accepted costs: a read still incomplete after three rounds waits for a hover,
or a Scan now or Recalibrate; a wrong placement waits for Scan now or
Recalibrate; a real trailing cell that is not confident and reads dark for one
frame is dropped, as a full read drops it, and is walked again as unseen on a
later partial round. Open residual: `geometry::placed_panel_contradicted` compares origins only, so a Scan now
taken while a tooltip hides the top rows can still remember an occluded locate.
