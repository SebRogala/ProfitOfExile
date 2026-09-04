# Gem and Item Icons

Status: current guide.

The server serves artwork at `/api/gem-icon/{name}` from `internal/gemicon`. Each
name is resolved against the category maps in `internal/gemicon/urls/` —
`gems.json` and `items.json` today — which are compiled into the binary
(`//go:embed urls/*.json`) and merged into one flat lookup at construction. A
name absent from that merged map returns `404` and the UI renders its `?`
fallback.

The split is source-side only, and the loader discovers the files rather than
naming them: a new category (POE-135) is one new `*.json` there and no Go
change. A key present in two of them fails construction naming the key and both
files rather than letting one file's URL silently win the other's name.

The architectural constraints — why production cannot fetch icons itself, and why
the cache is seeded before the map deploys — are recorded in
[ADR-012](adr/012-icons-are-pre-seeded-from-an-allowed-ip-and-cached-by-content-address.md). Read that before
changing how icons are fetched or cached.

Currency Exchange item icons run on the same cache under a second map and a
second route; the steps below are gem-specific, so see
[Currency Exchange items](#currency-exchange-items) for what differs.

Both sets share **one** cache root — `ICON_CACHE_DIR`, default
`./data/icons-cache`, `/data/icons-cache` on production — with **one
sub-directory per set**:

```
<ICON_CACHE_DIR>/gems               internal/gemicon/urls/*.json
<ICON_CACHE_DIR>/currency-exchange  internal/exchange/itemdata/icon-urls.json
```

One cache sub-directory per icon **set**, not per source file: every category
map under `internal/gemicon/urls/` feeds the one `/api/gem-icon/{name}` route
and therefore the one `gems/` directory.

The sub-directories are load-bearing, not tidiness. Both sets run through the
same cache-filename scheme and their key spaces are generated independently, so
a flat root would let a gem name and a metadata id that reduce to the same
**safe name**, and only if they also share a source URL, serve each other's
artwork.
`internal/server` derives the two paths from the root (`gemIconSubdir`,
`currencyExchangeIconSubdir`); nothing is ever cached in the root itself, and a
new icon set gets a new sub-directory rather than a new volume.

## Adding an icon

Order matters. Seeding after the deploy leaves a window where the server knows the
name, cannot fetch it, and returns `502`.

1. **Find the poewiki image URL.** Use the API rather than guessing the hash path:

   ```
   curl -s "https://www.poewiki.net/w/api.php?action=query&titles=File:Reap_of_Butchery_inventory_icon.png&prop=imageinfo&iiprop=url&format=json"
   ```

   Take `imageinfo[0].url` — the direct `https://www.poewiki.net/images/<x>/<xx>/<Name>_inventory_icon.png` form, never the `Special:Filepath` redirect. Confirm it returns a real image, not an HTML page:

   ```
   curl -s -o /tmp/i.png -w "%{http_code} %{content_type}\n" "<url>" && file -b /tmp/i.png
   ```

   Inventory icons are `78 x 78` PNGs. Anything else means you followed a redirect to the wrong file — a page redirect can silently resolve to a *different item's* artwork.

2. **Add the entry** to the category file it belongs in, keeping that file
   sorted by key. `internal/gemicon/urls/gems.json` for a skill gem;
   `internal/gemicon/urls/items.json` for anything that is not one — the two
   lab offerings live there because `MarketOverview.svelte` routes offering
   names through the gem endpoint. A category that does not exist yet is a new
   `*.json` file in the same directory and needs no code change; the same key
   must not appear in two of them, or construction fails.

3. **Pull the new file(s).** The puller writes using the server's exact cache-filename scheme — `<safe name>-<16 hex of the URL's SHA-256>.png` — and skips files already present under that name, so it is safe to re-run. Hand it the whole directory: it merges the category files the way the server does.

   ```
   python3 scripts/download-gem-icons.py pull internal/gemicon/urls icons-cache/gems
   ```

   To pull only new entries, hand it a JSON file containing just those keys.

4. **Seed the production volume** — from a machine poewiki does not block.
   `PROD_HOST` and `SERVER_SERVICE_ID` are placeholders; this repository is
   public, so the real values live in the private ops notes. See
   [Deployment](DEPLOY.md).

   ```
   tar czf new-icons.tgz -C icons-cache/gems .
   scp new-icons.tgz "$PROD_HOST":/tmp/
   ssh "$PROD_HOST" "C=\$(docker ps -q -f name=$SERVER_SERVICE_ID); \
     mkdir -p /tmp/ni && tar xzf /tmp/new-icons.tgz -C /tmp/ni && \
     for f in /tmp/ni/*.png; do docker cp \"\$f\" \"\$C:/data/icons-cache/gems/\"; done"
   ```

   The server image has no shell, so `docker cp` is the way in. The volume is
   `$SERVER_SERVICE_ID-profitofexile-icons` mounted at `/data/icons-cache`, and
   gem files go in its `gems/` sub-directory — a file dropped in the root is
   never read.

5. **Deploy** — the map is embedded, so the icon only resolves once the new binary
   is running. Merging to `main` deploys when the change touches a filtered path;
   `internal/gemicon/**` does, so a map edit is enough. See
   [Deployment](DEPLOY.md) for why a green pipeline is not proof the deploy
   landed.

6. **Verify:**

   ```
   curl -s -o /dev/null -w "%{http_code} %{content_type}\n" \
     "https://profitofexile.top/api/gem-icon/Reap%20of%20Butchery"
   ```

## Changing an icon

Since POE-136 this is the same six steps as [Adding an icon](#adding-an-icon),
with no extra manual step. The cache filename carries a hash of the source URL —
`<safe name>-<16 hex of the URL's SHA-256>.png` — so a corrected URL is a
different filename, which misses the cache and fetches. Edit the map entry, pull,
seed, deploy, verify.

Until POE-136 it needed a third, invisible step: `load()` returns the disk copy
unconditionally when the file exists, and the filename derived from the gem
**name** alone, so correcting the map changed nothing and the old artwork was
served indefinitely unless you also deleted the stale file from the production
volume by hand.

The file under the old URL's hash stays behind — nothing reads it and nothing
removes it. See [Superseded files](#superseded-files).

## Superseded files

A URL change leaves the previous file in the volume under its old hash. It is
never read again: the server only ever builds the path for the URL the current
map holds. Growth is bounded by how often upstream renames artwork, which is
rare, so this is a sweep you run when convenient, not on a timer.

The wanted set is a pure function of the current map, which is what makes the
sweep safe to re-run:

```
python3 scripts/download-gem-icons.py prune icons-cache/gems --dry-run
python3 scripts/download-gem-icons.py prune icons-cache/gems
```

`--dry-run` lists every file the map does not produce and deletes nothing; run it
first, and read the list. Pass `--map internal/exchange/itemdata/icon-urls.json`
when sweeping `icons-cache/currency-exchange` — pruning a directory against the
other set's map would produce a wanted set disjoint from everything on disk. That
one is caught rather than trusted: `prune` refuses when none of the on-disk
files, or under half of them, are produced by the map, printing the map, the
directory and the counts, and it refuses under `--dry-run` too so a wrong pairing
cannot get talked through on a plausible-looking list. `--force` overrides, for
the rare map that legitimately dropped most of its entries.

`prune` works on a directory, so sweep the staging copy before shipping it,
rather than trying to run the script on the production host. **On production the
superseded files are therefore cleared only by the re-upload chain** — nothing on
the VPS sweeps, and the server never deletes — so a stranded file sits in the
volume until a run of this sweep against a staging copy is uploaded over it.

## Currency Exchange items

Status: current. Added 2026-08-19 (POE-177); categories 2026-08-20 (POE-185).

The Currency Exchange page needs artwork for a different key space: GGG's feed
identifies items by metadata id (`Metadata/Items/Currency/CurrencyRerollRare`),
not by display name. Those icons are served at

```
GET /api/currency-exchange/icon/{metadata id, %2F-escaped}
```

by the **same** `internal/gemicon` cache over a second map —
`gemicon.NewWithMap(exchange.IconURLs(), <ICON_CACHE_DIR>/currency-exchange)`.
Everything above about 404s, 502s, `no-store`, caching headers and seed-before-
deploy applies unchanged, and so does the one-root/one-sub-directory layout: the
two sets share a volume but never a directory, because they share the
cache-filename scheme and a gem name and an item id could reduce to the same
safe name — and, only if they also share a source URL, to the same file.

What is different is where the map comes from. There is no hand-edited JSON to
add a row to: `internal/exchange/itemdata/items.json` (names, icons and
categories) and `icon-urls.json` (the flat `id → URL` map the puller reads) are
**generated**, and hand edits are lost on the next run. From the repository
root:

```
python3 scripts/generate-currency-exchange-items.py
```

It reads the item universe from the RePoE-fork base-item dump, joins poewiki's
`items` cargo table for the icon file names, resolves them through the imageinfo
API, and prints coverage per metadata bucket and per sidebar category. It
refuses to write on a coverage shortfall, a cache-filename collision or an id no
category rule matches, and its output is deterministic — an unchanged upstream
re-runs to an empty diff. Run it **once per league**: GGG adds items between
leagues, not within one. An id the asset misses is not a
broken page — the name falls back to the humanized id and the icon to none —
but it does log `WARN currency-exchange: unknown item id`, which is the signal
to re-run.

The category is the third generated field, and it has no upstream: neither
source knows the exchange's own sidebar, and the metadata path is not the
taxonomy (oils, catalysts, omens, tattoos and runegrafts all sit under
`Metadata/Items/Currency/`). `CATEGORY_RULES` in the script is the curated
answer — ordered `(prefix, optional id substring, category)` rows onto the
sixteen categories the in-game sidebar lists, first match wins (the substring is
what pulls Allflame embers and Legion emblems out of `MapFragments/` before its
catch-all) — and a rule naming a category outside those sixteen fails the run
before the first request. `internal/exchange/items.go` holds the same sixteen
for the wire, and both test suites restate the list as deliberate independent
oracles, so a sidebar change is a four-file edit that fails loudly until all
four agree. Refreshing
the taxonomy needs no icon pass: `--skip-icons` carries the previous asset's
icons forward and rewrites names and categories from the dump.

### Pre-seeding production

Step 4 above is the gem sub-directory of the shared volume; the item set needs
the same treatment against its own sub-directory. `PROD_HOST` and
`SERVER_SERVICE_ID` are the same placeholders as in step 4.

1. **Pull the files** — from a machine poewiki does not block. Same puller as
   step 3 above, pointed at the generated flat map and at the item
   sub-directory:

   ```
   python3 scripts/download-gem-icons.py pull \
     internal/exchange/itemdata/icon-urls.json icons-cache/currency-exchange
   ```

2. **First seed — untar onto the volume from the host.** This is the path to
   use whenever the container running right now does not already have the icon
   volume mounted. A container's mounts are fixed when it is created, so a
   volume added in Coolify is invisible to the running process and appears only
   on the next deploy; writing the host path sidesteps that, and needs no shell
   in the image.

   ```
   ICON_VOL=/var/lib/docker/volumes/$SERVER_SERVICE_ID-profitofexile-icons
   tar czf new-item-icons.tgz -C icons-cache/currency-exchange .
   scp new-item-icons.tgz "$PROD_HOST":/tmp/
   ssh "$PROD_HOST" "mkdir -p $ICON_VOL/_data/currency-exchange && \
     tar xzf /tmp/new-item-icons.tgz -C $ICON_VOL/_data/currency-exchange/"
   ```

   `$ICON_VOL` is the host path under Docker's volume root (`/var/lib/docker/volumes/<name>`, as observed on the production host) for
   `$SERVER_SERVICE_ID-profitofexile-icons`; the private ops notes carry the exact name. The
   `mkdir -p` is on the **host** path and is load-bearing before the first
   deploy — the server creates both sub-directories on startup, but only once it
   has the mount. Nothing in the layout reads the volume root, so files left in
   `_data/` are invisible rather than wrong.

3. **Then deploy.** The seed has to land *before* the first deploy that serves
   item icons, not after. This does not degrade gracefully: poewiki 403s the
   VPS, so an unseeded item icon is a permanent `502` and a permanent `?`, not
   a slow first request.

4. **Later top-ups — `docker cp` into the running container.** Once a deploy has
   mounted the volume and the server has created both sub-directories, new files
   can go straight in without waiting for a deploy. The server image has no
   shell, so `docker cp` is the way in.

   ```
   tar czf new-item-icons.tgz -C icons-cache/currency-exchange .
   scp new-item-icons.tgz "$PROD_HOST":/tmp/
   ssh "$PROD_HOST" "C=\$(docker ps -q -f name=$SERVER_SERVICE_ID); \
     mkdir -p /tmp/nii && tar xzf /tmp/new-item-icons.tgz -C /tmp/nii && \
     for f in /tmp/nii/*.png; do docker cp \"\$f\" \"\$C:/data/icons-cache/currency-exchange/\"; done"
   ```

   Both preconditions are silent when unmet. `docker cp` into a path the
   container does not have mounted writes to its writable layer — no error, and
   the bytes vanish on the next deploy — and dropping the trailing slash on the
   destination does the same thing under a different name. Use step 2 instead
   whenever you are not certain the mount is there.

### Migration to content-addressed names

Status: pending as of 2026-09-04. POE-136 changed the cache filename from
`<safe name>.png` to `<safe name>-<16 hex of the URL's SHA-256>.png`. Every file
already in production carries the old name, so after the deploy the server looks
for names that are not there, and it cannot recover by fetching — poewiki 403s
the VPS (ADR-012 decision 1). Unmigrated, the deploy answers `502` for **every**
icon in **both** sets.

> **DEPLOY-BLOCKER:** do not push main until both icon sub-dirs on prod hold the
> new `<safe>-<hex>.png` names (run `migrate` on the existing files, then `pull`
> for anything missing, then `prune`).

The rename needs no re-crawl: the bytes are already on disk and only the filename
changes, which is what `migrate` does.

```
python3 scripts/download-gem-icons.py migrate icons-cache/gems
python3 scripts/download-gem-icons.py migrate icons-cache/currency-exchange \
  --map internal/exchange/itemdata/icon-urls.json
```

`migrate` renames a file only when the old name exists and the new one does not,
so it is idempotent and safe to re-run. It touches no network, and it reports
three counts against the map's entry count. Read them as a sanity check:

- **`renamed` ≈ map size** is what a first run on an old-named directory looks
  like.
- **`already_addressed` ≈ map size, `renamed` 0** is a re-run, and is healthy —
  the files are already under their content-addressed names. On a *first* run it
  means the directory was migrated before, not that the map is wrong.
- **`no_old_file` > 0 on a first run** means neither name is on disk for those
  entries: the staging copy is incomplete (or the map is the wrong one for this
  directory). `pull` in the next step fetches whatever is genuinely new, but a
  large `no_old_file` on a directory you just copied down is a copy that lost
  files, and worth stopping for.

**The single-volume migration below has not run yet either, so the two are one
chain, not two.** Run them together in the order that section gives; it already
carries the `migrate`, `pull` and `prune` steps in place.

### The one-time migration to a single volume (POE-221) and to content-addressed names (POE-136)

Status: pending as of 2026-09-04. Production still runs the old layout — one
volume mounted at `/data/gem-icons-cache` under `GEM_ICON_CACHE_DIR`, no volume
and no variable for the item set, and every
`GET /api/currency-exchange/icon/<id>` answering `502` — and its gem files still
carry the old name-only filenames. The code no longer reads either old variable
and no longer looks for the old filenames, so the migration below is not optional
once the new binary deploys. Both changes are unreleased, which is why this is
one chain: the volume move and the rename touch the same files once.

Order matters for the same ADR-012 reason as everything above: **seed before the
deploy that reads `ICON_CACHE_DIR`.** The whole chain is

> create the volume → pull the existing gem files down to a staging copy →
> `migrate` them to content-addressed names → `pull` both sets for anything
> missing → `prune` both staging copies → upload **both** sub-directories to the
> host → set `ICON_CACHE_DIR` → deploy → verify one gem icon *and* one item icon
> → only then remove `GEM_ICON_CACHE_DIR` and the old volume.

Both upload steps are **host-side**, and that is not a stylistic choice. A
container's mounts are fixed when it is created, so the new volume is not
visible inside the server container that is running now — it appears on the
deploy in step 7. `docker cp` into `/data/icons-cache/...` before that deploy
either fails outright or, without a trailing slash on the destination, silently
writes into the container's writable layer, where the deploy throws it away.
The `docker cp` recipe in [Pre-seeding production](#pre-seeding-production)
step 4 is for top-ups *after* this migration, not for it.

The renaming and the sweep happen on a **staging copy on your own machine**, not
on the production host: the puller needs the repo's maps, and prod has neither
the repo nor a reason to grow one. The gem bytes come down once, are renamed
offline, and go back up under their new names.

1. **Create the Coolify persistent volume** `$SERVER_SERVICE_ID-profitofexile-icons`
   on the server service, mounted at `/data/icons-cache`.

2. **Pull the existing gem files down to a staging copy.** They are already on
   the host in the old volume's `_data/` directory — 764 files as of 2026-09-01
   — and moving bytes costs nothing, where re-pulling costs a full poewiki
   crawl. They come to your machine rather than straight across because their
   names are about to change:

   ```
   OLD_VOL=/var/lib/docker/volumes/$SERVER_SERVICE_ID-profitofexile-gem-icons
   NEW_VOL=/var/lib/docker/volumes/$SERVER_SERVICE_ID-profitofexile-icons
   mkdir -p icons-cache/gems
   ssh "$PROD_HOST" "tar czf - -C $OLD_VOL/_data ." | tar xzf - -C icons-cache/gems
   ```

   Both `$OLD_VOL` and `$NEW_VOL` are host paths under Docker's volume root
   (`/var/lib/docker/volumes/<name>`); the private ops notes carry the exact
   names.

3. **Rename them to content-addressed names** (POE-136). Offline, no network,
   idempotent:

   ```
   python3 scripts/download-gem-icons.py migrate icons-cache/gems
   ```

   This is a first run on an old-named directory, so expect `renamed` to be
   close to the 764 files that came down and `no_old_file` to be small — only
   map entries added since production was last seeded. `renamed` 0 with
   `no_old_file` at the map size means the staging directory or the map is wrong
   — stop and check. `already_addressed` at the map size instead means this
   directory has already been migrated, which is fine on a re-run and a sign you
   copied the wrong directory down on a first one.

4. **Pull anything still missing, for both sets** — from a machine poewiki does
   not block. The gem run is a top-up: `pull` skips every file step 3 already
   put under its content-addressed name, so it fetches only genuinely new
   entries. The item set has never been seeded on production, so its run is a
   full pull.

   ```
   python3 scripts/download-gem-icons.py pull internal/gemicon/urls icons-cache/gems
   python3 scripts/download-gem-icons.py pull \
     internal/exchange/itemdata/icon-urls.json icons-cache/currency-exchange
   ```

5. **Sweep what the maps no longer produce.** Old-named leftovers from step 2 —
   files for names the map has since dropped, which `migrate` left alone — must
   not be shipped. Read the `--dry-run` list before deleting:

   ```
   python3 scripts/download-gem-icons.py prune icons-cache/gems --dry-run
   python3 scripts/download-gem-icons.py prune icons-cache/gems
   python3 scripts/download-gem-icons.py prune icons-cache/currency-exchange \
     --map internal/exchange/itemdata/icon-urls.json
   ```

   Every remaining file in each directory is now exactly what the deployed
   binary will look for.

6. **Upload both sub-directories** — host-side, into the new volume:

   ```
   tar czf new-gem-icons.tgz -C icons-cache/gems .
   tar czf new-item-icons.tgz -C icons-cache/currency-exchange .
   scp new-gem-icons.tgz new-item-icons.tgz "$PROD_HOST":/tmp/
   ssh "$PROD_HOST" "mkdir -p $NEW_VOL/_data/gems $NEW_VOL/_data/currency-exchange && \
     tar xzf /tmp/new-gem-icons.tgz -C $NEW_VOL/_data/gems/ && \
     tar xzf /tmp/new-item-icons.tgz -C $NEW_VOL/_data/currency-exchange/"
   ```

   The `mkdir -p` is on the **host** path and is load-bearing: the server creates
   both sub-directories on startup, but not until the deploy in step 7 gives it
   the mount. Nothing in the layout reads the volume root, so files left in
   `_data/` are invisible rather than wrong.

7. **Set `ICON_CACHE_DIR=/data/icons-cache`** in the Coolify environment. There
   is no alias: leaving `ICON_CACHE_DIR` unset resolves the default
   `./data/icons-cache` inside the container's writable layer — no error, no
   icons, and everything gone on the next deploy. `GEM_ICON_CACHE_DIR` is dead
   in the code, so leaving it set for now changes nothing; it comes out in
   step 8, once the new layout is proven.

8. **Deploy, verify, then drop the old variable and volume.** Verify both routes
   before removing anything:

   ```
   curl -s -o /dev/null -w "%{http_code}\n" \
     "https://profitofexile.top/api/gem-icon/Absolution"
   curl -s -o /dev/null -w "%{http_code}\n" \
     "https://profitofexile.top/api/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare"
   ```

   The first `200` proves the gem move *and* the rename landed — a `502` there
   is the unmigrated-filename symptom, not a missing volume — and the second
   proves the item seed is readable. One alone proves half a migration. Only
   after both do you remove `GEM_ICON_CACHE_DIR` from the environment and drop
   `$SERVER_SERVICE_ID-profitofexile-gem-icons`.

## What "missing" looks like

A name absent from the map returns `404` and renders as `?`. Three ways this
happens:

- **A new skill the wiki has not published art for yet.** Recheck after a week or
  two; the four Allflame skills missing at launch all resolved within days.
- **A name the map was never meant to cover.** `MarketOverview.svelte` routes lab
  offering names through the gem endpoint, so `Gift to the Goddess` and
  `Dedication to the Goddess` need entries here despite not being gems — which
  is what `urls/items.json` is for. That `<img>` has no `?` fallback and
  renders broken instead.
- **A name the surface should never have asked for.** The map is not the place to
  fix this one — the name source is. See below.

All 765 map entries resolve as of 2026-08-05.

### Count the surfaces, not the market

"N gems have no icon" is not a number the map can answer on its own, because most
names the database holds never reach an `<img>`. Measured 2026-08-05 against the
latest local Allflame snapshot:

| Set | Names | No icon |
|---|---|---|
| `gem_colors` ∪ the league's snapshot names | 906 | 143 |
| Latest league snapshot only | 811 | 51 |
| Font (normal-mode) picker | 202 | 0 |
| Dedication picker, both pools | 582 | 45 |

The first row is the tempting one and the wrong one. 46 of its 143 are support
gems and 12 are `of Trarthus` — none of which any icon surface requests, because
`internal/lab/eligibility.go` keeps them out of every pool. Reading that number as
an icon backlog buys artwork nothing renders.

The Font picker reached 0 by routing its SQL through `isFontOutcome`'s fragments
rather than by adding icons: it used to offer the corrupted transfigured market
too, which is where 45 of its 46 misses came from.

### The remaining 45: `Vaal <Base> (<Transfigured>)`

poe.ninja gives a transfigured gem corrupted into its Vaal form a compound market
identity — `Vaal Arc (Arc of Surging)`, `Vaal Reap (Reap of Butchery)`. These are
legal Dedication *feeds* (`isDedicationFeed`), so the Dedication picker offers
them deliberately and the `?` is a real gap, not an eligibility leak.

Every one of the 45 has its `Vaal <Base>` prefix already in the map, so no
artwork needs sourcing. Closing it is a choice between two shapes, neither taken
yet:

- **45 alias entries** pointing at the URLs already present. Follows the process
  above unchanged, costs 45 duplicate files in the cache volume, and needs the
  prod seed before the deploy like any other addition.
- **Strip the parenthetical in the handler** before the map lookup. One rule
  instead of 45 rows, and it keeps working for next league's compounds — but it
  puts a name-shape rule inside `internal/gemicon`, which today knows nothing
  about gems beyond the map.

### A cached 404 is why a `?` survives the deploy that fixes it

This bit clients on production. The `404` used to carry no caching headers at
all — no `Cache-Control`, no `Expires`, no `Last-Modified` — which is *not* the
same as "do not cache". A `404` with no freshness information is heuristically
cacheable under RFC 7234, so browsers and webviews cached it on their own terms.
The result: gems that had since been added to the map, and that the server was
answering with `200`, still rendered `?`. `Ctrl+F5` "fixed" them, because a hard
reload bypasses exactly that heuristic cache.

The `404` now carries `Cache-Control: no-store`. Adding an icon is visible to
existing clients on their next render, with no hard reload and no waiting.

The trade is deliberate and cheap: `no-store` costs one map miss per render, and
the handler returns the `404` before touching the disk, the upstream, or the ETag
memo, so it is the least expensive response the endpoint produces. Any positive
TTL would buy that back at the price of re-introducing a silent staleness window
— a redeploy restarts the *server*, never the client's HTTP cache.

`502`s (upstream fetch failure) carry no caching directive either, so a retry
still reaches the server. That is the same reason a failed fetch writes nothing
to disk.

### Checking headers with curl

Use `GET`, not `HEAD`. `curl -I` on this route returns `200` with
`cache-control: no-cache` from the SPA static handler, because chi registers
`GET` only and `HEAD` falls through — so `-I` measures a different response than
the one an `<img>` gets:

```
curl -s -D- -o /dev/null "https://profitofexile.top/api/gem-icon/Absolution"
```
