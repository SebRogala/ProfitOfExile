# Lab font OCR lifecycle — panel reads, craft rounds and gem scans

**Status:** current implemented behavior.

**Last verified:** 2026-11-02 at `842d6a18`.

**Canonical for:** Lab font OCR, round ledger and gem tooltip scan transitions.

**Supersedes:** duplicated lifecycle and FontOpened-parity guidance in `desktop/src/lib/README.md`.

The game wording and craft-count semantics live in [Game facts](GAME-FACTS.md#divine-font).
Screen geometry follows [ADR-020](adr/020-one-shared-screen-scale-a-module-corroborates-or-withholds.md)
and [ADR-024](adr/024-module-ocr-positions-are-placed-from-the-screen-slice-not-located.md).

## Scan ownership

`lab_scan::LabScanState` owns separate gem and font generations and the font
scan's explicit liveness token, `font_scan_live_gen`. `lab_scan::run` schedules
both on one OS thread and shares one capture per eligible tick. Each processing
state keeps its own timeouts and bookkeeping. Shipped: `7227d757`, state grouped
in `e8cddef5`.

| Event | Current transition | Shipped |
| --- | --- | --- |
| Final Izaro / LabFinished | Reset the parity counter and start font panel OCR. | `4d5c67d9`, `b62e4a80` |
| FontOpened | Increment monotonic `font_opened_seq`; independently choose gem effects by parity and re-arm a dead font scan. | `479b94ec`, `827191f3` |
| Eligible font frame | Crop/preprocess the font region, parse the panel, and pass active nonempty options to the ledger only while its generation is current. | `7227d757`, `e8cddef5` |
| No active font panel for ten minutes | Finalize accumulated session data and release that scan generation's liveness token. | `827191f3`; current timeout in `7227d757` |
| ZoneChanged | Stop gem/font generations, clear font liveness and parity, submit accumulated font data and clear comparator gems. | `827191f3`, `479b94ec` |
| LabExited | Stop font OCR even if no font event or ZoneChanged was observed. | `827191f3` |

The shared scan interval is 250 ms. Gem scanning also ends on its 150-second
timeout or after finding three distinct gems. These are code bounds, not
guarantees that OCR will recognize a tooltip. Shipped in the shared scheduler:
`7227d757`.
This automatic scan breaks [Screen reading (GGG policy)](OVERLAY-GUIDE.md#screen-reading-ggg-policy); POE-391 replaces it.

Manual `start_scanning` starts a fresh gem generation and clears comparator
gems; `stop_scanning` invalidates that generation. A replacement start also
invalidates the old scan. Shipped: `7227d757` retains these command boundaries.

## Round ledger

`CraftLedger::observe` owns craft boundaries. The first accepted count and later
changes require two readable confirming frames. A later count change also
requires a new `font_opened_seq`; FontOpened alone seals nothing. A missing
count line means the last craft, while an unreadable label is ignored. Shipped:
`479b94ec`.

The boundary accepts changes in either direction; a round visible for fewer
than two confirming frames can fold into a neighbouring round. `merge_options`
unions frame options without replacing a known value with a later missing or
disagreeing value. Shipped: `479b94ec`.

`apply_font_frame` buffers options during a pending count change. On Seal it
moves the old options into a round and carries the pending options into the
new round. A candidate that returns to the accepted count merges back into
the current buffer. Finalization seals leftover options and reports at least
the number of sealed rounds as total crafts. Shipped: `479b94ec`.

ZoneChanged submits accumulated rounds through `POST /api/desktop/font-session`
and resets the session. Idle expiry also finalizes it. Re-arm therefore opens a
new segment; a town trip can produce two session submissions for one lab run.
The liveness token is released by compare-exchange so an old scan cannot clear
its replacement. Shipped: `479b94ec`, `827191f3`.

## Gem tooltip start/stop exception

`font_opened_effects` starts gem scanning on an odd per-run FontOpened count
and stops it on an even count. The even handler clears detected gems and emits
`gems-cleared`. This is the implemented CRAFT/CONFIRM guess; opening, closing
and reopening the Font can disturb the parity. Font scan re-arming instead
uses explicit liveness, and the craft ledger uses accepted panel counts.
Shipped: `b62e4a80` (parity), `479b94ec` (effect extraction), preserved by
`7227d757` / `e8cddef5`.

**Open question:** gem-scan start/stop is under discussion with the Operator.
The parity behavior above records the current implementation and resolves no
future choice.

## Code homes

- `desktop/src-tauri/src/lab/watch.rs`: navigation/log handlers.
- `desktop/src-tauri/src/lab/commands.rs`: spawn and session submission.
- `desktop/src-tauri/src/lab_scan.rs`: shared capture scheduler and per-frame processing.
- `desktop/src-tauri/src/font_session.rs`: effects, buffers, sealing and finalization.
- `desktop/src-tauri/src/font_ledger.rs`: count acceptance and round boundary.
- `desktop/src-tauri/src/font_parser.rs`: panel and count-line interpretation.

Retention of server-side `font_sessions` and `lab_runs`: see [Device data retention](DATA-RETENTION.md).
