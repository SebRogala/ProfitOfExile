# ADR-013: UI Picks Persist in a Schema-less Prefs Map

## Status

Accepted

## Context

Every select the user can set — rankings sort mode, colour filter, row limit,
budget, the ALL-tab view, the Runs difficulty filter — should be set the same
way on the next launch. Before this decision, each persisted pick grew its own
plumbing: `show_low_confidence` has a typed Rust settings field, an AppState
mutex, and a dedicated get/set command pair; the web app used an ad-hoc
localStorage key for the same toggle. Most picks had no persistence at all,
and adding it per-pick at that cost meant it kept not happening.

Two kinds of persisted values were being conflated:

1. **Values Rust itself reads** — the market stamped onto uploaded font
   sessions, the lab mode, overlay geometry. These need typed fields: Rust
   logic branches on them, so a typo must fail at compile time.
2. **View preferences only the frontend reads** — sort mode, filters, limits.
   Rust never branches on them; it is purely a persistence host.

## Decision

View preferences persist through a single schema-less map:

- **Desktop**: `ui_prefs: HashMap<String, String>` inside the Rust settings
  file, exposed by exactly two commands — `get_ui_prefs` (whole map, fetched
  once) and `set_ui_pref` (write-through). Rust stores the map blindly; the
  frontend owns the keys.
- **Web**: the same interface backed by localStorage (`poe-pref-` prefix).
- Both apps consume it through a `persisted(key, initial)` helper in
  `$lib/prefs.svelte.ts` that returns a reactive `{ value }` usable directly
  in `bind:value`.

The boundary rule: **a typed `Settings` field is added only when Rust itself
reads the value.** Everything else goes in the map. Transient inputs (the gem
search query) persist nowhere.

Key naming: camelCase, surface-prefixed — `rankingsSort`, `rankingsColor`,
`rankingsLimit`, `rankingsBudget`, `rankingsTabView`, `runsDifficulty`.

## Consequences

- Adding a persisted pick is one `persisted()` call — no Rust change, no new
  command, no settings-struct edit.
- The map is schema-less by design: values are strings, and consumers must
  validate on read (the sort pref narrows through a whitelist with a fallback
  default). A stale or garbage value degrades to the default, never crashes.
- Desktop and web do not share storage; the same key can hold different values
  per app. Accepted — they are different installs with different habits.
- `show_low_confidence` predates this decision and keeps its typed field; the
  migration is not worth the settings-file churn. New view-only picks must not
  follow its pattern.
- The desktop map loads asynchronously after mount, so components start on
  defaults and snap to the stored value when the load lands (the same
  start-default-then-overwrite pattern the low-confidence toggle established).
  A pick changed before the load resolves wins over the loaded value.
