# Temple module lifecycle — arming, detection, OCR, and what the overlay shows when

**Status:** normative design, owner-ordered 2026-09-04 (POE-249); **implemented 2026-09-04
(POE-249): fa5bc61 (the trigger), 15eb3f8 (the loop), 8d287e5 (the waiting notice), ee1f2c7
(the offer boxes)**. Every line is now tagged
**shipped** with the commit that shipped it; nothing here is planned any more. **Amended
2026-09-06** (owner): the slow-machine backoff is retired, the loop probes at its one cadence
straight from the arm, every read and every slow tick write a measured line, and the budget this
whole document serves is stated where it belongs — "Cadences and budgets" and "Owner decisions".
**Amended 2026-09-07 (WI-1, owner)**: the capture has no tails left. `ALVA_TAIL_MS` and
`PANEL_TAIL_MS` are retired; a non-START Alva line and a zone change stand the capture DOWN at
once (the temple's own banter excepted), and once a board has been READ the first miss after it —
the sheet closing — ends the cycle and the loop stops capturing. Rows 1, 3 and 4, the
consequences, the residuals and the tails bullet below carry the change; everything tagged
2026-09-04 or 2026-09-06 is history and is left standing.
**Amended 2026-09-07 (WI-2, owner)**: the retry rounds are PARTIAL. A board is still read at
most three times, but rounds 2 and 3 OCR only the regions the kept read left unclean, decided
before the round starts. Row 2, "Where each rule lives", "Cadences and budgets" and "Owner
decisions" carry it.
**Amended 2026-09-07 (POE-266)**: ADR-024 retires the in-game moved-frame and `window drags`
premise for module placement. The placed-origin verification and explicit fallback below
describe the current POE-269 behavior; the remaining exhaustive locate path is labeled
residual/debug-only.
**Amended 2026-09-11 (POE-275, owner)**: the sheet is gone on the SECOND consecutive missed cheap
tick, not the first. `RETIRE_AFTER` is 2, and the first miss over a live sheet is a held miss that
publishes nothing — it neither hides the sheet-bound overlays nor ends the cycle; the second does
both. This reverses the POE-249 row-3 rule and the WI-1 "one anchor miss" residual. Row 3, the
"for one tick" consequence, the residuals, the tails bullet and "Owner decisions" carry it; the
2026-09-07 sentence above about "the first miss after it" is history.
Read this before
touching `desktop/src-tauri/src/temple/{trigger,run,slice}.rs` or the temple overlay widgets.
Related: [Overlay Guide](OVERLAY-GUIDE.md) (windows, click-through, smoke items),
[ADR-014](adr/014-desktop-features-are-modules-with-a-work-toggle-and-a-view-page.md)
(the module contract; the POE-246 amendment note), [ADR-019](adr/019-nothing-a-module-draws-may-cover-what-that-module-reads.md),
[ADR-020](adr/020-one-shared-screen-scale-a-module-corroborates-or-withholds.md),
[ADR-022](adr/022-room-values-are-chaos-denominated-and-market-fed-presets-are-default-and-custom.md)
(what a room is worth, added 2026-09-06 by POE-257: `cd5627c`, `f722d49`),
[ADR-025](adr/025-a-capture-reads-once-re-reads-only-the-unknown-then-stops-only-a-manual-scan-moves-its-geometry.md)
(the capture contract rows 2–3 implement, shared with merc; POE-278).

## The one sentence

The capture runs only while something says an incursion is in scope, and stops the moment the
sheet it read closes; the sheet-bound overlays live with the temple **sheet** on screen; the room
overlay lives with the **incursion**; a full OCR read happens once per board, with bounded
retries, and then stops.

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
| 1 | **Alva START phrase** (`Time to go.` / `Let's go.` / `It's time!`) while `idle` — since 2026-09-07 (WI-1) it is the ONLY line that arms; any other Alva line stands the capture down (row 4) | arm the loop (`trigger.rs`): `trigger::classify` is the one owner of the per-line decision, and a START phrase arms `ArmReason::AlvaStart` with **no deadline** — the portal wait is unbounded (see the mining below), so the arm holds until an END line, a zone change or the completed cycle of row 3. Walking into the temple **assigns** `ArmReason::TempleArea`, so the banter inside cannot cut the arm short. Cheap presence tick every **650 ms** (`DETECT_INTERVAL`: screen grab + one windowed NCC at the placed Entrance origin from SSOT placements) | the **"waiting for the temple panel"** notice, as the PLACEABLE widget `temple.waiting` — gated on `view.ts`'s `overlayShowsWaiting` (`waitingForPanel` and no board, so a START heard with the sheet already open never blinks it). Its shipped default is **top-centre**, `{830, 16, 260, 40}` CSS px, NOT the screen centre the owner asked for: at 1920×1080 the centre sits on plates C1/D1/D2 and the notice is on screen in the capture that reads the sheet, so a centred box is OCR input the app wrote itself (ADR-019). Measured clearance to `panel_rect` on the one committed 1920×1080 frame: 41 px. The Settings row's **Show** checkbox is the toggle, and one drag puts the box anywhere the user wants it — the centre included, at which point it is their own placement and outranks every default | arm **shipped** (0dde882 / eb760c2); the START-phrase cycle, the indefinite `AlvaStart` arm and `waiting_for_panel` **shipped** (fa5bc61); 650 ms **shipped** (15eb3f8); the notice **shipped** (8d287e5); START-only arming 2026-09-07 (WI-1) |
| 2 | **Sheet detected** (cheap tick anchors) | full read: anchor + 13 plates + panel + budget line + door markers, all regions keyed on the Entrance anchor (POE-230, 71df527). **Once per board identity** `(temple_epoch, temple_rearm, slice::BoardFrame)` — `LoopState::gate` answers `GateAnswer::Read` only for an identity this loop has not read. Regions that did not read cleanly (`slice::unclean`: unknown plate, unresolved or missing offer, marker error, unread budget — a region whose ROI was reported CLIPPED is exempt, it cannot improve) buy **at most `RETRIES` = 2 more ROUNDS, so three in total**. **Amended 2026-09-07 (WI-2, owner):** rounds 2 and 3 are **PARTIAL** — they OCR only what the kept read still has unclean, decided by `slice::plan_read` from that kept read *before* any of the round's own OCR: the plates whose kept identity is not `is_known()`, the text regions the kept panel still owes (architect count below `panel::ARCHITECTS_PER_PANEL`, an offer whose target is unresolved, or a missing `incursions_remaining` — each honouring the same CLIPPED exemption, so a region off the capture is neither bought nor re-cropped), and the door diamond only on a kept `marker_error`. `plan_read` and `unclean` are one set of per-region predicates asked two ways, so "worth another round" and "what that round reads" cannot drift. Every round is merged region by region (`slice::merge_reads` over `slice::KeptRead`: a clean region is never replaced by an unclean one, and the merge is refused unless both reads carry the same `layout_signature`), and a region the round did not look at comes through as the KEPT value — that is an explicit contract in `merge_reads`, not a side effect of "unclean loses": a skipped plate is `Match::Unknown`, a skipped text region leaves an empty `PanelReading`, and a skipped diamond is `settled: None` with no `marker_error`, which the door arm now keys on the door SET rather than on the error so it cannot read as "looked and found nothing". WI-2 makes exactly two edits to this machinery: the merge's door arm is re-keyed off the door SET rather than off the error (above), and `run::kept_for` is made generic over what it is handed so `plan_read` can borrow the kept read before the merge takes it. `merge_reads`' semantics for the regions that WERE read, `kept_for`'s predicate, the anchor resolve, the board key/`BoardFrame` gate and `LoopState::note_read` are unchanged. After round 3 **all OCR stops** — only the cheap presence tick continues. The 4 s periodic panel re-OCR is **gone**. | from the anchoring tick until the read publishes, the room widget draws a muted `reading…` line (POE-276) — alone on an incursion's first read, under the previous room on a re-read; then hide the info overlay; show the sheet-bound overlays (offer boxes with the cyan frame on the advisor's pick, POE-249) and the room overlay (POE-244/248: the room's outline; every corridor the read settled in the game's own colours, green open and red closed; the advisor's door purple and bigger, carrying the NAME of the room it opens into and nothing else on the widget does (POE-261: the far plate's own read tier, `slice::recommended_exit`; an unread plate gets no name); the door a SECOND Stone of Passage would buy in the same purple, smaller and at half opacity — or, when the move opens NOTHING, the **convenience door** in that same faint seal (owner, 2026-09-05: *"if all rooms have the connections, app doesn't suggest to open the doors anymore at all"*): the in-cluster corridor that most shortens the walk, ranked in `advisor/convenience.rs` by Entrance → Apex, then Entrance → the wanted rooms, then the wanted rooms → Apex, then the longest open loop, with RU's veto honoured and a merge corridor never taken (that is the ranking's answer); the two faint answers are exclusive by construction, so one seal carries both; and BOTH kills as cyan glyphs on the two architect icon spots, the block nobody chose at a quarter opacity — faint is the alternative, on both marks) | full read **shipped** (07cf80c, 71df527); read-once-per-board, the bounded retries and "no OCR after a clean read" **shipped** (15eb3f8); the offer boxes **shipped** (ee1f2c7); the convenience door **shipped** (7c725e7) |
| 3 | **Sheet gone** (the SECOND consecutive missed cheap tick since 2026-09-11, POE-275 — the first until then) | **read and then closed → stand down** (2026-09-07, WI-1): `run::cycle_complete` is the rule — `DetectOutcome::Retired` while `LoopState::has_read` holds a read for the current `(temple_epoch, temple_rearm)` key — and `trigger::ArmState::complete_cycle` writes it, so the loop stops capturing entirely and `app.log` says `capture stood down — the sheet was read and closed`. **Amended 2026-09-11 (POE-275, owner):** `Retired` is the second consecutive clean miss (`RETIRE_AFTER` = 2); the first over a live sheet is `DetectOutcome::HeldMiss`, which keeps `LoopState::live` — and with it the arm gate — and completes nothing. A sighting between the two starts the count again; a failed grab does not touch it. A sheet closed BEFORE any read completed does NOT complete anything: no read is in hand, so the loop keeps probing on its arm. A tick whose screen GRAB failed completes nothing either — it learned nothing about the sheet. **And an `ArmReason::TempleArea` arm is not ended by a completed cycle**, the same carve-out row 4 gives Alva's banter: inside The Temple of Atzoatl the sheet is opened and closed once per ROOM with nothing between the rooms that moves the board key, so a completion on the first close would cost a Re-arm for every remaining room. That arm ends on leaving the area, as it always has. While the loop is still armed, a reopen whose sighting carries the SAME board identity re-shows the read at **zero OCR cost** (`TickOutcome::Reshown`, one `layout panel back — same board, no read` line). The identity is `(temple_epoch, temple_rearm, slice::BoardFrame)` — the anchor origin and scale inside a 2 px / 1 % band (`FRAME_ORIGIN_TOLERANCE`, `FRAME_SCALE_TOLERANCE_DENOM`) plus the exact `layout_signature` — so another room in the temple run, a geometry change currently re-detected by the loop or a corridor that has opened is a NEW board and is read. The placed-origin path checks one fixed Entrance location per detect tick. Its first successful recheck logs `Temple: placed-origin recheck — NCC N, N ms`. A below-floor recheck on a placed board gets one explicit cold fallback per `(temple_epoch, temple_rearm)` key only under the Manual arm (Re-arm) — since 2026-09-09 AlvaStart and TempleArea buy none, because their first miss is the sheet not being open yet and the sweep blocked the loop 30–34 s per incursion on the debug build (see "Owner decisions"); if the sweep lands elsewhere, the contradiction and informational geometry-notice lines are logged, the module reads there and remembers the origin after a successful read. The exhaustive locate button remains residual/debug-only | hide every sheet-bound overlay **on the retiring miss** — the second consecutive one since 2026-09-11 (POE-275, owner): the held miss before it publishes nothing (`run::miss_publish`), so the status the last sighting wrote stays on the slice and the overlays with it; **on the first miss** until then. The **room overlay stays** (the player is inside the room, which is exactly when it is needed). Unchanged by WI-1 — both were already true on that miss; what changed is that nothing is looking afterwards, so **reopening the sheet later in the same incursion shows no offer boxes until Re-arm** (accepted, owner 2026-09-07) | room-overlay persistence **shipped** (POE-248, b132e9b): `run::apply_gate` no longer drops the advice at stand-down, and `view.ts`'s `overlayShowsDoors` gates on the ADVICE plus a published room rather than on the status — so the widget also survives the stand-down itself, which on the live board landed mid-incursion (`12:39:05 capture stood down`). First-miss hide **shipped** (15eb3f8, `RETIRE_AFTER` = 1) — the STATUS already flipped on the first clean miss, so the overlays came down then; what one changed is that `LoopState::live`, the `layout panel gone` log line and the arm gate's view of the panel now agree with it. **Superseded 2026-09-11 (POE-275, owner):** `RETIRE_AFTER` = 2, and `run::miss_publish` answers nothing for the held miss where `miss` used to publish `NoPanel` on every clean miss — so the hide, `live`, the `layout panel gone` line and the completed cycle all move to the second consecutive miss together. Re-show-without-OCR **shipped** (15eb3f8). The cycle stand-down is 2026-09-07 (WI-1) |
| 4 | **Alva voice line that is not a START phrase, or zone change** | stand down **at once** (2026-09-07, WI-1): the arm goes to `Disarmed` on the line itself (`trigger::apply_line`), so the loop stops capturing on its next iteration rather than 120 s later. `ALVA_TAIL_MS` and `ArmReason::AlvaLine` are retired with it — a non-START Alva line never arms and never extends an arm. **One exception, unchanged:** inside The Temple of Atzoatl (`ArmReason::TempleArea`, assigned by `LineEvent::EnteredTemple`) Alva's banter leaves the arm alone; only the area line OUT ends that one. The same line also **ends the cycle**: `waiting_for_panel` goes down and `AppState.temple_epoch` is bumped (`trigger::ends_epoch` over `LineEvent`), which invalidates the board row 3 keys on. A sheet opened after this line is not read at all until Re-arm — the loop is not looking | clear and hide every overlay — the incursion is finished or the player died, which is finished either way | zone change **shipped** (0dde882); Alva-line clear **shipped** (POE-248, b132e9b): `trigger::advice_end` is the pure decision over the line — a `You have entered <not the temple>` line, or an `ALVA_SPEAKER` line stamped AFTER the read (the line that armed the read is spoken seconds before it, so an unconditional clear would blank the board the same line was the reason for reading) — and `slice::clear_advice` is the one writer. Read state other than the advice is kept: the Temple PAGE goes on showing the last board under its own timestamp, which is what it already does between reads. The cycle end and the epoch bump are **shipped** (fa5bc61). The immediate stand-down is 2026-09-07 (WI-1) |

Consequences that follow from the order, not from extra rules:

- The waiting overlay appears only on a START phrase heard while `idle`. The end line clears;
  it does not show "waiting" again, and a late end line arriving after a zone change (3 of 342
  in the PC log) cannot start a cycle. Since 2026-09-07 (WI-1) the end line no longer leaves an
  arm behind either: a sheet reopened after it is not read at all until **Re-arm**.
- **Between incursions nothing is looking.** That is the point of WI-1: the four ways the gate
  shuts (the cycle completing, a non-START Alva line, a zone change, Re-arm's grace running out)
  leave the loop capturing nothing, and the app log names which one — `capture stood down —
  the sheet was read and closed | Alva's line | the zone changed | Re-arm's grace is over |
  waiting for Alva` (`trigger::StandDown`, printed by `run::gate_line`, one line per stand-down).
- **A sheet on screen outranks all of it, for one tick.** `trigger::arm_source` reads
  `LoopState::live` — the loop's LAST detect tick found the panel — so a player holding the sheet
  open keeps the capture armed whatever Client.txt says, which is what lets a Re-arm read finish
  and retry past its 60 s grace. It is one tick (650 ms), not POE-246's 120 s tail, so a zone
  change carries at most one more capture into the next zone and that capture is the one that
  finds the sheet gone. **Amended 2026-09-11 (POE-275):** two ticks — `live` survives the held
  miss, so a zone change carries at most two captures into the next zone (1.3 s at the cadence),
  and the second is the one that retires the sheet; the sheet-bound overlays stay up through the
  first of them.
- A death is a zone change (row 4). A sheet opened from the hideout with Alva silent is not in
  scope — **Re-arm** is the manual override (`temple_rearm`, 60 s), unchanged since POE-242. A
  manual cycle ends by row 3 like any other: read the sheet, close it, and the loop stands down
  without waiting out the grace.
- The keys setting (`temple_keys`) is GONE (POE-253): stones drop from the kill INSIDE the
  incursion, after the sheet has been read, so the count was a prediction nobody could fill
  in. POE-248 item 9 (the faint second-stone door) is the second stone's answer.
- The faint seal has a SECOND meaning since 2026-09-05 (**shipped** 7c725e7): with no primary door — every corridor
  out of the room leads back into its own cluster, or RU declined them all — it marks the
  **convenience door**, the corridor to spend the key on for the walk (`advisor/convenience.rs`,
  `AdviceView.convenience`, the page's line under the top recommendation). The move itself
  still opens nothing and still says so (`NoUsableDoor` / `RuDeclined`, and the key still
  reads unspendable in `warnings`): the faint mark is what to do with a key the move has no
  use for, never the move.

**The read carries a valuation, and LOOKS IT UP** (POE-257, commits `cd5627c` + `f722d49`;
the lookup is WI-3, 2026-09-07). Row 2's full read does not only produce a board — it carries
ONE `valuation::Valued`, the 25 x 3 table of what each room-tier is worth in chaos, and hands
the same object to the ranking and to the offer boxes, so the number shown is the number ranked.
It is built from a `MarketInput` that arrives as state, never fetched: the 650 ms tick still does
no HTTP. Since POE-258 that state is real — `ssot::spawn_temple_market_poll` reads
`GET /api/analysis/temple-market` every five minutes and stores the payload, and the read takes
it through `ssot::temple_market_now`, which judges staleness against THIS tick's clock. A poll that
FAILS and a server that answers COLD both leave the last good read standing — neither says
anything about prices already in hand, and they age into stale on their own. What drops the
read is a payload keyed to another league or a server switch. So the board runs on grade rungs
rather than zero whenever no usable read is in hand: before the first poll answers, on a server
that has never priced anything, after a league mismatch or a switch, and on a read older than
two hours. What the player sees said about it is one line,
`view.ts::marketNote`, on the Temple page's Reader row and on every offer box — but about two
DIFFERENT markets, which is the correction of a shipped defect: the boxes say what THEIR read
was priced against (`TempleSlice::market`, written by `slice::project` alone) and the page says
what the next read will use (`TempleSlice::poll`, written by `ssot::publish_market_view` alone).
One field written by both let a poll put `prices 3 min old` over a board on the cold ladder, and
a DEBUG/PROD switch put `prices unavailable` over a priced one. The rules and
their homes are in the table below, the decision is [ADR-022](adr/022-room-values-are-chaos-denominated-and-market-fed-presets-are-default-and-custom.md).

**That table is built once per (preset, Custom table, market read), not once per read** (WI-3,
2026-09-07 — the owner's quote is in "Owner decisions this encodes" below). The accessor is
`ssot::temple_valuation_now(app, settings)`: it rebuilds a `preset::ValuationKey` from the
settings its CALLER hands it and the market standing now, and hands back the stored `Valued`
when the key is equal. **No READ reaches `preset::value_table` except through that accessor**;
the Temple page's preview of the OTHER preset computes it on the spot and never stores it (see
below), and it is the only direct caller left. That is the whole guard, and the reason
`slice::value_read` is now `#[cfg(test)]`: a read cannot be handed a table built from other
inputs even if an update path is missed. The settings arrive as an argument so the numbers and
the preset echoed beside them are always the same read's — `run::full_read` passes its own tick
snapshot and `commands::temple_value_table` passes the one snapshot it answered `in force` from,
so a `temple_set_preset` landing mid-read cannot cross the two. The update paths —
`ssot::warm_temple_valuation`, called off-thread from the market poll, from
`ssot::on_server_url_changed`, from `temple_set_preset` / `temple_set_custom` and from
`reset_all_settings` — exist only so that the read's call is a HIT, and a missed one costs that
read a rebuild rather than a wrong board. The build itself runs with the `temple_valuation`
mutex DROPPED — taken for the key comparison, released, re-taken to store — so a read arriving
while a warm-up builds waits for a key compare and never for the build; a lost race costs one
wasted rebuild and never a blocked read. The Temple page's own 25 x 3 table goes through the
same accessor for the preset in force (`commands::temple_value_table`), so the page and the
board are one object; the OTHER preset — the Copy button's "what would Default give me" — is
computed on the spot and never stored, because a probe that evicted the cache would make every
look at Default cost the next read a rebuild. The one input no update path can announce is the
market crossing `market::STALE_AFTER_MS` while nothing else moves: staleness is judged against
the calling clock, so the key flips on its own — the five-minute poll's warm-up normally reaches
it first, and at worst one read pays for it. What this bought: `advise` was **193-245 ms of a
666 ms read** on the PC's release build (`app.log` 2026-09-06 23:33) with the advisor's own
ranking — 13-22 ms of it, `advisor/mod.rs::the_conditional_ranking_cost_on_case_eight` — lumped
in. The read line now measures the two apart, and says which of the two the valuation was.

Nine residuals the rules above produce, all ACCEPTED with their answer named (POE-249, owner
decisions 1, 3 and 4 of the plan review; the WI-1 rows re-derived 2026-09-07, the failing-capture
residual added by WI-1's fix round the same day, and the retries-owed residual by the delivery
audit that found the list short of it, also 2026-09-07; the one-anchor-miss residual replaced
2026-09-11 by POE-275's two-miss rule):

- **A START with no incursion run** keeps the arm and the notice up until the zone changes, Alva
  speaks again, or the player opens the sheet and closes it — which under WI-1 is a third exit
  the indefinite arm did not have before. The answer is still the zone change, which every map
  ends with.
- **A reopen after the sheet was read and closed shows no offer boxes** until **Re-arm**, because
  the loop stood down when the sheet closed and nothing is looking. Accepted by the owner
  2026-09-07 as the price of not probing between incursions. The room diamond is unaffected — it
  lives with the incursion, not with the capture (POE-248). It does not apply inside The Temple
  of Atzoatl, which row 3 carves out.
- **A sheet that closes while retry rounds are still owed ends the cycle anyway**: an UNCLEAN
  first read loses the `RETRIES` = 2 more rounds row 2 buys it, because `run::cycle_complete` asks
  `LoopState::has_read`, which is `board.key == key` alone and never asks whether the board is
  `unclean` with `retries_left` above zero — the question `LoopState::gate` DOES ask on the same
  tick when it decides the OCR. It follows from the owner's *"once the full sheet is read once in
  the incursion, we stop reading the sheet … once the sheet is closed, we can already stop
  OCRing"* (2026-09-07, quoted in full below): a read that landed is a read, clean or not. The
  answer is **Re-arm**, which bumps `temple_rearm` so the read in hand is a read of ANOTHER key —
  `LoopState::note_read`'s first-look arm, a whole `RETRIES` budget and round 1 again. The
  [Overlay Guide](OVERLAY-GUIDE.md)'s retry smoke item says the same thing from the other side,
  as a precondition: keep the sheet OPEN for the whole check, because closing it ends the cycle
  instead of spending the budget.
- **Inside the temple the loop keeps probing for the whole run**, which is the one place WI-1
  deliberately did not stop it. The sheet is the navigation aid there and the run writes no line
  the gate could re-arm on, so the alternative was a Re-arm per room. The area line out ends it.
- **A missed END line** no longer leaves a stale board on the next reopen, because there is no
  reopen to serve: the cycle completed when the sheet closed, so the loop is stood down and
  **Re-arm** is the only way to a board. What a missed END costs instead is that the epoch never
  moves — the rearm counter is the other half of the key, so pressing the button still forces the
  read. The parked third
  "still in the incursion" signal is the eventual fix and is not this task. The mining below has
  no rate for a missed END — what it measured is the symmetric case, one incursion in 342 with no
  START line at all.
- **A closed sheet costs one more tick, and two misses over an open one still end the cycle**
  (POE-275, owner 2026-09-11). This replaces the WI-1 residual "one anchor miss over a sheet that
  is still open ends the cycle" (`RETIRE_AFTER` = 1), which no longer holds: one miss is now a
  held miss and changes nothing on screen. What the two-miss rule costs instead: a sheet the
  player really did close keeps its offer boxes up and the capture armed for one more 650 ms tick
  before the retire hides them and stands the loop down — and a failed tick's `error` and its
  message stand through a held miss the same way. And the loop still cannot tell TWO consecutive
  misses over an open sheet from a close: they end the cycle, the boxes do not come back, and the
  answer is the same Re-arm, bounded as before by the board already being published — what is
  lost is the boxes, not the advice or the room widget.
- **A kill taken mid-incursion** changes panel content that no `BoardFrame` can see: the origin,
  the scale and the `layout_signature` are all unchanged. The answers are the END line's own
  epoch bump and Re-arm (`BoardRead` says so at the type).
- **Placed-origin verification** checks the Entrance placement from the current
  screen slice once per detect tick. The first successful recheck logs
  `Temple: placed-origin recheck — NCC N, N ms`. A below-floor result on a
  placed board gets one explicit cold fallback per `(temple_epoch, temple_rearm)`
  key only under the Manual arm (Re-arm; since 2026-09-09 AlvaStart and TempleArea
  buy none — see "Owner decisions");
  a null or unplaced slice gets one cold-start fallback per key. If a fallback
  finds an anchor but its proposed slice is withheld, the null-slice key is
  released for one retry only; a second withheld sweep keeps it spent until the
  key changes. If the sweep lands elsewhere, the contradiction line and the
  informational geometry-notice line are logged, and the module reads there and
  remembers the origin after a successful read. The exhaustive locate button
  remains residual/debug-only.
- **A capture that fails on EVERY tick after a sighting keeps the gate open for as long as it
  keeps failing** (WI-1 fix round, 2026-09-07). A tick whose screen grab errored is not evidence
  about the sheet, so it leaves `run::LoopState::live` where the last tick that could SEE the
  screen left it (`LoopState::on_blind_tick`) — it only spends the start-up probe. While `live`
  is stuck true the panel branch of `trigger::arm_source` is ORed above Client.txt, so an Alva
  line or a zone change disarms the TRIGGER without shutting the gate: what ends it is the first
  grab that SUCCEEDS, which finds no panel, retires `live` and returns the loop to the ordinary
  rules (since 2026-09-11, POE-275, the second consecutive successful grab to find no panel — the
  first is a held miss; the failed grabs between them touch neither `live` nor the miss count).
  What it costs meanwhile is a screen grab attempt every 650 ms and no OCR, with the
  failure on the page (`last_error`) and one line in `app.log`. Accepted against the alternative
  it replaced, which was worse and far more likely: at `RETIRE_AFTER` = 1 one TRANSIENT grab
  failure retired a panel that was on screen, and with the panel the only thing holding the gate
  — a hideout read, or a Re-arm whose grace has run out under an open sheet — the capture stood
  down in front of the player with Re-arm the only way back. At `RETIRE_AFTER` = 2 (2026-09-11)
  the same fold would do it on two failed grabs, or one beside a real miss, so the trade stands.

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
  line and entering is **unbounded**. The arm must therefore hold until an end line, a zone
  change or the read sheet closing, never a fixed burst — the panel on screen (POE-246) and the
  incursion context (POE-248) are what do that.
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

- **The budget (owner, 2026-09-06): the verdict is on screen about 1 s after the panel opens.**
  Panel open → next cheap tick (0–650 ms) → one full read → publish → the overlay's next
  snapshot. **Measured 2026-09-06 on the PC, same code, same evening, one A/B on the build
  profile:** the release build read the sheet in **666 ms from grab to publish** (capture 35,
  cheap detect 64, anchor 96, text OCR 165, plates 102, markers 6, advise 193 — the pair
  POE-257 WI-3 then split and printed apart, the valuation being nearly all of that 193 and,
  since WI-3, a lookup) and the overlay showed it **+43 ms** later; the DEBUG `tauri dev` build
  read the same sheet in **4247 ms** (cheap detect 1042, anchor 1351, text OCR 1076, plates 496)
  with the overlay again at +44 ms — the "5–6 s verdict" of that session, tick wait included,
  was the build profile and nothing else. So the budget holds on release at the cadence (≤ 650 +
  666 + 43 ms worst case) and can not hold on a debug build. Nothing in the code asserts the
  budget — the two log lines below MEASURE it per read, and that is how a regression is found:
  in `app.log`, not in a test.
- Cheap presence tick: `DETECT_INTERVAL` **650 ms, shipped** (15eb3f8). Cost: one monitor grab
  (225 ms on the laptop's debug build, `temple-debug/1788516327712`; **52 ms on the PC's release
  build**, `temple-debug/1788567663863`) plus one windowed correlation. **There is no slow-machine
  backoff since 2026-09-06** (owner decision below). The retired one — `DETECT_INTERVAL_SLOW`
  3 s, sticky for the life of the thread after ONE cheap tick over 1.5 s — fired once on the PC
  (`2026-09-06 21:57:12`, 1519 ms: a screen-capture stall as a fight started, right after the
  sheet closed) and cost every later incursion of that session up to 3 s before the sheet was even
  seen. The loop is self-pacing, and it sleeps the interval AFTER the tick rather than around
  it — so a slow tick delays the next one by its whole duration and a machine that cannot hold
  650 ms runs at tick + 650 ms per detect; what that spends is CPU while armed, which POE-242
  bounds to Alva's window.
- **Measured, every time** (`run.rs`, `TickStages` / `ReadStages`): every read writes
  `Temple: read timings — capture N ms, cheap detect N ms, anchor N ms, text ocr N ms, plates N
  ms, markers N ms, valuation N ms (cached | computed), advise N ms, publish N ms — N ms from
  grab to publish, read at <unix ms>;
  round N of 3: full | re-read 3 plates, panel | re-read markers | nothing left to re-read;
  clean | unclean, N retries left`, and the temple overlay writes `[temple-overlay] board read at
  <unix ms> on screen +N ms (via nudge | poll | write)` against the same stamp
  (`routes/overlay/temple/+page.svelte`, `ssot.svelte.ts::lastSsotDelivery`). The first is the
  loop's half of the budget, the second the delivery's, and `via` answers whether the overlay is
  living on the `ssot-changed` nudge or on the 3 s poll — the question `ssot.rs`'s "overlays must
  still poll" note left open since 2026-07-24. **Answered 2026-09-06: `via nudge` on every read
  measured (5 of 5, +13 to +44 ms), on the debug and the release build alike.** The poll stays
  as the backstop it was meant to be; it is not what the overlay lives on. A cheap tick that
  overruns the cadence writes
  `Temple: slow detect tick — N ms (capture N ms, cheap detect N ms, anchor N ms); the cadence is
  650 ms`, at most one line per 10 s (`SLOW_TICK`, `SLOW_TICK_LOG_EVERY`): the stage breakdown is
  the point, because a stall in `capture` and a slow `cheap detect` are different problems with
  the same total. None of these lines changes what the loop does.
- The placed-origin recheck runs at the existing **650 ms** detect cadence and does one
  windowed NCC at the current placement. Its first successful result logs the NCC and elapsed
  milliseconds once per session. There is no automatic sweep cadence or moving-frame read cap:
  a null or unplaced screen slice gets one cold fallback per `(temple_epoch, temple_rearm)` key,
  and a below-floor placed miss gets one fallback per the same key under the Manual arm only
  (Re-arm, 2026-09-09). A sweep contradiction logs the
  placed-vs-swept line and the informational geometry-notice line; a null-slice sweep origin is
  also remembered after a successful read.
- The cold fallback remains the pyramid sweep (5.3 s in the release container, POE-234),
  entered only by those explicit recovery conditions or the residual/debug exhaustive button.
- Full read: anchor+doors 1.5 s, panel OCR 1.9 s, plate OCR 0.7 s on the laptop's DEBUG build
  (same dump); **on the PC's release build 75 / 85 / 56 ms** (`temple-debug/1788567663863`,
  2026-09-05), so ~270 ms from grab to plates and the advisor adds 13–22 ms
  (`advisor/mod.rs`, `the_conditional_ranking_cost_on_case_eight`). A DEBUG build is 10–20× that
  on the same machine — when a session reads slow, check which build ran before anything else.
- **A partial round costs only the regions it re-reads (WI-2, 2026-09-07).** Round 1 is the full
  28 OCR calls; rounds 2 and 3 pay 2 calls per still-unread plate, one crop per text region the
  kept panel still owes, and the diamond only on a kept marker error — so a board whose only
  fault was one covered plate spends 2 plate calls on round 2 instead of 28. **No figure is
  quoted here on purpose:** it is measured per round in the read line above, whose `text ocr`,
  `plates` and `markers` stages sit beside the `round N of 3: …` the round named, and the numbers
  are a property of the machine and the build (see the A/B at the top of this section). Read
  them out of `app.log`; do not assume them.
- Cold fallback: a null or unplaced screen slice spends the pyramid sweep once per
  `(temple_epoch, temple_rearm)` key; a below-floor placed miss may spend it once per
  the same key under the Manual arm only (Re-arm, 2026-09-09). A fallback that finds an anchor
  but is withheld releases the null-slice key for one retry only; a second withheld
  sweep keeps it spent until the key changes. The sweep is 5.3 s in the release
  container (POE-234, 29ac1b9), and the 348 s exhaustive sweep remains behind the
  debug button.
- Tails: **there are none since 2026-09-07 (WI-1)**. `ALVA_TAIL_MS` and `PANEL_TAIL_MS` (both
  120 s) are retired; `MANUAL_ARM_GRACE_MS` 60 s (`trigger.rs`) is the only deadline left in the
  module, and it is what a `Re-arm's grace is over` stand-down reports. What replaced the two
  tails is not a shorter clock but a state: the START arm is indefinite until row 3 or row 4, and
  a sheet on screen holds the gate through `LoopState::live` — one 650 ms tick, re-earned by
  every anchored tick, for any arm reason. Two ticks since 2026-09-11 (POE-275): `live` survives
  one held miss and goes on the second consecutive one.

## Where each rule lives

| Rule | Home |
|---|---|
| what arms / disarms | `temple/trigger.rs` (`arm_source`, `ArmState`, `ArmReason`, `TempleArm`) — a START phrase or the temple area arms with no deadline, Re-arm for `MANUAL_ARM_GRACE_MS`, and `LoopState::live` holds the gate from a sighting until the retire (`RETIRE_AFTER` = 2 consecutive clean misses since 2026-09-11, POE-275) |
| what SHUTS the gate, and the word the log uses | `temple/trigger.rs` (`StandDown` on `ArmState`, written by `apply_line`, `ArmState::arm_manual` and `ArmState::complete_cycle`) and `temple/run.rs` (`gate_line`, which prints it) — WI-1 |
| the completed cycle — read once, sheet closed, stop looking | `temple/run.rs` (`cycle_complete` decides, `LoopState::has_read` is its board half, `miss` calls it with the key the tick opened on, `board_key` builds that pair) and `temple/trigger.rs` (`ArmState::complete_cycle` writes, and holds both the `TempleArea` carve-out and the `key_current` guard — a tick takes seconds, so a START line or a Re-arm inside it arms a cycle this observation is not about; `trigger::complete_cycle` is the loop's seam into the lock and re-reads the pair there) — WI-1 |
| what ONE Client.txt line means — the area parse, the speaker match, the staleness gate and the three START phrases, decided once | `temple/trigger.rs` (`classify` → `LineEvent`, `ends_epoch`) |
| tick order: prune → hint from placements → placed-origin recheck → explicit fallback when qualified → full read → publish | `temple/run.rs` (`tick`, `cheap_hint_from_screen`, `cold_sweep_reason`, `cold_sweep`, `full_read`) — current POE-269 path |
| the OCR gate: read this board or re-show it | `temple/run.rs` (`LoopState::gate` → `GateAnswer`, `LoopState::reshow`, `BoardRead`) |
| board IDENTITY — is what I am looking at the thing I already read | `temple/slice.rs` (`BoardFrame`: the anchor origin and scale in a banded form plus `layout_signature`, the semantic half) and the `(temple_epoch, temple_rearm)` key |
| the retry merge | `temple/slice.rs` (`KeptRead`, `merge_reads`, `unclean`) and `temple/run.rs` (`kept_for`) |
| what a retry ROUND re-reads — 1 full + up to 2 partial | `temple/slice.rs` (`ReadPlan`, `plan_read`, and the one `retry_plan` predicate set `unclean` is the bool view of) decides; `temple/run.rs` (`full_read` asks it before any OCR, off the reading `kept_for` allows; `panel_text` takes the region names, `read_timings_line` prints the round) and `temple/panel.rs` (`read_slots`, which still reports all 13 slots) execute it. `merge_reads` is what a skipped region rides out on — WI-2 |
| the capture contract rows 2–3 and the Manual-only sweep implement — read once, re-read only the unclean, stop; a placed miss sweeps only under Re-arm | stated once, for temple and merc, in [ADR-025](adr/025-a-capture-reads-once-re-reads-only-the-unknown-then-stops-only-a-manual-scan-moves-its-geometry.md) (POE-278); the temple homes are the rows above and `run::cold_sweep_reason` |
| the re-arm counter (all that is left of the old read gate) | `temple/slice.rs` (`RearmGate`) |
| what INVALIDATES a board vs what FORCES a read | `AppState.temple_epoch` invalidates a board already read; `AppState.temple_rearm` forces one read with nothing sighted (`lib.rs`, both fields carry the invariant) |
| which overlay shows on which status / context | `desktop/src/lib/temple/view.ts` — `overlayShowsBoard(status)` for the sheet-bound surfaces, `overlayShowsDoors(slice)` for the room widget's diamond, which since POE-248 reads the ADVICE and not the status, composed by `doorWidget(slice)` with the widget's one status rule — a `reading…` line while the status is `reading` (POE-276) — and `overlayShowsWaiting(slice)` for the notice, which reads `waitingForPanel` AND the absence of a board |
| what each offer box says | `desktop/src/lib/temple/view.ts` (`offerBoxes` → `OfferBox`, one per panel block in the panel's own order) |
| where the notice ships and where the boxes are drawn | `desktop/src/lib/temple/overlay-geometry.ts` (`waitingDefaultPlacement` for the notice's offered default, `offerStackPlacement` for the column in the sheet's left margin) |
| what ENDS the advice (and with it the room widget) | `temple/trigger.rs` (`advice_end`) decides, `temple/slice.rs` (`clear_advice`, `force_off`) writes |
| never-cover set and placement | `temple/run.rs::read_rois` → `layout.rois`; `desktop/src/lib/temple/overlay-geometry.ts` (ADR-019) |
| what the recommended exit is CALLED, and where that name is drawn | `temple/slice.rs` (`recommended_exit` → `AdviceView.recommended_exit`, the ONE decision: the far plate's `RoomIdentity::display_name` at the tier THIS read gave it, `None` for a move with no door, an unresolved plate or more than one door) and `desktop/src/lib/temple/overlay-geometry.ts` (`diamondGeometry`'s `exitLabel` — pinned inside the shape's own box, on the `suggested` seal alone, costing the widget no height) |
| what one room-tier is WORTH, in chaos — sale + drops + bonus, the tier fraction, the drivers behind the number | `temple/valuation.rs` (`Valued::compute_with`, `RoomValue`, `Driver`, `Knobs`) — [ADR-022](adr/022-room-values-are-chaos-denominated-and-market-fed-presets-are-default-and-custom.md) |
| the market read a valuation is computed against, and what "stale" costs | `temple/market.rs` (`MarketInput`, `prices_anything`, `sale_delta`, `price`, `aged_at`, `STALE_AFTER_MS`) — mirrors POE-255's `GET /api/analysis/temple-market`; **no HTTP here**. The whole payload is in `ValuationKey`, so a price that moved invalidates the stored table |
| where that read comes from, how often, and what a wrong-league or unreachable server does to it | `ssot.rs` (`spawn_temple_market_poll`, `TEMPLE_MARKET_POLL` 5 min, `judge_market` → `MarketPoll`, `store_market`, `temple_market_now`, `on_server_url_changed`) — POE-258. A failed poll AND a cold server both KEEP the last good read and let it age — neither is evidence about the prices in hand; only a payload for another league and a server switch drop it to `MarketInput::none()` |
| what the player is told about the prices behind a number | `temple/slice.rs` (`MarketView`, `market_view`) on the wire, `desktop/src/lib/temple/view.ts` (`marketNote`, `marketStale`) in words — `prices 12 min old`, `prices stale (3 h) — base values`, `prices stale (3 h)`, `prices unavailable — base values`. The age AND the stale verdict are re-derived from `asOf` + the published `staleAfterMs` + the clock at render time, so a line ages without a republish. The `— base values` suffix is a claim about the NUMBERS and not about the age: it rides only when the read was ALREADY unpriced when it was valued (stale or unavailable at read time, so the board came off the cold ladder). A read that priced live and has since aged past the line says `prices stale (3 h)` alone — its figures are real prices gone old, and they become base values at the next read |
| WHICH market a surface is talking about | TWO fields, one writer each: `TempleSlice::market` is the read on screen (`slice::project` alone, from the very `MarketInput` that board was valued with) and every per-read surface — the offer boxes — reads it; `TempleSlice::poll` (`pollMarket` on the wire) is the latest poll (`ssot::publish_market_view` alone, on every poll and on a server switch) and the Temple page's reader row and value table read it. `project` seeds `poll` from the read because it replaces the whole slice, and that seed is exact — `run::full_read` takes its market from the same `ssot::temple_valuation_now` call that gave it the table, which reads `ssot::temple_market_now`. One field written by both let a poll re-label a standing board |
| the fallback when nothing is priced — the letter grade in chaos | `temple/rooms.rs` (`Grade::fallback_chaos` cold, `Grade::fallback_chaos_scaled` live) and `temple/valuation.rs` (`fallback_rung`, `lowest_summed_room_total`): a read that is not live values EVERY room at the cold ladder; on a live read a room that summed nothing hangs its letter off the LOWEST tier-3 total among the rooms that summed something, so no letter reaches a room the market priced (POE-262, ADR-022 §3 as amended). The two `INSTRUMENTAL_LINES` are the exception and keep their own anchor-and-cap — `instrumental_rung` + `lowest_sale_priced_room_total`, ADR-022 §4 |
| which valuation is in force, and the player's own numbers | `temple/preset.rs` (`Preset`, `TempleCustomSettings`, `value_table`) persisted as two separate `settings.rs` fields, `temple_preset` and `temple_custom`; one malformed entry costs its own room, never the table |
| the bridge from a chaos valuation to the advisor's profile, and the relative-unit rescale | `temple/slice.rs` (`TempleProfileSettings::to_profile`) and `temple/strategy.rs` (`REFERENCE_TOP_ROOM_VALUE`, `StrategyProfile::value_scale` — that doc's table is the normative list of the five scaled magnitudes) |
| ONE valuation per read, shown and ranked | the object `run::full_read` gets from `ssot::temple_valuation_now`, handed to `slice::advise_read` AND to `slice::project` → `OfferView.value` |
| where that object comes from, and when it is BUILT | `temple/preset.rs` (`ValuationKey`, `CachedValuation`, `ValuationSource`, and the cache decision as three pure functions — `cached_valuation`, `compute_valuation`, `store_valuation`) behind `ssot::temple_valuation_now(app, settings)` over `AppState.temple_valuation`; built on an input change by `ssot::warm_temple_valuation` (POE-257 WI-3) and never on the read path unless a key moved. Three functions and not one so the accessor builds with the slot mutex dropped. `slice::value_read` is `#[cfg(test)]` — no read may rebuild the table |
| smoke items per rule | `OVERLAY-GUIDE.md` "Windows smoke checks" |

## Owner decisions this encodes (2026-09-04, amended 2026-09-06, 2026-09-07, 2026-09-09 and 2026-09-11)

"Sheet presence should be every 1 s, as sometimes 4 s is the time the user already finished the
temple — and if that presence test is cheap, I'd even go every 650 ms." "After the Alva voice
line (doesn't matter which one) or zone change, we clear the overlays and hide them, as the user
has finished the incursion or died." "When the user starts playing, the cheap sheet detect notices
the sheet is gone; we hide the info overlay, and only the room overlay stays."

2026-09-06, after a session whose verdicts took 4–6 s: "On Alva voiceline, it should start cheap
OCRing to detect the panel, if it's detected, it should scan the layout and propose the verdict —
it should all close in about 1 s from the moment I open the panel." "If the cheap probe is really
cheap — we can just straight go to it, without the need to do the weird logic." "Let's add
logging measurements, so we know what is really going on." Encoded as: no backoff, one cadence
from the arm, and the three measured lines above. The 1 s is the owner's expectation of what is
possible, not a number the code asserts — the lines are how it is checked.

2026-09-07 (WI-1), after a session in which the loop kept probing after the incursion ended:
"After leaving the incursion, OCR never stops probing, and it should on any Alva voiceline, or
zone change." "Once the full sheet is read once in the incursion, we stop reading the sheet, we
only need the read whether we are still in incursion to keep the overlay open, but in fact, once
the sheet is closed, we can already stop OCRing, hide the explanation, keep the diamond overlay,
and upon stop encounter, we hide the diamond overlay." Encoded as rows 3 and 4 above: no tails,
a stand-down on any non-START Alva line or zone change, and a cycle that ends when the sheet the
loop has read closes. The reopen inside the same incursion is the accepted cost, and Re-arm is
its answer — the "still in the incursion" signal the second quote reaches for is the parked
follow-up, not this work item.

2026-09-07 (WI-2): *"We do up to 2 more rounds of temple reading, but only for the parts that
was previously not clear, so we in fact do 3 reading rounds total."* Encoded as row 2's partial
rounds: the budget (`RETRIES` = 2, three rounds in all) and the round numbering are unchanged —
what changed is what a round 2 or 3 reads. `slice::plan_read` derives it from the kept read
before the round OCRs anything, and shares its per-region predicates with `slice::unclean`, so
the parts that buy a round are exactly the parts that round re-reads. "Previously not clear" is
read as `unclean`'s own components, clipped exemptions included: a region that is off the capture
was not unclear, it was absent, and it neither buys a round nor is re-cropped by one.

One reading recorded rather than resolved silently: the completed cycle is NOT applied to an
`ArmReason::TempleArea` arm. The quote is about the timed incursion, and the same paragraph that
grants Alva's banter its temple exception says that arm "ends only on leaving the area, as
today"; applying the cycle there would have cost a Re-arm per room of the Temple of Atzoatl run,
where the sheet is the surface the run is played on. One condition in
`trigger::ArmState::complete_cycle` is the whole of it, if the owner wants the strict reading.

One deviation from those words, recorded rather than resolved silently: the notice ships at the
TOP centre, not the screen centre, because a centred box covers plates C1/D1/D2 in the very
capture that reads them (ADR-019). It is placeable, so the centre is one drag away and is then
the user's own placement.

2026-09-07 (WI-3): *"What I expected from the advisor and prices, was that they are cached, and
computed on update, and module uses already computed weightings each time — I thought it was
cheap one."* Encoded as "The read carries a valuation" above: the table is a function of the
preset, the Custom table and the market read alone, so it is built when one of those changes
(`ssot::warm_temple_valuation`) and a read looks it up (`ssot::temple_valuation_now`). The key
is the guard and the warm-ups are the optimisation, not the other way round. The read line now
names the two costs apart — `valuation N ms (cached | computed), advise N ms` — because "is it
cached?" was a question `app.log` could not answer while the two were one number, and the owner's
"I thought it was cheap one" is exactly the belief a single lumped field let stand.

- **R5 (leave the map) is withheld from every surface — 2026-09-07 (owner).** The rule as
  written gives false advice on live boards. The advisor still computes and tests it;
  `R5_WITHHELD` in `slice.rs` projects a `LeaveMap` verdict as `continue`, so the page and
  overlay banners never render. Lifting it is flipping that const and inverting its test.

2026-09-09, after three incursions whose first read waited on a sweep: *"I believe we decided to
remove the backoff from the temple reads, and keep default 650 ms ones after the Alva dialogue?
From the testing that I do it seems like it's not the case at all."* The backoff was gone
(2c1ec6b); what the log showed was POE-269's placed-miss fallback firing on the first tick after
Alva's start line, before the sheet was open — `armed 02:41:47`, `sweep found no layout panel
02:42:18`, `layout panel found 02:42:19`, 30–34 s each on the debug build, and no tick in
between. Encoded in `run::cold_sweep_reason`: a placed miss buys the pyramid sweep under the
Manual arm (Re-arm) only. Under AlvaStart and TempleArea the placed origin is trusted and the
650 ms recheck sees the sheet when it opens. The null-slice cold start is unchanged.

2026-09-11 (POE-275): *"RETIRE_AFTER goes from 1 to 2 - one missed probe is easy to get (a
misread, a tooltip over the plate) and must neither hide the sheet-bound overlays nor end the
cycle; two consecutive misses do both. Note miss() publishes NoPanel on the first clean miss
regardless of RETIRE_AFTER, so the constant alone does not deliver this. It reverses the POE-249
row-3 rule …"* Encoded as row 3's amendment: `RETIRE_AFTER` = 2 in `run.rs`, the first clean miss
over a live sheet answers `DetectOutcome::HeldMiss`, and `run::miss_publish` publishes nothing for
it — so the status the last sighting wrote stays on the slice, `LoopState::live` and the arm gate
hold, and `run::cycle_complete`, which keys on `Retired`, does not fire. The second consecutive
miss retires, hides and completes; a sighting between the two starts the count again. The cost is
one 650 ms tick on every sheet the player really did close (the residual above).
