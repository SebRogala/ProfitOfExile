---
uid: 3ab72d40-74bd-4286-aaec-8981e5630087
---

# ADR-029: Overlay Widget Action Boundary

## Status

Accepted; implemented in POE-325 (`32721a27`, `cfa9b8bd`).
Last verified 2026-11-02.

## Enforcement

Tier: required

Scope: Forwarded overlay clicks, WidgetHost action dispatch and widget action registration.

Summary: WidgetHost owns hit mapping and hot rectangles; the shared registry resolves actions; each widget owns its action map.

## Context

The overlay hook forwards coordinates into a click-through window. Window
geometry and a widget's domain actions have separate owners. ADR-021 describes
the window/widget layout; its Proposed Windows-smoke status remains unchanged.
This decision records the delivered action boundary without promoting ADR-021.

## Decision

`WidgetHost.svelte` maps forwarded coordinates with `cssHotPoint` and
`elementFromPoint`, then hands the hit to `createWidgetActions().dispatch`.
The host owns placement and hot rectangles and provides `WIDGET_ACTIONS`.

`widget-actions.ts` resolves the nearest `data-action` and enclosing
`data-widget`, then selects that widget's registration. Each widget registers
its own action map; `WidgetActions.svelte` supplies route-level registrations.
The registry removes a registration only if the disposing registration still
owns that key.

Config mode suppresses dispatch, including interaction hooks. Otherwise the
widget's interaction hook runs before disabled-action suppression. The
inclusive walk from the original hit to the action target checks `:disabled`;
a nested child cannot bypass its disabled button. The action runs only when
the selected map owns the action key. Native config Save/Cancel remain outside
this widget-action path.

## Consequences

Adding a widget action changes that widget's map and metadata, with no
widget-specific dispatch branch in the host. Panel interaction can reset a
loot countdown even when the clicked Save button is disabled.

The [Overlay Guide](../OVERLAY-GUIDE.md) remains the home for window creation,
click-through setup, coordinate mapping and Windows verification.
