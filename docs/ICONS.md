# Icons

Status: current. Last verified: 2026-09-08. This is the canonical guide for
the HTTP icon routes, embedded source maps, cache layout, and cache seeding.

## Routes and cache layout

The canonical route is:

~~~text
GET /api/icon/{type}/{name}
~~~

The current types are gems, items, temple, and currency-exchange. {type} is the
basename of the source map, so a map rename is a public API break and requires
coordinated client changes.

GET /api/gem-icon/{name} remains as a compatibility alias for installed
desktop builds. It resolves the name through the merged embedded gems/items/
temple index and then uses the typed cache. Currency-exchange ids are not in
that index and the alias must not serve them. The browser does not use the
alias: the SPA and API ship together, while installed desktop builds can
outlive the server deploy.

GET /api/currency-exchange/icon/{name} was dropped. It is not an alias and
must not be reintroduced.

ICON_CACHE_DIR is one cache root. Each source map has one subdirectory:

| Type | Source map | Cache directory | Entries |
| --- | --- | ---: | ---: |
| gems | internal/icons/urls/gems.json | gems/ | 763 |
| items | internal/icons/urls/items.json | items/ | 2 |
| temple | internal/icons/urls/temple.json | temple/ | 32 |
| currency-exchange | internal/exchange/itemdata/icon-urls.json | currency-exchange/ | 1541 |

The server creates one icons.Cache per map. Cache filenames are unchanged:
safeName(name)-shortHash(url).png. Moving a category only moves its directory;
it does not rename its files. The server never searches another category's
directory.

Each embedded category is constructed independently. A malformed map disables
that typed category and logs its error while valid categories remain available.
Cross-map duplicate names are rejected individually from the compatibility
alias, with an error naming both files; every non-conflicting name remains
resolvable and the typed routes remain unambiguous.

## Client paths

Web and desktop use $lib/icons.ts:

~~~ts
getIconUrl('gems', name)
getIconUrl('items', name)
getIconUrl('temple', name)
getIconUrl('currency-exchange', id)
~~~

The temple package emits /api/icon/temple/<escaped name>. Currency Exchange
payloads emit the API-relative path /icon/currency-exchange/<escaped id>; the
desktop client joins it to its /api base. Metadata IDs are one escaped path
segment, so / becomes %2F.

Mercenary artwork is a separate system: merc_icon_templates in PostgreSQL,
device signatures, and /api/desktop/merc-templates. It does not use this
cache or ICON_CACHE_DIR.

## Adding or changing an icon

1. Update the category map or the generator that produces it. Keep the map
   basename stable unless the public {type} route is intentionally changing.
2. For gems, items, or temple, pull the directory into a cache root. The
   command fans out by map basename:

   ~~~sh
   python3 scripts/download-gem-icons.py pull internal/icons/urls icons-cache
   ~~~

   For a one-off flat map, including Currency Exchange, pass the file and the
   destination directory directly:

   ~~~sh
   python3 scripts/download-gem-icons.py pull \
     internal/exchange/itemdata/icon-urls.json icons-cache/currency-exchange
   ~~~

3. Inspect failures. The puller validates PNG signatures, uses the same
   content-addressed filename scheme as the Go server, and does not overwrite
   an existing non-empty file.
4. Seed the matching production subdirectory before deploying the binary that
   embeds the map. The production VPS cannot recover by fetching poewiki.
5. Test the typed route and the relevant client surface. Keep the 45 missing
   Vaal <Base> (<Transfigured>) entries open; choosing which of the two icon
   shapes they should use is outside this route/layout change.

To remove superseded files, prune the whole root against the category maps:

~~~sh
python3 scripts/download-gem-icons.py prune icons-cache \
  --map internal/icons/urls --dry-run
python3 scripts/download-gem-icons.py prune icons-cache \
  --map internal/icons/urls
~~~

The wrong-map/directory refusal is evaluated independently for each pair. A
single flat map keeps the --map override, for example:

~~~sh
python3 scripts/download-gem-icons.py prune icons-cache/currency-exchange \
  --map internal/exchange/itemdata/icon-urls.json --dry-run
~~~

Use --force only when the map really dropped most of that category.

## Production seeding and migration

This section is an owner-run procedure. It was not executed by the route/layout
change.

Production gems/ currently contains the 763 gem files plus the two lab-offering
files. Before deploying, move those two offering files into items/; do not
re-pull them merely to change the layout. Seed temple/ with its 32 files, plus
currency-exchange/ when that cache is being deployed. The final gems/ directory
must contain the gem files only; the items cache will not read offering files
left behind there.

For a prepared local root, the shape to copy is:

~~~sh
tar czf gems.tgz -C icons-cache/gems .
tar czf items.tgz -C icons-cache/items .
tar czf temple.tgz -C icons-cache/temple .
tar czf currency-exchange.tgz -C icons-cache/currency-exchange .
scp gems.tgz items.tgz temple.tgz currency-exchange.tgz "$PROD_HOST":/tmp/
~~~

On the production host, extract each archive into the corresponding
$NEW_VOL/_data/{gems,items,temple,currency-exchange}/ directory before the
deploy. Use the actual persistent icon-cache volume path; do not replace it
with an ephemeral container directory.

For an existing old-layout cache, the filename migration remains offline and
does not re-download artwork:

~~~sh
python3 scripts/download-gem-icons.py migrate icons-cache \
  --map internal/icons/urls
python3 scripts/download-gem-icons.py migrate icons-cache/currency-exchange \
  --map internal/exchange/itemdata/icon-urls.json
~~~

## Verification

Use GET, not curl -I: the router registers GET only, so HEAD measures the
SPA/static fallback rather than the icon handler.

After the seeded deploy, these should be checked:

~~~sh
curl -s -D- -o /dev/null \
  "https://profitofexile.top/api/icon/temple/Vial%20of%20Fate"
curl -s -D- -o /dev/null \
  "https://profitofexile.top/api/gem-icon/Absolution"
curl -s -D- -o /dev/null \
  "https://profitofexile.top/api/icon/nosuchtype/Absolution"
~~~

The first request is the typed temple check; the second is the installed-build
alias check. If seeded, both should be 200 with an image response and the long
immutable cache header. The last should be 404 with Cache-Control: no-store.
Also verify an exchange ID containing %2F through
/api/icon/currency-exchange/<escaped id>.

The alias exit condition is measurable: the access log's route field records
the matched chi pattern. Remove the alias one minor release after its share of
icon requests reaches zero, and record that trigger in the ADR before deleting
the route.
