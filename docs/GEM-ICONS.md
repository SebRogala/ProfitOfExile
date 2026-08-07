# Gem and Item Icons

Status: current guide.

The server serves artwork at `/api/gem-icon/{name}` from `internal/gemicon`. Each
name is resolved against `internal/gemicon/gem-icon-urls.json`, which is compiled
into the binary (`//go:embed`). A name absent from that map returns `404` and the
UI renders its `?` fallback.

The architectural constraints — why production cannot fetch icons itself, and why
the cache is seeded before the map deploys — are recorded in
[ADR-012](adr/012-icon-cache-preseeded-and-content-addressed.md). Read that before
changing how icons are fetched or cached.

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
   python3 scripts/download-gem-icons.py internal/gemicon/gem-icon-urls.json gem-icons-cache
   ```

   To pull only new entries, hand it a JSON file containing just those keys.

4. **Seed the production volume** — from a machine poewiki does not block.
   `PROD_HOST` and `SERVER_SERVICE_ID` are placeholders; this repository is
   public, so the real values live in the private ops notes. See
   [Deployment](DEPLOY.md).

   ```
   tar czf new-icons.tgz -C gem-icons-cache .
   scp new-icons.tgz "$PROD_HOST":/tmp/
   ssh "$PROD_HOST" "C=\$(docker ps -q -f name=$SERVER_SERVICE_ID); \
     mkdir -p /tmp/ni && tar xzf /tmp/new-icons.tgz -C /tmp/ni && \
     for f in /tmp/ni/*.png; do docker cp \"\$f\" \"\$C:/data/gem-icons-cache/\"; done"
   ```

   The server image has no shell, so `docker cp` is the way in. The volume is
   `$SERVER_SERVICE_ID-profitofexile-gem-icons` mounted at
   `/data/gem-icons-cache`.

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
