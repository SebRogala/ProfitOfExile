---
uid: 47bf2d92-138d-41b3-941c-6e9b39265528
---

# ADR-028: Boss Loot Capture and Ownership

## Status

Accepted; implemented in POE-317. Last verified 2026-11-02.

## Enforcement

Tier: required

Scope: Boss loot native capture, confirmation, profile validation and server persistence boundary.

Summary: Rust owns capture identity, profiles, effective selections and submission; webviews project native state; the server stores versioned device-owned observations.

## Context

The confirmation widget and main page share a pending capture, while OCR and
HTTP completion can outlive the action that started them. A webview countdown
must not create another submission authority. ADR-014 covers module enablement
and SSOT delivery, but does not define this capture-to-observation boundary.

## Decision

`loot_capture::LootOwner` owns the capture sequence, area attribution, pending
selection, native deadline and submission phase. Async read and submit results
apply to their capture identity; cancelled module work cannot publish into a
replacement capture. `loot_profiles` owns the built-in pools, matching and
selection validation. The generated `loot_item_catalogue.json` supplies the
exchange-backed item identities used by those profiles.

Webviews read `ssot.bossLoot` and send typed actions to Rust. The displayed
countdown advances locally from native time; Rust decides when a run is due
and performs submission through `loot_api`. A selected item can be unmarked;
the effective selection is validated before saving.

The Go `bossloot` repository owns durable observations and optimistic row
versions. It validates the storage shape and counted membership; it does not
reimplement the native loot pools. The HTTP boundary requires device
identification, which is the implemented ownership mechanism, not a claim of
cryptographic device authentication.

League admission, cumulative historical reads and owned historical mutations
remain governed by [ADR-009](009-league-scoped-repository-convention.md).
Retention remains governed by [ADR-011](011-wipe-the-outgoing-league-at-rollover-preserve-it-as-a-dump.md).

## Consequences

The overlay is a projection rather than an independent save loop. Server
unreachability is shown as a run not recorded; a successful native save uses
the returned stored run. Profile versions separate changed pools without
discarding earlier observations.

The [Boss loot lifecycle](../BOSS-LOOT-LIFECYCLE.md) records transitions, API
routes and the import procedure. Implementations: `loot_capture.rs`,
`loot_profiles.rs`, `loot_api.rs`, `boss-loot/native-adapters.ts`,
`internal/server/handlers/boss_loot.go` and `internal/bossloot/repository.go`.
