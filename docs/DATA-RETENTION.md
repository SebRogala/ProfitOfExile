# Device data retention — the purge job and its period

**Status:** current implemented behavior, POE-396.

**Canonical for:** how long rows stay linked to a device, and when devices are deleted.

Code: `internal/retention/purge.go`; schema: `internal/db/migrations/20261113140000_device_retention.up.sql`.

This job anonymises rows; it is not a league-data retention policy. League-scoped
hypertables follow [ADR-010](adr/010-archived-league-history-is-retained-indefinitely.md)
and [ADR-011](adr/011-wipe-the-outgoing-league-at-rollover-preserve-it-as-a-dump.md).

## Period

Two calendar years (`RetentionYears`): the cutoff is the same date two years back.

## Rows older than the cutoff

| Table | Age measured from | Cleared |
| --- | --- | --- |
| `font_sessions` | `time` | `device_id`, `pair_code` |
| `lab_runs` | `created_at` (server-set) | `device_id` |
| `boss_loot_runs` | `created_at` (server-set) | `device_id` |
| `merc_icon_templates` | `created_at` | `device_id` |

The rows themselves stay. `font_rounds` and `lab_run_rooms` have no device
column and keep their rows. The boss-loot collective "devices" count drops
anonymised runs; they stay in run and item totals.

## Devices

- A device not seen for two years (`last_seen` before the cutoff) and not
  banned is deleted, identified devices (alias or role) included. Its remaining
  rows in the tables above are unlinked first; `boss_loot_runs` follows via its
  `ON DELETE SET NULL` foreign key.
- A banned device not seen for two years keeps `fingerprint`, `banned`,
  `first_seen` and `last_seen`; `alias` and `app_version` are cleared and
  `role` is reset to `user`.

## Schedule

The server runs the purge at start and then every 24 h, on the fenced server
instance (`serviceCtx`), all statements in one transaction. A failed run is
logged and retried on the next tick.
