# Lab font OCR lifecycle — panel reads, craft rounds and gem reads

**Status:** current implemented behavior.

**Last verified:** 2026-11-13 at POE-397 WI-4 (`lanethird`, parent `bd765a01`).

**Canonical for:** Lab font OCR, round ledger and gem read transitions.

**Supersedes:** duplicated lifecycle and FontOpened-parity guidance in `desktop/src/lib/README.md`.

The game wording and craft-count semantics live in [Game facts](GAME-FACTS.md#divine-font).
Screen geometry follows [ADR-020](adr/020-one-shared-screen-scale-a-module-corroborates-or-withholds.md)
and [ADR-024](adr/024-module-ocr-positions-are-placed-from-the-screen-slice-not-located.md).
Every read follows [Screen reading (GGG policy)](OVERLAY-GUIDE.md#screen-reading-ggg-policy):
one frame per read key press, nothing read without one.

## Reader ownership

Two readers are registered on the read key (`read_hotkey::READERS`):
`LabFontReader` reads the options panel (`lab.font`) and `LabGemReader` reads
the gem-choice screen: the three slot icons (`lab.gem_slots`) and, for the
hovered slot only, the gem tooltip band (`lab.gem`). Client.txt events arm and disarm a context
for each; a press grabs one frame only when at least one context is armed, and
each armed reader consumes that frame once. There is no lab capture loop.

`lab_scan::LabScanState` holds the state: `read_token_seq` (the one token
issuer, never reset), `font_context` and `gem_context` (the armed tokens, 0 =
unarmed), `font_idle_seq`, `last_gem_read`, the gem vocabulary (tooltip matcher
and icon references) and its generation. A press
captures the token at arm time; every write path re-checks it under the guard
that owns the context (`font_session` for the font, `detected_gems` for gems),
so a press captured before a disarm can never write into a later context.

| Event | Transition |
| --- | --- |
| Final Izaro / LabFinished | Reset the parity counter; arm a fresh font context (session reset, alerts cleared), start its idle timer and the gem vocabulary preload. |
| FontOpened | Increment monotonic `font_opened_seq`; clear the font alerts; arm or disarm the gem context by parity; re-arm the font context when none is armed (portal return). |
| Read key press, font context armed | Crop `lab.font` from the one frame, parse the panel, replace the alert set; an active panel bumps the idle seq and passes nonempty options to the ledger. |
| Read key press, gem context armed | See [Gem read](#gem-read). |
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

## Gem read

`font_opened_effect` arms the gem context on an odd per-run FontOpened count
(CRAFT) and disarms it, clearing detected gems and emitting `gems-cleared`, on
an even count (CONFIRM). This is the implemented CRAFT/CONFIRM guess; opening,
closing and reopening the Font can disturb the parity. The font context instead
re-arms on an explicit token, and the craft ledger uses accepted panel counts.

A press while the gem context is armed reads that one frame (POE-397):

1. **Icons.** The three `lab.gem_slots` are cropped, resized to the 48 px
   canonical icon size and matched against the preloaded reference icons of
   both sets (`lab::gem_icons`). The set with the lower joint score (the sum of
   the three slots' best RMSE) is chosen; a frame never mixes sets
   ([Game facts](GAME-FACTS.md#gem-choice-screen)).
2. **Presence gate.** Above `GEM_PANEL_JOINT_RMSE_MAX` (85, provisional, its
   measurements in its doc comment) the frame is "no gem panel": logged,
   nothing written, no tooltip read.
3. **Certain or uncertain.** A slot is certain when its second-best reference
   scores at least `UNCERTAIN_MARGIN` (1.1) times its best, otherwise uncertain
   with its top 3 candidates. While the references are `incomplete` (some art
   failed to load, not a 404) every slot is uncertain.
4. **Hover.** Only when the press cursor lies on a slot, the `lab.gem` band of
   the same frame is OCR'd (Lanczos3 2×, then bilinear when that yields no
   candidates) and matched against the tooltip vocabulary; a match makes that
   slot certain with the tooltip's name. A slot a tooltip resolved stays
   resolved while later icon matches of it are uncertain.

`commit_gem_read` stores the read, merged over the context's previous read, in
`last_gem_read` and makes its certain names the held gems, only while the
press's token and the vocabulary generation are both still current. A changed
list emits `gems-cleared`, `gem-detected` per name and the status, and is sent
to the server; an unchanged one is only logged. Uncertain slots never reach the
held gems or the server; showing their candidates is deferred (no design). The
context stays armed after 3 certain slots, so a hover can still correct one; it
ends on CONFIRM, FontClosed or ZoneChanged.

The vocabulary preload (`lab::gem_vocab`) installs the tooltip matcher as soon
as the dictionary arrives; the icon references follow once their art is in
`<app_data>/gem-art/` (cache first, then `/api/icon/gems/{name}`). Without
them the icon step is skipped and logged once per context, and hover reads
still work. With no vocabulary current the read is skipped and logged once. A
lab-mode change bumps the vocabulary generation and clears an armed context's
gems and `last_gem_read`.

**Open question:** gem-read start/stop is under discussion with the Operator.
The parity behavior above records the current implementation and resolves no
future choice.

## Code homes

- `desktop/src-tauri/src/lab/watch.rs`: navigation/log handlers.
- `desktop/src-tauri/src/lab/font_reader.rs`: the font reader.
- `desktop/src-tauri/src/lab/gem_reader.rs`: the gem reader and its commit.
- `desktop/src-tauri/src/lab/font_context.rs`: font arm, disarm, read record, finalize and idle timer.
- `desktop/src-tauri/src/lab/gem_icons.rs`: slot icon matching, set choice, presence gate (pure).
- `desktop/src-tauri/src/lab/gem_icon_refs.rs`: reference icons from the dictionary and the gem art.
- `desktop/src-tauri/src/lab/gem_vocab.rs`: vocabulary and icon preload, generation checks.
- `desktop/src-tauri/src/lab/commands.rs`: gem arm/disarm, lab mode, gem submission.
- `desktop/src-tauri/src/lab_scan.rs`: state only.
- `desktop/src-tauri/src/font_session.rs`: effects, buffers, sealing and finalization.
- `desktop/src-tauri/src/font_ledger.rs`: count acceptance and round boundary.
- `desktop/src-tauri/src/font_parser.rs`: panel and count-line interpretation.

Retention of server-side `font_sessions` and `lab_runs`: see [Device data retention](DATA-RETENTION.md).
