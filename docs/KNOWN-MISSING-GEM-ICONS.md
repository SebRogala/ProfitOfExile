# Known Missing Gem Icons

Status: current tracking list. Update as icons are found/added.

The gem icon cache (`internal/gemicon`, served at `/api/gem-icon/{name}`) resolves
each gem name against `internal/gemicon/gem-icon-urls.json`. Gems **absent from
that map**, or whose mapped URL is stale, return `404` and render as the "?"
fallback in the UI. This list is what to chase later.

## Missing as of Allflame launch (2026-07-24)

**New Allflame transfigured skills** — brand-new, no entry in the map yet, and
poewiki likely has no icon file for them until the wiki catches up:

- `Divine Blast of Radiance`
- `Holy Hammers of Spirals`
- `Holy Sweep of Hammerfalls`
- `Reap of Butchery`

**Stale URL** — present in the map but the URL now returns an HTML page (moved /
renamed on the wiki), so the puller rejects it:

- `Dark Pact`

Everything else (755 of 756 map entries) is populated and serving.

## How to fix a missing icon

1. Find the gem's poewiki inventory-icon image URL (the direct
   `https://www.poewiki.net/images/<x>/<xx>/<Name>_inventory_icon.png` form, not
   the `Special:Filepath` redirect).
2. Add / correct the `"Gem Name": "<url>"` entry in
   `internal/gemicon/gem-icon-urls.json`.
3. Re-run the puller to download only the new/changed files (it skips ones
   already present), then repopulate the server's icon-cache volume from the
   output. The puller mirrors the server's exact cache filenames, so files drop
   in as cache hits — see the ops notes for the repopulate step.
4. Note: poewiki 403s datacenter IPs, so the server cannot fetch at runtime — the
   cache **must** be pre-populated from an allowed IP. This is why the puller +
   repopulate step exists rather than on-demand fetching.
