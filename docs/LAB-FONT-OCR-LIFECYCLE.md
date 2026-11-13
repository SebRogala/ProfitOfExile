# Lab font OCR lifecycle — panel reads, craft rounds and gem reads

**Status:** current implemented behavior.

**Last verified:** 2026-11-13 at POE-391 WI-3 (`lanethird`, parent `badd6698`).

**Canonical for:** Lab font OCR, round ledger and gem tooltip read transitions.

**Supersedes:** duplicated lifecycle and FontOpened-parity guidance in `desktop/src/lib/README.md`.

The game wording and craft-count semantics live in [Game facts](GAME-FACTS.md#divine-font).
Screen geometry follows [ADR-020](adr/020-one-shared-screen-scale-a-module-corroborates-or-withholds.md)
and [ADR-024](adr/024-module-ocr-positions-are-placed-from-the-screen-slice-not-located.md).
Every read follows [Screen reading (GGG policy)](OVERLAY-GUIDE.md#screen-reading-ggg-policy):
one frame per read key press, nothing read without one.

## Reader ownership

Two readers are registered on the read key (`read_hotkey::READERS`):
`LabFontReader` reads the options panel (`lab.font`) and `LabGemReader` reads
the gem tooltip band (`lab.gem`). Client.txt events arm and disarm a context
for each; a press grabs one frame only when at least one context is armed, and
each armed reader consumes that frame once. There is no lab capture loop.

`lab_scan::LabScanState` holds the state: `read_token_seq` (the one token
issuer, never reset), `font_context` and `gem_context` (the armed tokens, 0 =
unarmed), `font_idle_seq`, the gem vocabulary and its generation. A press
captures the token at arm time; every write path re-checks it under the guard
that owns the context (`font_session` for the font, `detected_gems` for gems),
so a press captured before a disarm can never write into a later context.

| Event | Transition |
| --- | --- |
| Final Izaro / LabFinished | Reset the parity counter; arm a fresh font context (session reset, alerts cleared), start its idle timer and the gem vocabulary preload. |
| FontOpened | Increment monotonic `font_opened_seq`; clear the font alerts; arm or disarm the gem context by parity; re-arm the font context when none is armed (portal return). |
| Read key press, font context armed | Crop `lab.font` from the one frame, parse the panel, replace the alert set; an active panel bumps the idle seq and passes nonempty options to the ledger. |
| Read key press, gem context armed | See [Gem tooltip read](#gem-tooltip-read). |
| No font read that saw the panel for ten minutes | The idle timer finalizes the session (seal, send, reset) if its seq is still current. |
| ZoneChanged | Disarm the gem context (clearing comparator gems) and finalize the font context: seal leftover, submit, reset. |
| LabExited | Disarm the font context; nothing is sent. |
| FontClosed | Disarm the gem context, keeping held gems. |

Finalization (`lab::font_context::take_font_upload`) stores `font_context = 0`,
clears the alerts, seals the leftover round, builds the payload and resets the
session under one `font_session` hold, so an expiring timer and a late read
cannot both act. Re-arm opens a new segment; a town trip can produce two
session submissions for one lab run.

## Round ledger

`CraftLedger::observe` owns craft boundaries. Every count is accepted on its
first readable read: the first seeds the session, and a different count after
a new `font_opened_seq` seals the round. A count change with no new event is
ignored, as is an unreadable label; FontOpened alone seals nothing. A missing
count line means the last craft.

The boundary accepts changes in either direction. A garbled first count is
accepted too; `finalize_font_session` drops a ledger total outside 1..=20 and
reports the number of sealed rounds instead. `merge_options` unions the reads of
one round without replacing a known value with a later missing or disagreeing
value. On Seal, `apply_font_frame` moves the buffered options into a round and
opens the new round with this read's options. Finalization seals leftover
options and reports at least the number of sealed rounds as total crafts.

ZoneChanged and idle expiry submit accumulated rounds through
`POST /api/desktop/font-session`.

## Gem tooltip read

`font_opened_effect` arms the gem context on an odd per-run FontOpened count
(CRAFT) and disarms it, clearing detected gems and emitting `gems-cleared`, on
an even count (CONFIRM). This is the implemented CRAFT/CONFIRM guess; opening,
closing and reopening the Font can disturb the parity. The font context instead
re-arms on an explicit token, and the craft ledger uses accepted panel counts.

A press while the gem context is armed OCRs the `lab.gem` band of that one
frame with Lanczos3 2×, and once more bilinearly when that yields no
candidates. It matches against the preloaded vocabulary (`lab::gem_vocab`); with
none current the read is skipped and logged once. `commit_gem_name` adds a new
name only while the press's token and the vocabulary generation are both still
current; the third gem ends the context and sets the lab state Idle. A lab-mode
change bumps the vocabulary generation and clears an armed context's gems.

**Open question:** gem-read start/stop is under discussion with the Operator.
The parity behavior above records the current implementation and resolves no
future choice.

## Code homes

- `desktop/src-tauri/src/lab/watch.rs`: navigation/log handlers.
- `desktop/src-tauri/src/lab/font_reader.rs`: the font reader.
- `desktop/src-tauri/src/lab/gem_reader.rs`: the gem reader and its commit.
- `desktop/src-tauri/src/lab/font_context.rs`: font arm, disarm, read record, finalize and idle timer.
- `desktop/src-tauri/src/lab/gem_vocab.rs`: vocabulary preload and generation checks.
- `desktop/src-tauri/src/lab/commands.rs`: gem arm/disarm, lab mode, gem submission.
- `desktop/src-tauri/src/lab_scan.rs`: state only.
- `desktop/src-tauri/src/font_session.rs`: effects, buffers, sealing and finalization.
- `desktop/src-tauri/src/font_ledger.rs`: count acceptance and round boundary.
- `desktop/src-tauri/src/font_parser.rs`: panel and count-line interpretation.

Retention of server-side `font_sessions` and `lab_runs`: see [Device data retention](DATA-RETENTION.md).
