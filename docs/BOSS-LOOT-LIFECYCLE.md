# Boss loot lifecycle — capture, confirmation and stored observations

**Status:** current implemented behavior, POE-317.

**Last verified:** 2026-11-02 at `842d6a18`.

**Canonical for:** Boss loot transitions, user surfaces, API and seed import.

**Supersedes:** the duplicated boss-loot lifecycle notes in `desktop/src/lib/README.md`.

Ownership is accepted in [ADR-028](adr/028-boss-loot-capture-and-ownership.md).
League scope and retained observations follow [ADR-009](adr/009-league-scoped-repository-convention.md)
and [ADR-011](adr/011-wipe-the-outgoing-league-at-rollover-preserve-it-as-a-dump.md).
The module's work flag is `boss_loot`; its widget window is `loot`.

## States and transitions

`reading → loading → open → saving → saved | unreachable | rejected`

| Trigger | Current behavior | Shipped |
| --- | --- | --- |
| Module enabled | Native shortcut polling runs; stopping cancels work and clears pending capture, preview and rebind. Outside rebinding it reads only the two bound keys and Shift/Ctrl/Alt; while rebinding it reads every key, and its first rebinding read only records the keys already held; the same tick retires saved captures and preview frames past their time (desktop M3, 2026-11-12). | `139a67ba`, cancellation hardened in `24e9d6a1`, narrowed poll `870d03d6`, tick retirement `611f2327` |
| Area observed | The native owner records area, level and seed; startup catches up from the last area line. | `c656c6a7`, `83e1e14f` |
| Capture shortcut | Requires game foreground, a matching boss profile and resolved league; snapshots attribution, layout and counted set. | `139a67ba`, `83e1e14f` |
| OCR completes | Raw and preprocessed crop reads are combined; profile matches become incoming picks, ambiguous bases remain unresolved, unknown lines remain visible. | `139a67ba`, `24e9d6a1` |
| Latest run loaded | Same profile, league and nonempty seed selects the latest run as merge target; otherwise the next run number is offered. | `139a67ba`, `688f82d0` |
| User edits | Typed actions change native picks, quantity, force-new or discard; valid interaction restarts the native deadline. | `139a67ba`, `93b428e2` |
| Save now or deadline | Rust validates effective items, enters Saving and posts new/forceNew/merge; queued work remains bound to its capture id. | `139a67ba`, `5e35b2a3` |
| Save response | Success uses the stored run and closes after two seconds; unreachable shows that the run was not recorded; other failures are rejected. | `139a67ba` |
| Merge conflict | Re-read the latest run; a changed target pauses auto-save and offers save-as-new or discard. | `83e1e14f` |

`loot_capture::capture_rect` defines the cursor-relative crop; this manual
capture does not use a standing Temple/Merc placement. The `loot` window is
excluded while grabbing under [ADR-023](adr/023-a-window-excluded-from-the-grab-may-cover-what-its-module-reads.md).
Shipped: `139a67ba` (capture), `892d997a` (capture exclusion and module shell),
`93b428e2` (native overlay binding).

## Confirmation and profiles

The confirmation supports grid and list layouts. `loot.capture` is anchored;
`loot.confirmation` is placeable. Independent Show flags and Configure use the
shared widget host. Shipped: `4efedaef` (confirmation layouts), `892d997a`
(registry, Show flags and Configure integration), `93b428e2` (native binding).

**A7 — correction:** clicking an already selected unique unmarks it, including
an OCR selection. Required-group validation can then pause saving until the
selection is valid. The original OCR read tags remain provenance, not a second
selection. Shipped: `139a67ba`, `93b428e2`; the original-read projection shipped
in `502fb05c` and the A7 correction proof in `215f6854`.

**A10 — countdown:** `createOverlayClock` redraws every second from native
`nowMs` plus elapsed `performance.now()`. New snapshots recalibrate native time
without restarting that cadence. The displayed clock performs no submission.
Shipped: `12080814`, hardened in `b29d04d1`.

`loot_profiles::built_in_profiles` currently contains Catarina Normal version 2
(3.30 changed her pool: exactly one of three Allflame Embers per kill), matched
by area and level. Its profile owns the unique and Allflame Ember groups and
additional items; Communion Support defaults to not counted. Exchange-backed names and
keys come from `loot_item_catalogue.json`. Shipped: `c656c6a7`; the trimmed
current pool shipped in `41ec67ea` (A9a). These are implemented profile choices,
not independently verified game drop rates.

## Main page

Boss loot is beta-gated; its page is browsable independently of the work flag.
The layout injects native profile, stats, runs, settings and device adapters.
Shipped: native data binding `e7a3a2c1`.

**A8/A9b — Results layout:** the reusable `BossResultsTables` keeps icons beside
item names and preserves word boundaries. At a container width of 1100 px it
splits unique pools and additional drops into two tables using one shared
row/header renderer. Shipped: `8e0644a8`, `6e33a61d`, `9c39f313`, `db3e0682`.

| Surface | Current behavior | Shipped |
| --- | --- | --- |
| Results | Own/Collective statistics and per-item drops over eligible runs; `BossResultsTables` renders the projection. | `a43e6028`, `e7a3a2c1` |
| Runs | Own paginated history, versioned edit/delete and counted-set-preserving editor. | `9dbc97aa`, `e7a3a2c1` |
| Runs dates (A11) | Timestamp rows show local date and clock; date-only values stay date-only and null says `no date`. | `842d6a18` |
| Settings | Native module settings, shortcuts and not-counted choices; accepted state returns through SSOT. | `04451375`, `e7a3a2c1` |
| CSV export | Rust chooses the destination through a save dialog; cancellation is a separate result. | `b4e5e654` |

## Server API

Current routes shipped in `a5594665`:

| Route | Purpose |
| --- | --- |
| `POST /api/boss-loot/runs` | Submit new, forceNew or versioned merge. |
| `GET /api/boss-loot/runs` | List the identified device's cumulative profile history. |
| `PATCH /api/boss-loot/runs/{id}` | Edit owned items using expectedVersion. |
| `POST /api/boss-loot/runs/{id}/delete` | Delete an owned row using expectedVersion. |
| `GET /api/boss-loot/stats` | Own or collective cumulative profile statistics. |

All routes require device identification. Writes use the device write limiter.
The server validates positive quantities within the counted set and returns
409 for row-version or merge-target conflict. `StatsCumulative` derives each
item's denominator from runs whose stored counted set includes that item.
The server does not validate loot pools. Shipped: `a5594665`.

`boss_loot_runs` rows lose their device link after the period in [Device data retention](DATA-RETENTION.md).

## Seed import and release order

`cmd/boss-loot-import` reads the fixed `CatarinaRuns` workbook table; explicit
workbook, historical league and existing device arguments are required. Default
mode prints an anonymous preview without opening the database. `--apply` writes
all 78 runs in one transaction and refuses an existing device/profile history.
The importer preserves day precision in source metadata rather than inventing
an observed capture time. Shipped: `88929d35`.

[Deployment](DEPLOY.md#boss-loot-release-order) owns migration, import and
server/desktop release ordering.
Schema shipped: `a5594665`; importer shipped: `88929d35`.
