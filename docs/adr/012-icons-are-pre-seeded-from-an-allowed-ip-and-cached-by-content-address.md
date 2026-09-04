---
uid: 5daeb06f-ad96-499d-a640-f66bac72b04c
---

# ADR-012: Icons Are Pre-Seeded from an Allowed IP and Cached by Content Address

## Status

Accepted. Decisions 1 and 2 describe current behaviour. **Decision 3
(content-addressed cache filenames) is accepted but not yet implemented** —
`safeFileName` still keys on the gem name alone. Until it lands, changing an
icon's URL also requires deleting the stale file from the production volume by
hand.

Amended 2026-08-19 (POE-177) — see the amendment at the end: a second icon set
now shares this mechanism, and one factual claim in Context below is corrected.

Amended 2026-09-01 (POE-221) — the two cache directories became sub-directories
of one volume. Layout only; every decision below stands. See the second
amendment at the end.

Amended 2026-09-04 (POE-135) — the gem set's source map became a directory of
category files merged at construction. Layout only; every decision below
stands. See the third amendment at the end.

## Context

The server serves gem and item artwork at `/api/gem-icon/{name}` from
`internal/gemicon`. Each name is resolved against a name→URL map and the image is
cached on disk so the same request never hits upstream twice.

Two facts constrain the design, and neither is visible from the code:

- **poewiki 403s datacenter IPs.** The production VPS cannot fetch icons at
  runtime at all. Verified again 2026-07-26: the same URL that a developer
  machine downloads without incident is refused from the server.
- **The map is compiled into the binary** (`//go:embed urls/*.json` in
  `internal/gemicon/gemicon.go`; one file per category since the 2026-09-04
  amendment, merged at construction). The handler looks a name up in that embedded
  map and returns 404 before it ever touches disk, so dropping a file into the
  cache volume does nothing for a name the binary does not already know.

Together these mean adding one icon is a two-step operation with an ordering
requirement: seed the cache volume from an allowed IP **first**, then deploy the
map. Seeding after the deploy leaves a window where the server knows the name,
cannot fetch it, and returns 502.

A third fact surfaced while adding six icons on 2026-07-26. `load()`
(`internal/gemicon/gemicon.go:145`) returns the disk copy unconditionally when
the file exists — no TTL, no ETag, no revalidation — and the cache filename is
derived from the **gem name** (`safeFileName`), not from the URL or the bytes.
When poewiki re-uploads an image its URL hash path changes, but the cache still
holds a file under the same name and serves it forever. Correcting the URL in the
map has no effect. Changing an icon therefore required three coordinated manual
steps: edit the map, delete the file from the production volume, redeploy.

That third step is invisible, easy to forget, and produces a silent wrong-artwork
failure rather than a loud one.

## Decision

**1. The icon cache is pre-seeded from an allowed IP; the server never fetches in
production.** `scripts/download-gem-icons.py` pulls every mapped icon using the
server's exact cache-filename scheme, and its output is copied into the
`GEM_ICON_CACHE_DIR` volume (`/data/gem-icons-cache` in production) — **that
variable and that volume are superseded by the 2026-09-01 (POE-221) amendment
below**, which replaces them with a `gems/` sub-directory of the single
`ICON_CACHE_DIR` root; the decision itself is unchanged. Runtime
fetching remains in the code because it is how a developer machine populates its
own cache; it is not a production path.

**2. Seed before deploy.** The cache volume is populated first, the map second.
The reverse order opens a 502 window.

**3. Cache identity is content-addressed by source URL.** The cache filename
includes a short hash of the mapped URL, so changing a URL in the map changes the
filename and misses the cache naturally. The map becomes the single source of
truth for which bytes a name resolves to, and updating an icon is a one-step
change again. Superseded files linger as garbage in the volume; they are pruned
by sweeping files whose hash does not appear in the current map.

## Consequences

- Adding or changing an icon is a code change requiring a deploy. This is
  accepted: the map is embedded, and embedding is what makes lookups allocation-free
  and keeps the icon set versioned with the binary that serves it.
- A developer machine still fetches on demand and populates its own cache, so
  local work needs no seeding step.
- An icon whose upstream URL breaks fails as a 502, not as stale artwork. That is
  the intended trade: a loud failure is preferable to silently serving art that no
  longer matches the game.
- The volume accumulates superseded files. Growth is bounded by how often upstream
  re-uploads, which is rare, and a prune is a pure function of the current map.

## Alternatives considered

**On-demand fetching in production.** Impossible — poewiki refuses the VPS. This
is the trap the ADR exists to document: the code path looks like it would work.

**Hashing the image bytes instead of the URL.** Correct invalidation, but it
cannot be computed without first fetching the bytes, which production cannot do.
The URL is the identity production can actually evaluate offline.

**A TTL on cached files.** Would revalidate on a timer, but every revalidation is
an upstream fetch the production server is not permitted to make.

**Reading the map from disk instead of embedding it.** Would let an icon be added
without a deploy, but splits the icon set from the binary that serves it and
introduces a new failure mode where the two disagree. Rejected in favour of the
deploy step.

## Amended 2026-08-19 (POE-177)

Status of this section: current behaviour, EXCEPT its cache-directory layout,
which the 2026-09-01 amendment below supersedes. The two environment variables
and the two volumes it names no longer exist; the requirement they served — one
directory per icon set — does, as two sub-directories of one volume. The old
names are kept here because they are what the production migration moves *from*.

**A second icon set shares this decision and its code.** Currency Exchange item
icons are served at `/api/currency-exchange/icon/{metadata id}` by the same
`internal/gemicon` cache, constructed through the exported
`gemicon.NewWithMap(urls, cacheDir)` over a second embedded map —
`internal/exchange/itemdata/items.json`, generated by
`scripts/generate-currency-exchange-items.py` and read out as
`exchange.IconURLs()`. `gemicon.New` is now a thin wrapper that loads the gem
map and calls the same constructor, so both sets get identical fetch-once,
serve-from-disk, ETag and cache-header behaviour.

Decisions 1 and 2 apply to it unchanged and for the same reason: poewiki 403s
the production VPS, so the volume is seeded from an allowed IP **before** the
map deploys. The puller is the existing one — the asset ships a flat
`id → URL` map for exactly this:

```
python3 scripts/download-gem-icons.py \
  internal/exchange/itemdata/icon-urls.json currency-exchange-icons-cache
```

The gem seeding steps do not transfer verbatim: they hardcode the gem directory
and the gem volume. The item set needs a second persistent volume, its own
`CURRENCY_EXCHANGE_ICON_CACHE_DIR`, and its own copy — see
[GEM-ICONS.md → Currency Exchange items → Pre-seeding production](../GEM-ICONS.md#pre-seeding-production)
for the four steps and the exact commands. **This is an operator step on
production, and it has no substitute** — an unseeded item icon is a permanent
502 there, not a slow first request.

**The two sets keep separate cache directories.** `GEM_ICON_CACHE_DIR` and
`CURRENCY_EXCHANGE_ICON_CACHE_DIR` (default
`./data/currency-exchange-icons-cache`, a persistent volume in production). The
filename scheme is shared and neither generator can see the other's keys, so one
directory would let a gem name and an item id that reduce to the same
`safeFileName` serve each other's artwork. `NewWithMap` therefore rejects an
empty cache directory rather than defaulting; only `New` keeps the
`DefaultCacheDir` fallback, and that default is the gem set's.

**Correction to Context: "When poewiki re-uploads an image its URL hash path
changes" is false.** MediaWiki derives the `/images/<h>/<hh>/` path from the MD5
of the image's **file name**, not of its bytes, so a re-upload under the same
name keeps the same URL and silently changes what that URL serves. The rest of
the paragraph stands — the cache does serve a stale file forever — but the
trigger is narrower than written: the mapped URL moves only when the **file
name** moves.

That narrows Decision 3 too, which is worth knowing before implementing it:
addressing the cache filename by source URL fixes the rename case and does
nothing for a same-name re-upload, because the URL the map holds is unchanged.
"Content address" in this ADR's title has always meant the URL, never the bytes
(the Alternatives section says why bytes are not available to production).
Nothing here changes Decision 3's status: still accepted, still unimplemented.

**Measured while wiring this up (2026-08-19):** a container on the development
host fetched `Chaos_Orb_inventory_icon.png` through the new route live from
poewiki — 200, a 78x78 PNG. The block is specific to the production VPS's IP,
which is what makes the seeding step easy to forget: every environment a
developer can test in works without it.

## Amended 2026-09-01 (POE-221)

Status of this section: current behaviour.

**One volume, one sub-directory per icon set.** The two cache directories are no
longer two independently configured volumes. There is a single root —
`ICON_CACHE_DIR`, default `./data/icons-cache`, `/data/icons-cache` in
production — and `internal/server` derives one sub-directory per set beneath it:

```
<ICON_CACHE_DIR>/gems               internal/gemicon/urls/*.json
<ICON_CACHE_DIR>/currency-exchange  internal/exchange/itemdata/icon-urls.json
```

`GEM_ICON_CACHE_DIR` and `CURRENCY_EXCHANGE_ICON_CACHE_DIR` are removed, with no
aliases; `gemicon` now holds no default directory at all, because a default
living in that package would necessarily be the gem set's and could silently be
handed to a second map. The root's default and the two sub-directory names live
in `internal/server` instead, and that is where the safety property now sits:
`NewRouter` substitutes `DefaultIconCacheDir` for an empty `IconCacheDir` before
it joins either sub-directory, so an unset root resolves to a real per-set
directory rather than to the process's working directory. `New` and `NewWithMap`
still reject an empty directory, but that guard is unreachable from the
production caller — the join always yields a non-empty path — so it is a
constructor precondition for future callers, not what protects this one.

**This changes nothing about the decisions above.** Seeding still happens from an
allowed IP, still lands before the deploy, and Decision 3 is still accepted and
still unimplemented. The separate-directories requirement in the POE-177
amendment is unchanged and is exactly what the sub-directories preserve: the
filename scheme is shared and neither generator can see the other's keys, so a
flat root would still let a gem name and an item id that reduce to the same
`safeFileName` serve each other's artwork. What the split no longer requires is a
*second volume*, and a future icon set gets a sub-directory rather than another
one.

**What forced it (measured on production, 2026-09-01).** The second volume the
POE-177 amendment called for was never created. The server container had only the
gem volume mounted, no `CURRENCY_EXCHANGE_ICON_CACHE_DIR` in its environment, and
so the item cache resolved to the code default inside the container's writable
layer — where, per Decision 1, poewiki refuses the VPS and every
`GET /api/currency-exchange/icon/<id>` answered `502`. The desktop table rendered
no item icons at all. A per-set volume is an operator step that has to be
repeated for every new set and fails silently when skipped; a per-set
sub-directory under one mount is created by the server itself.

**The migration is one-time and ordered.** Mount the new volume, move the
existing gem files into `gems/` (764 files as of 2026-09-01 — a move, not a
re-crawl), seed `currency-exchange/` from an allowed IP, set `ICON_CACHE_DIR`,
then deploy, then drop the old volume. Seeding before the deploy is the same
Decision 2 ordering as any icon addition, for the same reason.
[GEM-ICONS.md → The one-time migration to a single volume](../GEM-ICONS.md#the-one-time-migration-to-a-single-volume-poe-221)
carries the commands.

## Amended 2026-09-04 (POE-135)

Status of this section: current behaviour.

**The gem set's source map is a directory of category files.**
`internal/gemicon/gem-icon-urls.json` is replaced by
`internal/gemicon/urls/gems.json` (763 skill gems) and
`internal/gemicon/urls/items.json` (the two lab offerings, which are items the
gem endpoint has to answer for because `MarketOverview.svelte` routes offering
names through it). `//go:embed urls/*.json` embeds the directory and `New`
merges every file it finds into the same single flat `map[string]string` the
handler has always looked names up in. The merged content is byte-for-byte what
the single file held: 765 entries, same keys, same URLs.

**Layout only; every decision above stands.** The map is still compiled into the
binary, so Decision 1 (production never fetches) and Decision 2 (seed before
deploy) are untouched, adding an icon is still a code change requiring a deploy,
and Decision 3 is still accepted and still unimplemented. The runtime lookup is
unchanged — one flat map, no categories — so nothing about 404s, 502s, caching
headers or the cache-filename scheme moves. There is still no runtime filesystem
read on the production path.

**Discovery rather than a named list, and a loud duplicate.** The loader globs
`urls/*.json` instead of naming the categories, so a future set — currency,
Alva/temple — is one new file and no Go change. A key present in two files is a
construction error naming the key and both files. That failure is the reason the
split is safe: the flat runtime map cannot represent two sources for one name,
so without the check one category's artwork would silently be served under the
other's, which is the same class of collision the POE-177 amendment's
separate-cache-directories rule exists to prevent one layer down. A duplicate
key WITHIN one file is a different, unguarded hazard: every loader that reads it
(encoding/json, json.load, serde's BTreeMap) still resolves it last-writer-wins
with no error, so that case is guarded only by the sorted-one-key-per-line file
convention, not by code.

**What else moved with it.** `docker-compose.yml` bind-mounts the `urls`
directory (not one file) into the `desktop` container, and the merc seed-art
contract test (`desktop/src-tauri/src/mercenary/seed.rs`) unions the keys of
every `*.json` in it — the contract is what `/api/gem-icon/{name}` answers for,
which is the merged map. `scripts/download-gem-icons.py` accepts a directory and
merges it with the same duplicate-key failure.

**Not done here, deliberately.** POE-135 also proposed renaming the package
`gemicon` → `icons` and the route `/api/gem-icon/{name}` → `/api/icon/{name}`.
Both are left for the owner: the installed desktop builds call the current
route, so a rename without an alias breaks them on the next server deploy, and
the package's own doc comment records a later (POE-177) decision to keep its
name for its origin.
