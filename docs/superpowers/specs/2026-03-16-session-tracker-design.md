# Lab Run Session Tracker — Design Spec (POE-51)

## Problem

During a lab run with 6-8 Font of Divine Skill uses, the user picks transfigured gems one at a time using the Comparator. After each pick, the Comparator resets and the previous selection is lost. By the time the user returns to hideout to sell, they've forgotten prices and can't compare what they got vs current market.

## Solution

Add a session queue below the Comparator that accumulates gem picks across a full lab run. Each pick snapshots the trade data at selection time. When ready to sell, the user refreshes all prices and gets updated recommendations with deltas.

## Workflow

1. User types 3 transfigured gems into Comparator
2. System auto-highlights BEST with green border
3. User can click a different card to override selection
4. User clicks **"Next"** → selected gem + trade snapshot goes to queue
5. All comparator slots clear, input focuses for next pick
6. Repeat 6-8 times → queue shows full sell list
7. Back in hideout: click **"Refresh Prices"** → system re-fetches trade data for all queued gems
8. Queue shows price delta (snapshot vs current) and updated recommendation badges
9. Queue auto-clears after 2 minutes of inactivity (configurable)

## Components

### Comparator Changes (minimal)

- **BEST auto-selection:** When `results` load and a gem has `recommendation: "BEST"`, auto-set it as `selectedForQueue`. Visual: green border (`border: 2px solid var(--color-lab-green)`) on the selected card.
- **Card click override:** Clicking any card sets it as `selectedForQueue`, removing the highlight from the previous.
- **"Next" button:** Visible when `selectedForQueue` is set. On click:
  1. Emit `onQueueGem(gem, tradeData)` callback to parent
  2. Clear `selectedGems`, `results`, `tradeData`, `tradeLoading`
  3. Focus input
- **No changes to recommendation logic, trade fetching, or card display.** The Comparator remains a pure comparison tool.

### SessionQueue.svelte (new component)

Renders below the Comparator. Receives queue state from parent page.

**Per-item display:**
- Gem icon + name + variant
- ROI at pick time
- Trade floor at pick time (with currency: "4 div" or "145c")
- Current recommendation badge (SELL NOW / UNDERCUT / HOLD / WAIT) — from latest data
- After refresh: price delta ("↑ +30c" green or "↓ -15c" red)
- Timestamp of when it was picked

**Queue actions:**
- **Refresh Prices** button: re-fetches trade data for all queued gems via `lookupTrade(gem, variant, force=true)`. Compares new floor to snapshot floor, shows delta. Resets auto-clear timer.
- **Clear Session** button: immediately clears entire queue
- **Remove single item:** × button per row

**Auto-clear timer:**
- 2-minute default (configurable 1-5 min via dropdown)
- Countdown visible in queue header ("Auto-clear in 1:45")
- Resets on: gem addition, refresh prices
- Does NOT reset on: remove single item, card click in comparator

### Page Orchestration (+page.svelte)

Queue state managed in page component:

```typescript
interface QueueItem {
    gem: string;
    variant: string;
    pickedAt: Date;
    snapshotROI: number;
    snapshotFloor: number;
    snapshotCurrency: string;  // "chaos" or "divine"
    snapshotDivineRate: number;
    currentFloor?: number;     // populated on refresh
    currentCurrency?: string;
    recommendation?: string;   // from compare API
}

let sessionQueue = $state<QueueItem[]>([]);
let autoClearMinutes = $state(2);
```

**Callbacks passed to Comparator:**
- `onQueueGem(gemName, variant, roi, tradeData)` — adds to queue if not duplicate

**Dedup:** Before adding, check `sessionQueue.some(q => q.gem === gem)`. If exists, skip silently (gem already in queue from a previous font pick).

## Data Flow

### Pick Flow (no new API calls)
```
User selects gem → Comparator already has trade data
  → Click "Next"
  → onQueueGem fires with current gem + tradeData snapshot
  → Page adds to sessionQueue (if not duplicate)
  → Comparator clears
  → Timer resets
```

### Refresh Flow (re-uses existing trade lookup)
```
User clicks "Refresh Prices"
  → For each queue item: lookupTrade(gem, variant, force=true)
  → Each lookup persisted to trade_lookups (automatic via gate)
  → Compare new floor to snapshot floor → delta
  → Fetch fresh compare results for recommendation badges
  → Update queue items with current data
```

## Timer Behavior

- Starts when first gem is added to queue
- Resets to full duration on: gem addition, refresh prices
- Counts down visually in queue header
- At zero: clears entire queue
- Manual "Clear" button clears immediately regardless of timer
- Timer config persisted in localStorage

## Styling

- Queue positioned directly below Comparator section
- Compact table layout: Icon | Name | Picked Price | Current Price | Delta | Rec | ×
- Green/red delta coloring
- Timer countdown in muted text in section header
- "Refresh Prices" button prominent, "Clear Session" button secondary
- Consistent with existing `--color-lab-*` variables

## What This Does NOT Include

- Price suggestion text (v2 — depends on POE-50 trend overlay maturity)
- Clipboard/export functionality
- Persistence across page reloads (session is ephemeral by design)
- Background auto-refresh (user-triggered only)
