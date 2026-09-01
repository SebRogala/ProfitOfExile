# Gem and Item Icons

Status: current guide.

The server serves artwork at `/api/gem-icon/{name}` from `internal/gemicon`. Each
name is resolved against `internal/gemicon/gem-icon-urls.json`, which is compiled
into the binary (`//go:embed`). A name absent from that map returns `404` and the
UI renders its `?` fallback.

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
<ICON_CACHE_DIR>/gems               internal/gemicon/gem-icon-urls.json
<ICON_CACHE_DIR>/currency-exchange  internal/exchange/itemdata/icon-urls.json
```

The sub-directories are load-bearing, not tidiness. Both sets run through the
same cache-filename scheme and their key spaces are generated independently, so
a flat root would let a gem name and a metadata id that reduce to the same
filename serve each other's artwork. `internal/server` derives the two paths
from the root (`gemIconSubdir`, `currencyExchangeIconSubdir`); nothing is ever
cached in the root itself, and a new icon set gets a new sub-directory rather
than a new volume.

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

2. **Add the entry** to `internal/gemicon/gem-icon-urls.json`, keeping the file sorted by key.

3. **Pull the new file(s).** The puller writes using the server's exact cache-filename scheme and skips files already present, so it is safe to re-run:

   ```
   python3 scripts/download-gem-icons.py internal/gemicon/gem-icon-urls.json icons-cache/gems
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

Until ADR-012 decision 3 lands, this needs an extra manual step. `load()` returns
the disk copy unconditionally when the file exists, and the filename derives from
the gem **name**, not the URL — so correcting the map alone changes nothing and
the old artwork is served indefinitely.

Update the map entry, **delete the stale file from the production volume**, then
deploy. Once cache filenames are content-addressed, the map edit alone will
suffice.

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
file.

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
   python3 scripts/download-gem-icons.py \
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

### The one-time migration to a single volume (POE-221)

Status: pending as of 2026-09-01. Production still runs the old layout — one
volume mounted at `/data/gem-icons-cache` under `GEM_ICON_CACHE_DIR`, no volume
and no variable for the item set, and every
`GET /api/currency-exchange/icon/<id>` answering `502`. That is the symptom this
change exists to fix; the code no longer reads either old variable, so the
migration below is not optional once the new binary deploys.

Order matters for the same ADR-012 reason as everything above: **seed before the
deploy that reads `ICON_CACHE_DIR`.** The whole chain is

> create the volume → seed **both** sub-directories from the host → set
> `ICON_CACHE_DIR` → deploy → verify one gem icon *and* one item icon → only
> then remove `GEM_ICON_CACHE_DIR` and the old volume.

Both seeding steps are **host-side**, and that is not a stylistic choice. A
container's mounts are fixed when it is created, so the new volume is not
visible inside the server container that is running now — it appears on the
deploy in step 4. `docker cp` into `/data/icons-cache/...` before that deploy
either fails outright or, without a trailing slash on the destination, silently
writes into the container's writable layer, where the deploy throws it away.
The `docker cp` recipe in [Pre-seeding production](#pre-seeding-production)
step 4 is for top-ups *after* this migration, not for it.

1. **Create the Coolify persistent volume** `$SERVER_SERVICE_ID-profitofexile-icons`
   on the server service, mounted at `/data/icons-cache`.

2. **Move the existing gem files into `gems/`.** They are already on the host in
   the old volume's `_data/` directory — 764 files as of 2026-09-01 — and moving
   them costs nothing, where re-pulling costs a full poewiki crawl:

   ```
   OLD_VOL=/var/lib/docker/volumes/$SERVER_SERVICE_ID-profitofexile-gem-icons
   NEW_VOL=/var/lib/docker/volumes/$SERVER_SERVICE_ID-profitofexile-icons
   ssh "$PROD_HOST" "mkdir -p $NEW_VOL/_data/gems && \
     mv $OLD_VOL/_data/*.png $NEW_VOL/_data/gems/"
   ```

   Both are host paths under Docker's volume root (`/var/lib/docker/volumes/<name>`); the private
   ops notes carry the exact names. Nothing in the new layout reads the root, so a file
   left behind in `_data/` is invisible rather than wrong.

3. **Seed `currency-exchange/` the same way** — host-side, symmetric with the
   gem move in step 2. This has never been seeded on production, so it is a
   full pull from an allowed IP, not a top-up: run the puller per
   [Pre-seeding production](#pre-seeding-production) step 1, then untar the
   result onto the volume, exactly as its step 2 does:

   ```
   tar czf new-item-icons.tgz -C icons-cache/currency-exchange .
   scp new-item-icons.tgz "$PROD_HOST":/tmp/
   ssh "$PROD_HOST" "mkdir -p $NEW_VOL/_data/currency-exchange && \
     tar xzf /tmp/new-item-icons.tgz -C $NEW_VOL/_data/currency-exchange/"
   ```

   The `mkdir -p` is on the host path, for the same reason step 2's is: the
   server creates both sub-directories on startup, but not until the deploy in
   step 4 gives it the mount.

4. **Set `ICON_CACHE_DIR=/data/icons-cache`** in the Coolify environment. There
   is no alias: leaving `ICON_CACHE_DIR` unset resolves the default
   `./data/icons-cache` inside the container's writable layer — no error, no
   icons, and everything gone on the next deploy. `GEM_ICON_CACHE_DIR` is dead
   in the code, so leaving it set for now changes nothing; it comes out in
   step 5, once the new layout is proven.

5. **Deploy, verify, then drop the old variable and volume.** Verify both routes
   before removing anything:

   ```
   curl -s -o /dev/null -w "%{http_code}\n" \
     "https://profitofexile.top/api/gem-icon/Absolution"
   curl -s -o /dev/null -w "%{http_code}\n" \
     "https://profitofexile.top/api/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare"
   ```

   The first `200` proves the gem move landed, the second that the item seed is
   readable — one alone proves half a migration. Only after both do you remove
   `GEM_ICON_CACHE_DIR` from the environment and drop
   `$SERVER_SERVICE_ID-profitofexile-gem-icons`.

## What "missing" looks like

A name absent from the map returns `404` and renders as `?`. Three ways this
happens:

- **A new skill the wiki has not published art for yet.** Recheck after a week or
  two; the four Allflame skills missing at launch all resolved within days.
- **A name the map was never meant to cover.** `MarketOverview.svelte` routes lab
  offering names through the gem endpoint, so `Gift to the Goddess` and
  `Dedication to the Goddess` need entries here despite not being gems. That
  `<img>` has no `?` fallback and renders broken instead.
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
