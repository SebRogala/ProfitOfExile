# ADR-012: Icons Are Pre-Seeded from an Allowed IP and Cached by Content Address

## Status

Accepted. Decisions 1 and 2 describe current behaviour. **Decision 3
(content-addressed cache filenames) is accepted but not yet implemented** —
`safeFileName` still keys on the gem name alone. Until it lands, changing an
icon's URL also requires deleting the stale file from the production volume by
hand.

## Context

The server serves gem and item artwork at `/api/gem-icon/{name}` from
`internal/gemicon`. Each name is resolved against a name→URL map and the image is
cached on disk so the same request never hits upstream twice.

Two facts constrain the design, and neither is visible from the code:

- **poewiki 403s datacenter IPs.** The production VPS cannot fetch icons at
  runtime at all. Verified again 2026-07-26: the same URL that a developer
  machine downloads without incident is refused from the server.
- **The map is compiled into the binary** (`//go:embed gem-icon-urls.json`,
  `internal/gemicon/gemicon.go:41`). The handler looks a name up in that embedded
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
`GEM_ICON_CACHE_DIR` volume (`/data/gem-icons-cache` in production). Runtime
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
