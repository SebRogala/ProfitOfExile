# Temple module lifecycle — arming, detection, OCR, and what the overlay shows when

**Status:** normative design, owner-ordered 2026-09-04 (POE-249); **implemented 2026-09-04
(POE-249): fa5bc61 (the trigger), 15eb3f8 (the loop), 8d287e5 (the waiting notice), ee1f2c7
(the offer boxes)**. Every line is now tagged
**shipped** with the commit that shipped it; nothing here is planned any more. Read this before
touching `desktop/src-tauri/src/temple/{trigger,run,slice}.rs` or the temple overlay widgets.
Related: [Overlay Guide](OVERLAY-GUIDE.md) (windows, click-through, smoke items),
[ADR-014](adr/014-desktop-features-are-modules-with-a-work-toggle-and-a-view-page.md)
(the module contract; the POE-246 amendment note), [ADR-019](adr/019-nothing-a-module-draws-may-cover-what-that-module-reads.md),
[ADR-020](adr/020-one-shared-screen-scale-a-module-corroborates-or-withholds.md).

## The one sentence

The capture runs only while something says an incursion is in scope; the sheet-bound overlays
live with the temple **sheet** on screen; the room overlay lives with the **incursion**; a full
OCR read happens once per board, with bounded retries, and then stops.

## States

`idle → waiting → reading → read → playing → idle`

These five words are this document's, and they are NOT `slice::TempleStatus`'s — two of them
mean the opposite there, and renaming the enum would be churn across the suite with no
behaviour change. The mapping: this doc's `idle` is `waiting_for_panel` false with no advice
(`TempleStatus::Idle` is *running, nothing read yet*); its `waiting` is `waiting_for_panel` true
(`TempleStatus::Waiting` is *stood down, not capturing*); `reading` and `read` are the statuses
of those names;
`playing` is advice present with the status `PanelNotVisible` or stood down.

| # | Event | Capture / OCR | Overlay | Status |
|---|---|---|---|---|
| 1 | **Alva START phrase** (`Time to go.` / `Let's go.` / `It's time!`) while `idle` — any other Alva line while `idle` arms the capture (POE-242 speaker match) but starts no cycle and shows no waiting overlay | arm the loop (`trigger.rs`): `trigger::classify` is the one owner of the per-line decision, and a START phrase arms `ArmReason::AlvaStart` with **no deadline** — the portal wait is unbounded (see the mining below), so the arm holds until an END line or a zone change. An END line is the one permitted shortening, and only of an `AlvaStart` arm: it becomes the ordinary `ALVA_TAIL_MS` tail from its own stamp. Walking into the temple **assigns** `ArmReason::TempleArea`, so the banter inside cannot cut the arm short. Cheap presence tick every **650 ms** (`DETECT_INTERVAL`: screen grab + one hinted correlation at the remembered screen scale, ADR-020) | the **"waiting for the temple panel"** notice, as the PLACEABLE widget `temple.waiting` — gated on `view.ts`'s `overlayShowsWaiting` (`waitingForPanel` and no board, so a START heard with the sheet already open never blinks it). Its shipped default is **top-centre**, `{830, 16, 260, 40}` CSS px, NOT the screen centre the owner asked for: at 1920×1080 the centre sits on plates C1/D1/D2 and the notice is on screen in the capture that reads the sheet, so a centred box is OCR input the app wrote itself (ADR-019). Measured clearance to `panel_rect` on the one committed 1920×1080 frame: 41 px. The Settings row's **Show** checkbox is the toggle, and one drag puts the box anywhere the user wants it — the centre included, at which point it is their own placement and outranks every default | arm **shipped** (0dde882 / eb760c2); the START-phrase cycle, the indefinite `AlvaStart` arm and `waiting_for_panel` **shipped** (fa5bc61); 650 ms **shipped** (15eb3f8); the notice **shipped** (8d287e5) |
| 2 | **Sheet detected** (cheap tick anchors) | full read: anchor + 13 plates + panel + budget line + door markers, all regions keyed on the Entrance anchor (POE-230, 71df527). **Once per board identity** `(temple_epoch, temple_rearm, slice::BoardFrame)` — `LoopState::gate` answers `GateAnswer::Read` only for an identity this loop has not read. Regions that did not read cleanly (`slice::unclean`: unknown plate, unresolved or missing offer, marker error, unread budget — a region whose ROI was reported CLIPPED is exempt, it cannot improve) buy **at most `RETRIES` = 2 more** full reads, merged region by region (`slice::merge_reads` over `slice::KeptRead`: a clean region is never replaced by an unclean one, and the merge is refused unless both reads carry the same `layout_signature`); then **all OCR stops** — only the cheap presence tick continues. The 4 s periodic panel re-OCR is **gone**. | hide the info overlay; show the sheet-bound overlays (offer boxes with the cyan frame on the advisor's pick, POE-249) and the room overlay (POE-244/248: the room's outline; every corridor the read settled in the game's own colours, green open and red closed; the advisor's door purple and bigger; the door a SECOND Stone of Passage would buy in the same purple, smaller and at half opacity; and BOTH kills as cyan glyphs on the two architect icon spots, the block nobody chose at a quarter opacity — faint is the alternative, on both marks) | full read **shipped** (07cf80c, 71df527); read-once-per-board, the bounded retries and "no OCR after a clean read" **shipped** (15eb3f8); the offer boxes **shipped** (ee1f2c7) |
| 3 | **Sheet gone** (first missed cheap tick) | keep the cheap tick (armed by the panel tail, POE-246: 120 s from the last sighting) so a reopened sheet is noticed; a reopen whose sighting carries the SAME board identity re-shows the read at **zero OCR cost** (`TickOutcome::Reshown`, one `layout panel back — same board, no read` line). The identity is `(temple_epoch, temple_rearm, slice::BoardFrame)` — the anchor origin and scale inside a 2 px / 1 % band (`FRAME_ORIGIN_TOLERANCE`, `FRAME_SCALE_TOLERANCE_DENOM`) plus the exact `layout_signature` — so another room in the temple run, a moved frame or a corridor that has opened is a NEW board and is read. A frame that will not sit still is bounded by `GEOMETRY_READS_CAP` = 8 reads per board, after which the loop re-shows without OCR and says so once (`anchor origin keeps moving …`) | hide every sheet-bound overlay **on the first miss**; the **room overlay stays** (the player is inside the room, which is exactly when it is needed) | room-overlay persistence **shipped** (POE-248, b132e9b): `run::apply_gate` no longer drops the advice at stand-down, and `view.ts`'s `overlayShowsDoors` gates on the ADVICE plus a published room rather than on the status — so the widget also survives the stand-down itself, which on the live board landed mid-incursion (`12:39:05 capture stood down`). First-miss hide **shipped** (15eb3f8, `RETIRE_AFTER` = 1) — the STATUS already flipped on the first clean miss, so the overlays came down then; what one changed is that `LoopState::live`, the `layout panel gone` log line and the arm gate's view of the panel now agree with it. Re-show-without-OCR **shipped** (15eb3f8) |
| 4 | **Alva voice line again, or zone change** | stand down: stop capturing, clear all read state (`left_area_ms` stamp on zone change, POE-246; the Alva clear is POE-248). The same line also **ends the cycle**: `waiting_for_panel` goes down and `AppState.temple_epoch` is bumped (`trigger::ends_epoch` over `LineEvent`), which invalidates the board row 3 keys on. The END line's own tail arm therefore still lets a sheet reopened after it be READ rather than re-shown — the epoch moved, so it is a new board | clear and hide every overlay — the incursion is finished or the player died, which is finished either way | zone change **shipped** (0dde882); Alva-line clear **shipped** (POE-248, b132e9b): `trigger::advice_end` is the pure decision over the line — a `You have entered <not the temple>` line, or an `ALVA_SPEAKER` line stamped AFTER the read (the line that armed the read is spoken seconds before it, so an unconditional clear would blank the board the same line was the reason for reading) — and `slice::clear_advice` is the one writer. Read state other than the advice is kept: the Temple PAGE goes on showing the last board under its own timestamp, which is what it already does between reads. The cycle end and the epoch bump are **shipped** (fa5bc61) |

Consequences that follow from the order, not from extra rules:

- The waiting overlay appears only on a START phrase heard while `idle`. The end line clears;
  it does not show "waiting" again, and a late end line arriving after a zone change (3 of 342
  in the PC log) cannot start a cycle. The end line's arm still lets a reopened sheet be
  read (the loop is armed), silently.
- A death is a zone change (row 4). A sheet opened from the hideout with Alva silent is not in
  scope — **Re-arm** is the manual override (`temple_rearm`, 60 s), unchanged since POE-242.
- The keys setting (`temple_keys`) is GONE (POE-253): stones drop from the kill INSIDE the
  incursion, after the sheet has been read, so the count was a prediction nobody could fill
  in. POE-248 item 9 (the faint second-stone door) is the second stone's answer.

Four residuals the rules above produce, all ACCEPTED with their answer named (POE-249, owner
decisions 1, 3 and 4 of the plan review):

- **A START with no incursion run** keeps the arm and the notice up until the zone changes or
  Alva speaks again. That is what an indefinite arm means; the answer is the zone change, which
  every map ends with.
- **A missed END line** leaves a stale board on the next reopen — the epoch never moved, so the
  reopen re-shows instead of reading. It lasts only until the next line that DOES end the epoch
  (the next START, or the zone change out of the map), and **Re-arm** forces a read before that;
  the parked third "still in the incursion" signal is the eventual fix and is not this task. The
  mining below has no rate for a missed END — what it measured is the symmetric case, one
  incursion in 342 with no START line at all.
- **A kill taken mid-incursion** changes panel content that no `BoardFrame` can see: the origin,
  the scale and the `layout_signature` are all unchanged. The answers are the END line's own
  epoch bump and Re-arm (`BoardRead` says so at the type).
- **More than `GEOMETRY_READS_CAP` = 8 window drags inside one incursion**, with no Alva line and
  no room change between them, leave the overlay drawing the ROIs of the last read — a few px
  stale — until Re-arm or the next room. The alternative was 28 OCR calls every 650 ms for an
  anchor that will not agree with itself.

## Alva's lines, as measured (Client.txt)

PC log, 2026-01-29 → 2026-09-04 (mined on the PC 2026-09-04): **684 Alva lines across 144 map
instances**. Laptop (whole history, 9 lines) agrees on every line it has.

| line | PC count | role |
|---|---|---|
| `Time to go.` | 122 | start (portal opens) |
| `Let's go.` | 118 | start |
| `It's time!` | 101 | start |
| `Good job.` | 168 | end |
| `Good job, exile.` | 174 | end |
| `Just in time.` | 1 | ignore (no incursion followed) |
| `No wonder it's lost…` / `At last... Atzoatl.` | — | temple-zone banter, not a cycle event |

Facts that shape the rules (PC mining):

- The three start lines are used about equally — a phrase gate needs all three; any one alone
  misses two thirds of incursions. Starts and ends pair 341 : 342 (one orphan end).
- The start line fires **when the portal opens** (the Alva click), not when the player steps
  through. Start → end is typically ~34 s; 9 cases ran over two minutes and one **22 min** — the
  long ones are not long incursions but the player being away from the PC with the portal
  waiting (owner): nothing in the game times out an open portal, so the gap between the start
  line and entering is **unbounded**. The arm must therefore hold until an end line or a zone
  change, never a fixed burst — the panel-on-screen clock (POE-246) and the incursion context
  (POE-248) are what do that.
- **End lines can arrive after a zone change** (3 of 342: the player left the map mid-incursion
  and `Good job` fired seconds after re-entering). The zone change has already cleared the cycle
  by then; the late end line must NOT start a new one → **a cycle starts only on a known START
  phrase** (`Time to go.` / `Let's go.` / `It's time!`); **any** Alva line ends one. An unheard
  start variant costs a Re-arm (the existing fallback), never a false "waiting" overlay.
- One incursion in 342 had **no start line at all** — keep Re-arm.
- 1–3 incursions per map instance (81 instances had all three, never 4): the gate re-arms
  several times per map.
- `Time to go, exile.` does not exist in either log; the `, exile` variant is on the END line.
- Not verified against the wiki's canonical list (fetch blocked); rarer variants cannot be ruled
  out — a missed one degrades to Re-arm by the rule above.

## Cadences and budgets (measured numbers, where they exist)

- Cheap presence tick: `DETECT_INTERVAL` **650 ms, shipped** (15eb3f8). Cost: one monitor grab
  (225 ms on the laptop's debug build, `temple-debug/1788516327712`) plus one windowed
  correlation. The slow-machine backoff is unchanged — `DETECT_INTERVAL_SLOW` 3 s, fired once for
  the life of the thread by a cheap tick slower than `SLOW_TICK`, which is 1.5 s and is an
  ABSOLUTE CEILING rather than the cadence: a machine between 650 ms and 1.5 s runs at its own
  rate and is not backed off.
- The backstop `FULL_READ_EVERY_N_MISSES` is 30 cheap ticks, which is now **≈ 19.5 s** (it was
  30 s at 1000 ms) and 90 s once the backoff has fired. Since POE-249's gate split it forces the
  ANCHOR RESOLVE and NOT the 28 OCR calls — a backstop tick that re-anchors a board already read
  re-shows it — so the name overstates what it buys.
- That shortening raised the COLD SWEEP's duty cycle on an uncalibrated screen with no panel on
  it, from ~18 % to **~27 %** (5.3 s of pyramid sweep per 19.5 s). Accepted: it is bounded by the
  arm window, and the sweep is what gets a cold screen its first board.
- Full read: anchor+doors 1.5 s, panel OCR 1.9 s, plate OCR 0.7 s on the laptop's DEBUG build
  (same dump); release is faster.
- Cold start (no remembered scale): the pyramid sweep, 5.3 s in the release container (POE-234,
  29ac1b9), never the 348 s exhaustive sweep.
- Tails: `ALVA_TAIL_MS` = `PANEL_TAIL_MS` = 120 s; `MANUAL_ARM_GRACE_MS` 60 s (`trigger.rs`).

## Where each rule lives

| Rule | Home |
|---|---|
| what arms / disarms, the three clocks | `temple/trigger.rs` (`arm_source`, `ArmState`, `ArmReason`) |
| what ONE Client.txt line means — the area parse, the speaker match, the staleness gate and the three START phrases, decided once | `temple/trigger.rs` (`classify` → `LineEvent`, `ends_epoch`) |
| tick order: prune → hint → cheap detect → sweep gate → promote → full read → publish | `temple/run.rs` (`tick`, `wants_full_read`, `full_read`, `SweepGate`) — the ANCHOR gate, unchanged by POE-249 |
| the OCR gate: read this board, re-show it, or re-show it capped | `temple/run.rs` (`LoopState::gate` → `GateAnswer`, `LoopState::reshow`, `BoardRead`, `GEOMETRY_READS_CAP`) |
| board IDENTITY — is what I am looking at the thing I already read | `temple/slice.rs` (`BoardFrame`: the anchor origin and scale in a banded form plus `layout_signature`, the semantic half) and the `(temple_epoch, temple_rearm)` key |
| the retry merge | `temple/slice.rs` (`KeptRead`, `merge_reads`, `unclean`) and `temple/run.rs` (`kept_for`) |
| the re-arm counter (all that is left of the old read gate) | `temple/slice.rs` (`RearmGate`) |
| what INVALIDATES a board vs what FORCES a read | `AppState.temple_epoch` invalidates a board already read; `AppState.temple_rearm` forces one read with nothing sighted (`lib.rs`, both fields carry the invariant) |
| which overlay shows on which status / context | `desktop/src/lib/temple/view.ts` — `overlayShowsBoard(status)` for the sheet-bound surfaces, `overlayShowsDoors(slice)` for the room widget, which since POE-248 reads the ADVICE and not the status, and `overlayShowsWaiting(slice)` for the notice, which reads `waitingForPanel` AND the absence of a board |
| what each offer box says | `desktop/src/lib/temple/view.ts` (`offerBoxes` → `OfferBox`, one per panel block in the panel's own order) |
| where the notice ships and where the boxes are drawn | `desktop/src/lib/temple/overlay-geometry.ts` (`waitingDefaultPlacement` for the notice's offered default, `offerStackPlacement` for the column in the sheet's left margin) |
| what ENDS the advice (and with it the room widget) | `temple/trigger.rs` (`advice_end`) decides, `temple/slice.rs` (`clear_advice`, `force_off`) writes |
| never-cover set and placement | `temple/run.rs::read_rois` → `layout.rois`; `desktop/src/lib/temple/overlay-geometry.ts` (ADR-019) |
| smoke items per rule | `OVERLAY-GUIDE.md` "Windows smoke checks" |

## Owner decisions this encodes (2026-09-04)

"Sheet presence should be every 1 s, as sometimes 4 s is the time the user already finished the
temple — and if that presence test is cheap, I'd even go every 650 ms." "After the Alva voice
line (doesn't matter which one) or zone change, we clear the overlays and hide them, as the user
has finished the incursion or died." "When the user starts playing, the cheap sheet detect notices
the sheet is gone; we hide the info overlay, and only the room overlay stays."

One deviation from those words, recorded rather than resolved silently: the notice ships at the
TOP centre, not the screen centre, because a centred box covers plates C1/D1/D2 in the very
capture that reads them (ADR-019). It is placeable, so the centre is one drag away and is then
the user's own placement.
