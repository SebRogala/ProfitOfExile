# Temple module lifecycle — arming, detection, OCR, and what the overlay shows when

**Status:** normative design, owner-ordered 2026-09-04 (POE-249). Each line is tagged
**shipped** (on `main`, commit named) or **planned** (POE-249 / POE-248). Read this before
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

| # | Event | Capture / OCR | Overlay | Status |
|---|---|---|---|---|
| 1 | **Alva voice line** (either) while `idle` | arm the loop (`trigger.rs`, POE-242: speaker match, 120 s tail from the line's stamp); cheap presence tick every **650 ms** (screen grab + one hinted correlation at the remembered screen scale, ADR-020) | centre **"waiting for the temple panel"** info overlay, toggleable in config | arm **shipped** (0dde882 / eb760c2); 650 ms and the info overlay **planned** (today 1000 ms, no info overlay) |
| 2 | **Sheet detected** (cheap tick anchors) | full read: anchor + 13 plates + panel + budget line + door markers, all regions keyed on the Entrance anchor (POE-230, 71df527). Regions that did not read cleanly (unknown plate, unresolved offer, marker count mismatch, unread budget) are re-captured on following ticks **at most 2 more times**; then **all OCR stops** — only the cheap presence tick continues. No periodic panel re-OCR. | hide the info overlay; show the sheet-bound overlays (offer boxes with the cyan frame on the advisor's pick, POE-249) and the room overlay (POE-244/248: the room's outline, the open doors green, the advisor's door purple, and the kill as a cyan glyph on that architect's own icon spot) | full read + signature gate **shipped** (07cf80c, 71df527); bounded retries and "no OCR after a clean read" **planned** (today: panel text re-OCR every 4 s while the sheet is up, `PANEL_RECHECK_INTERVAL`) |
| 3 | **Sheet gone** (first missed cheap tick) | keep the cheap tick (armed by the panel tail, POE-246: 120 s from the last sighting) so a reopened sheet is noticed; reopening re-shows the same data, no new OCR unless the board signature changed | hide every sheet-bound overlay **on the first miss**; the **room overlay stays** (the player is inside the room, which is exactly when it is needed) | room-overlay persistence **shipped** (POE-248, b132e9b): `run::apply_gate` no longer drops the advice at stand-down, and `view.ts`'s `overlayShowsDoors` gates on the ADVICE plus a published room rather than on the status — so the widget also survives the stand-down itself, which on the live board landed mid-incursion (`12:39:05 capture stood down`). First-miss hide **planned** (today `RETIRE_AFTER = 2` ticks) |
| 4 | **Alva voice line again, or zone change** | stand down: stop capturing, clear all read state (`left_area_ms` stamp on zone change, POE-246; the Alva clear is POE-248) | clear and hide every overlay — the incursion is finished or the player died, which is finished either way | zone change **shipped** (0dde882); Alva-line clear **shipped** (POE-248, b132e9b): `trigger::advice_end` is the pure decision over the line — a `You have entered <not the temple>` line, or an `ALVA_SPEAKER` line stamped AFTER the read (the line that armed the read is spoken seconds before it, so an unconditional clear would blank the board the same line was the reason for reading) — and `slice::clear_advice` is the one writer. Read state other than the advice is kept: the Temple PAGE goes on showing the last board under its own timestamp, which is what it already does between reads |

Consequences that follow from the order, not from extra rules:

- The waiting overlay appears only on a voice line that **starts** a cycle (from `idle`). The end
  line clears; it does not show "waiting" again. The end line's arm still lets a reopened sheet be
  read (the loop is armed), silently.
- A death is a zone change (row 4). A sheet opened from the hideout with Alva silent is not in
  scope — **Re-arm** is the manual override (`temple_rearm`, 60 s), unchanged since POE-242.
- The keys setting (`temple_keys`) is orthogonal; POE-248 item 9 (the faint second-stone door)
  is what makes it unnecessary.

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

- Cheap presence tick: 650 ms **planned** (today `DETECT_INTERVAL` 1000 ms, `DETECT_INTERVAL_SLOW`
  backoff once a tick measured slow). Cost: one monitor grab (225 ms on the laptop's debug build,
  `temple-debug/1788516327712`) plus one windowed correlation.
- Full read: anchor+doors 1.5 s, panel OCR 1.9 s, plate OCR 0.7 s on the laptop's DEBUG build
  (same dump); release is faster.
- Cold start (no remembered scale): the pyramid sweep, 5.3 s in the release container (POE-234,
  29ac1b9), never the 348 s exhaustive sweep.
- Tails: `ALVA_TAIL_MS` = `PANEL_TAIL_MS` = 120 s; `MANUAL_ARM_GRACE_MS` 60 s (`trigger.rs`).

## Where each rule lives

| Rule | Home |
|---|---|
| what arms / disarms, the three clocks | `temple/trigger.rs` (`arm_source`, `ArmState`) |
| tick order: prune → hint → cheap detect → sweep gate → promote → full read → publish | `temple/run.rs` (`tick`, `wants_full_read`, `full_read`, `SweepGate`) |
| the read gate (signature-based "did the board change") | `temple/slice.rs` (`layout_signature`, `panel_signature`, `ReadGate`) |
| which overlay shows on which status / context | `desktop/src/lib/temple/view.ts` — `overlayShowsBoard(status)` for the sheet-bound surfaces, `overlayShowsDoors(slice)` for the room widget, which since POE-248 reads the ADVICE and not the status |
| what ENDS the advice (and with it the room widget) | `temple/trigger.rs` (`advice_end`) decides, `temple/slice.rs` (`clear_advice`, `force_off`) writes |
| never-cover set and placement | `temple/run.rs::read_rois` → `layout.rois`; `desktop/src/lib/temple/overlay-geometry.ts` (ADR-019) |
| smoke items per rule | `OVERLAY-GUIDE.md` "Windows smoke checks" |

## Owner decisions this encodes (2026-09-04)

"Sheet presence should be every 1 s, as sometimes 4 s is the time the user already finished the
temple — and if that presence test is cheap, I'd even go every 650 ms." "After the Alva voice
line (doesn't matter which one) or zone change, we clear the overlays and hide them, as the user
has finished the incursion or died." "When the user starts playing, the cheap sheet detect notices
the sheet is gone; we hide the info overlay, and only the room overlay stays."
