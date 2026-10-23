---
uid: fc898b2c-7a08-435c-9f47-5fc6e3293f54
---

# Trade Is the Primary Gem Price; poe.ninja Is the Fallback

## Status

Accepted (2026-10-14, owner during the POE-295 breakdown: "we should rather take trade data over PoeNinja"). Amends [ADR-007](007-v3-hybrid-analysis.md), which wired trade data into the feature layer only and named trade-primary pricing as future intent.

## Context

Gem tiers, Font EV, Dedication and Double-corrupt classify on the poe.ninja price. poe.ninja publishes lines built from a sample of 1–3 (`count`, empty sparkline). On 2026-10-14, 34 lines sat at exactly 1817c (5.0 div), among them Summon Flame Golem of the Meteor 20/20, which trades at ~14c. Those lines became TOP and inflated Font advice. The server trade gate already looks up ~1,100 gems a day, and desktop users submit lookups to the same history. That trade data only adjusted sellability, never the price that tiers and EV use.

## Decision

1. For each gem and variant, the price used before tier classification is a **trade price** when one is usable, otherwise the poe.ninja price. One shared loader feeds every analysis run (RunV2, Font, Dedication, Double-corrupt), so they cannot disagree.
2. The trade price is the **median of the cheapest ~5 listings** of the latest lookup, never the single floor listing.
3. A trade price is usable for **24h**. After that poe.ninja wins again, unless poe.ninja's sample is thin (`count` <= 5), in which case the last trade price keeps winning.
4. A gem whose poe.ninja sample is thin and that has never been trade-checked is **flagged low-confidence**: it takes no part in TOP detection or tier boundaries and contributes nothing to Font EV, but it stays visible with its flag (ADR-017/018).
5. Server-gate lookups and desktop-submitted lookups carry the **same trust**.
6. Everything is per variant.

## Consequences

- The trade refresh must cover every variant the analyses price (not only 20/20), and must reach gems it has never looked up (POE-298).
- The gate becomes load-bearing, so it must survive GGG rate limits and bans (POE-297).
- A gem's displayed price may now come from trade or from poe.ninja; the source must be visible.
- Eligibility and cutoff stay with the callers (ADR-026): the thin-sample flag is set in `internal/lab`, not in `internal/price/tier`.

## Alternatives rejected

- Mark every thin ninja line low-confidence on the existing path (EV = 0): 90 of 157 20/20 transfigured lines are thin late-league, so Font EV would collapse.
- A targeted CHAOTIC rule for thin lines priced above the top boundary: it treats the symptom and keeps ninja as the primary source.
- Ninja-history anomaly detection (POE-78): cannot tell a real price spike from a 1-sample artefact.
