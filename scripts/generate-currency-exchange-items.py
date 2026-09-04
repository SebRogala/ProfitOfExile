#!/usr/bin/env python3
"""Generate the Currency Exchange item-data asset (names + icon URLs + categories).

The currency exchange feed identifies every item by its metadata id
(``Metadata/Items/Currency/CurrencyRerollRare``). The server needs three things
that the feed never sends: the in-game display name ("Chaos Orb"), an icon, and
the in-game sidebar category the item is filed under ("Currency").

Two upstreams supply the first two:

* **names** — the RePoE-fork base-item dump, keyed by metadata id. This is the
  id universe: every id the feed can contain in the eight item categories the
  exchange trades.
* **icons** — poewiki's ``items`` cargo table, joined on the same metadata id,
  whose ``inventory icon`` column names a wiki File page. Those File pages are
  resolved to their ``/images/<h>/<hh>/`` URLs — MediaWiki derives that path
  from the MD5 of the File NAME, so it is stable per name and a re-upload keeps
  the URL — in batches of fifty through the ``imageinfo`` API, with
  ``Special:FilePath`` as the per-file fallback.

The category has no upstream: neither source knows the exchange's own sidebar,
so ``CATEGORY_RULES`` maps id prefixes onto the sixteen categories the game
shows, and every id in the universe must match a rule or the run fails.

The result is committed as ``internal/exchange/itemdata/items.json`` (embedded
into the server binary by ``internal/exchange/items.go``) plus a flat
``icon-urls.json`` for ``scripts/download-gem-icons.py``, which pre-seeds the
server's disk cache from an allowed IP — poewiki 403s the production VPS
(docs/adr/012-*.md), so nothing is fetched at runtime in production.

Run it from the repository root once per league::

    python3 scripts/generate-currency-exchange-items.py

The output is deterministic: entries are sorted by id and formatted identically
on every run, so an unchanged upstream produces a zero-length diff. A full run
costs about thirty requests to poewiki, paced at ``--rate`` per second. If it
does hit a 429, stop and let the window clear — retrying immediately extends
the block rather than shortening it.

Exit codes: 0 success, 1 a coverage, uniqueness, or category gate failed, 2 an
upstream could not be read.
"""

from __future__ import annotations

import argparse
import html
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from collections import defaultdict

# --- upstreams ---------------------------------------------------------------

BASE_ITEMS_URL = (
    "https://raw.githubusercontent.com/repoe-fork/repoe-fork.github.io"
    "/master/data/base_items.json"
)
WIKI_API = "https://www.poewiki.net/w/api.php"
WIKI_INDEX = "https://www.poewiki.net/w/index.php"

# Every resolved icon must be a poewiki upload. MediaWiki derives the
# /images/<h>/<hh>/ path from the MD5 of the File NAME, so the URL is stable for
# a given name (a re-upload keeps it) — that is what makes it committable.
IMAGE_URL_PREFIX = "https://www.poewiki.net/images/"

# Every exchangeable metadata id starts with this.
METADATA_PREFIX = "Metadata/Items/"

# The item categories the currency exchange trades. A metadata id outside these
# eight prefixes is not exchangeable, so widening this list without a feed
# observation to justify it only inflates the asset.
CATEGORIES = [
    "Currency",
    "DivinationCards",
    "Scarabs",
    "MapFragments",
    "Deepwater",
    "AtlasExiles",
    "Delve",
    "Heist",
]

# The sixteen categories of the in-game Currency Exchange sidebar, in the game's
# order and spelling. The desktop filter renders this list verbatim, so both are
# the game's to change, not ours. Expedition is listed although no id maps to it
# (the exchange trades no logbooks or artifacts): the sidebar shows it, and an
# empty filter entry is cheaper than a list that drifts from the game's.
GAME_CATEGORIES = [
    "Currency",
    "Essences",
    "Delve",
    "Scarabs",
    "Divination Cards",
    "Delirium",
    "Legion",
    "Fragments",
    "Oils",
    "Catalysts",
    "Omens",
    "Tattoos",
    "Expedition",
    "Harvest",
    "Runegrafts",
    "Allflame",
]

# (id prefix, required substring or None, game category), matched against the id
# with METADATA_PREFIX stripped, first match wins. The order is load-bearing:
# every narrower "Currency/" rule precedes the "Currency/" catch-all and the
# three "MapFragments/" splits precede theirs, so appending a rule at the end
# rather than above its catch-all makes it dead.
CATEGORY_RULES = [
    ("Scarabs/", None, "Scarabs"),
    ("DivinationCards/", None, "Divination Cards"),
    ("Delve/", None, "Delve"),
    ("AtlasExiles/", None, "Currency"),
    # Of the Heist bucket only Rogue's Marker trades, and the sidebar files it
    # under Currency.
    ("Heist/", None, "Currency"),
    # Ducats and "Message in a Bottle"; the Deepwater charts themselves are not
    # on the exchange.
    ("Deepwater/", None, "Allflame"),
    ("Currency/CurrencyDeepwater", None, "Allflame"),  # Dead Man's Sulphur
    ("MapFragments/AtlasMemory/", None, "Fragments"),  # "Echo of ..."
    ("MapFragments/", "AllflamePack", "Allflame"),  # "Allflame Ember of ..."
    ("MapFragments/", "Legion", "Legion"),  # emblems and legion splinters
    ("MapFragments/", None, "Fragments"),  # incl. breach splinters and stones
    ("Currency/CurrencyEssence", None, "Essences"),
    ("Currency/CurrencyAffliction", None, "Delirium"),
    ("Currency/Mushrune", None, "Oils"),
    ("Currency/CurrencyJewelleryQuality", None, "Catalysts"),
    ("Currency/Runegraft", None, "Runegrafts"),
    ("Currency/HarvestSeed", None, "Harvest"),
    # Ancestral omens and tattoos share a prefix in the feed and split by id.
    ("Currency/AncestralOmen", None, "Omens"),
    ("Currency/AncestralTattoo", None, "Tattoos"),
    ("Currency/CurrencyDelveCrafting", None, "Delve"),  # fossils
    ("Currency/CurrencyLegion", None, "Legion"),  # timeless splinters
    ("Currency/LegionCocoon", None, "Legion"),  # Enshrouding Crystals
    # Catch-all: astrolabes, memories, Kalguuran, Sanctum, Incubation, Sentinel,
    # Bestiary, scouting reports, Eldritch.
    ("Currency/", None, "Currency"),
]

# A polite, identifying User-Agent. poewiki asks for one and answers a generic
# urllib agent with a block.
USER_AGENT = "ProfitOfExile/1.0 (currency-exchange-items)"

# cargo caps a single query at 500 rows regardless of what limit asks for.
CARGO_PAGE = 500

# MediaWiki caps an anonymous query at 50 titles, so this is the largest legal
# batch, not a tuning choice.
IMAGEINFO_BATCH = 50

# A File title can be normalized and then redirected; a couple of hops is more
# than the wiki produces and the bound keeps a redirect cycle from spinning.
MAX_ALIAS_HOPS = 5

# Give up on a single URL after this many attempts, doubling the wait each time.
MAX_ATTEMPTS = 5
BACKOFF_BASE_SECONDS = 1.0

# Abort the icon pass once this many requests fail back to back. A wiki-side
# block fails every request, and grinding through a thousand of them helps
# nobody.
CONSECUTIVE_FAILURE_LIMIT = 10

# safeFileName in internal/gemicon: runs of non-alphanumerics collapse to "_",
# leading and trailing ones are trimmed. This is the NAME half of the cache
# filename on both sides; since POE-136 a "-<16 hex of sha256(url)>" suffix
# follows it (short_hash / cache_file_name in scripts/download-gem-icons.py).
# Neither half may drift apart from the server's.
_UNSAFE = re.compile(r"[^A-Za-z0-9]+")


def safe_name(name: str) -> str:
    """Reduce an id to the cache filename token gemicon.safeFileName produces."""
    return _UNSAFE.sub("_", name).strip("_")


# --- http --------------------------------------------------------------------


class Fetcher:
    """Rate-limited HTTP client with bounded retries."""

    def __init__(self, rate: float, verbose: bool = False) -> None:
        self.min_interval = 1.0 / rate if rate > 0 else 0.0
        self.verbose = verbose
        self._last = 0.0
        self.requests = 0

    def _wait(self) -> None:
        if self.min_interval <= 0:
            return
        elapsed = time.monotonic() - self._last
        if elapsed < self.min_interval:
            time.sleep(self.min_interval - elapsed)
        self._last = time.monotonic()

    def get(self, url: str, method: str = "GET", timeout: int = 60):
        """Return (status, final_url, body). Raises RuntimeError when exhausted.

        A 404 is returned rather than raised: a missing wiki File page is a data
        fact about one item, not a transport failure.
        """
        last_error = None
        for attempt in range(1, MAX_ATTEMPTS + 1):
            self._wait()
            self.requests += 1
            request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
            request.get_method = lambda m=method: m
            try:
                with urllib.request.urlopen(request, timeout=timeout) as response:
                    return response.status, response.geturl(), response.read()
            except urllib.error.HTTPError as err:
                if err.code == 404:
                    return 404, url, b""
                last_error = f"HTTP {err.code} {err.reason}"
                # 429 and 503 may carry a server-chosen wait; honour it.
                retry_after = err.headers.get("Retry-After") if err.headers else None
                delay = _parse_retry_after(retry_after)
            except (urllib.error.URLError, TimeoutError, OSError) as err:
                last_error = str(err)
                delay = None
            if attempt == MAX_ATTEMPTS:
                break
            wait = delay if delay is not None else BACKOFF_BASE_SECONDS * (2 ** (attempt - 1))
            if self.verbose:
                print(f"    retry {attempt}/{MAX_ATTEMPTS} after {wait:.0f}s "
                      f"({last_error}): {url}", file=sys.stderr, flush=True)
            time.sleep(wait)
        raise RuntimeError(f"{url}: {last_error}")


def _parse_retry_after(value):
    if not value:
        return None
    try:
        seconds = float(value)
    except ValueError:
        return None
    # Cap a hostile Retry-After so one header cannot stall the run for an hour.
    return max(0.0, min(seconds, 120.0))


# --- sources -----------------------------------------------------------------


def fetch_base_items(fetcher: Fetcher) -> dict:
    print(f"Fetching base items from {BASE_ITEMS_URL}", flush=True)
    _, _, body = fetcher.get(BASE_ITEMS_URL)
    items = json.loads(body)
    print(f"  {len(items)} base items", flush=True)
    return items


def id_universe(base_items: dict) -> list:
    """Every metadata id in the eight exchange categories, sorted."""
    prefixes = tuple(f"{METADATA_PREFIX}{category}/" for category in CATEGORIES)
    return sorted(k for k in base_items if k.startswith(prefixes))


def category_of(item_id: str) -> str:
    parts = item_id.split("/")
    return parts[2] if len(parts) > 2 else "?"


def category_for(item_id: str) -> str | None:
    """Return the sidebar category for a metadata id, or None when no rule matches."""
    if not item_id.startswith(METADATA_PREFIX):
        return None
    relative = item_id[len(METADATA_PREFIX):]
    for prefix, contains, category in CATEGORY_RULES:
        if relative.startswith(prefix) and (contains is None or contains in relative):
            return category
    return None


def unknown_rule_categories() -> list:
    """Return the rule categories that are not one of GAME_CATEGORIES."""
    known = set(GAME_CATEGORIES)
    return sorted({category for _, _, category in CATEGORY_RULES if category not in known})


def fetch_cargo_rows(fetcher: Fetcher) -> dict:
    """Return metadata id -> {name, icon file} from poewiki's items table.

    cargo pages at 500 rows, so each category is walked with an offset until a
    short page comes back.
    """
    rows = {}
    for category in CATEGORIES:
        offset = 0
        found = 0
        while True:
            query = {
                "action": "cargoquery",
                "tables": "items",
                "fields": "items.metadata_id,items.name,items.inventory_icon,items.class",
                "where": f'items.metadata_id LIKE "Metadata/Items/{category}/%"',
                # Without an explicit order cargo's row order is unspecified, so
                # LIMIT/OFFSET paging could repeat or skip rows and the
                # first-row-wins dedup below could pick a different row per run.
                "order_by": "items.metadata_id",
                "limit": str(CARGO_PAGE),
                "offset": str(offset),
                "format": "json",
            }
            _, _, body = fetcher.get(f"{WIKI_API}?{urllib.parse.urlencode(query)}")
            page = json.loads(body).get("cargoquery", [])
            for entry in page:
                title = entry.get("title", {})
                item_id = title.get("metadata_id") or title.get("metadata id")
                if not item_id:
                    continue
                # cargo answers with HTML-escaped field values ("Gull&#039;s
                # Nest"), so both fields are unescaped here, at the source: an
                # escaped File title resolves to no icon and an escaped name
                # would ship into the asset verbatim.
                icon = html.unescape(
                    title.get("inventory_icon") or title.get("inventory icon") or "")
                name = html.unescape(title.get("name") or "")
                existing = rows.get(item_id)
                # Several wiki pages can share a metadata id (league variants of
                # one base). Keep the first, but upgrade to a later row if the
                # first carried no icon.
                if existing is None or (not existing["icon"] and icon):
                    rows[item_id] = {"name": name, "icon": icon}
            found += len(page)
            if len(page) < CARGO_PAGE:
                break
            offset += CARGO_PAGE
        print(f"  cargo {category}: {found} rows", flush=True)
    print(f"  {len(rows)} distinct metadata ids from cargo", flush=True)
    return rows


def resolve_icon_files(fetcher: Fetcher, files: list) -> dict:
    """Resolve wiki File names to their poewiki ``/images/`` URLs.

    Resolution is per distinct File, not per item: every divination card shares
    one card-back icon, so deduplicating here alone removes hundreds of lookups.

    The bulk path is the imageinfo API, which answers IMAGEINFO_BATCH files per
    request. The obvious alternative — one Special:FilePath request per file,
    reading the redirect target — needs one request per file instead of one per
    fifty, and measured against poewiki on 2026-08-19 that earns a 429 partway
    through the run. Special:FilePath survives only as the per-file fallback for
    anything a batch failed to answer.
    """
    resolved = {}
    total = len(files)
    batches = [files[i:i + IMAGEINFO_BATCH] for i in range(0, total, IMAGEINFO_BATCH)]
    print(f"Resolving {total} distinct icon files in {len(batches)} imageinfo batches",
          flush=True)
    for index, batch in enumerate(batches, 1):
        try:
            resolved.update(batch_image_urls(fetcher, batch))
        except (RuntimeError, ValueError, KeyError) as err:
            print(f"  batch {index}/{len(batches)} failed ({err}); "
                  f"falling back to Special:FilePath for its {len(batch)} files",
                  file=sys.stderr, flush=True)
        if index % 5 == 0 or index == len(batches):
            print(f"  batch {index}/{len(batches)} (resolved={len(resolved)})", flush=True)

    stragglers = [name for name in files if name not in resolved]
    if stragglers:
        resolved.update(resolve_via_filepath(fetcher, stragglers))

    missing = [name for name in files if name not in resolved]
    print(f"  resolved {len(resolved)}/{total} icon files", flush=True)
    for file_name in missing[:15]:
        print(f"  MISS  {file_name}", flush=True)
    if len(missing) > 15:
        print(f"  ... and {len(missing) - 15} more misses", flush=True)
    return resolved


def batch_image_urls(fetcher: Fetcher, batch: list) -> dict:
    """Return {requested File title: image URL} for one imageinfo batch."""
    query = {
        "action": "query",
        "format": "json",
        "formatversion": "2",
        "prop": "imageinfo",
        "iiprop": "url",
        # A File page can redirect to the page that actually holds the image
        # (renamed icons do this). Without these the redirect answers with no
        # imageinfo at all.
        "redirects": "1",
        "titles": "|".join(batch),
    }
    _, _, body = fetcher.get(f"{WIKI_API}?{urllib.parse.urlencode(query)}")
    payload = json.loads(body).get("query", {})

    # MediaWiki answers under the title it resolved to, not the one asked for,
    # so the normalization and redirect chains have to be walked back.
    alias = {}
    for chain in ("normalized", "redirects"):
        for entry in payload.get(chain, []):
            alias[entry["from"]] = entry["to"]

    by_title = {}
    for page in payload.get("pages", []):
        info = page.get("imageinfo") or []
        url = info[0].get("url") if info else None
        # Same guard as the Special:FilePath fallback: only a poewiki upload URL
        # is an icon. Anything else (an interwiki or foreign-repo file, an error
        # placeholder) counts as unresolved, so the file falls through to the
        # fallback path rather than being committed as an icon.
        if url and url.startswith(IMAGE_URL_PREFIX):
            by_title[page["title"]] = url

    resolved = {}
    for name in batch:
        title = name
        for _ in range(MAX_ALIAS_HOPS):
            if title in by_title or title not in alias:
                break
            title = alias[title]
        if title in by_title:
            resolved[name] = by_title[title]
    return resolved


def resolve_via_filepath(fetcher: Fetcher, files: list) -> dict:
    """Per-file fallback: follow Special:FilePath to the final image URL.

    One request per file, so this only runs for what the batches could not
    answer. If poewiki is blocking, the whole batch pass fails and every file
    lands here — hence the consecutive-failure abort, which stops a thousand
    doomed requests from being fired at a host that is already saying no.
    """
    resolved = {}
    consecutive_failures = 0
    print(f"  Special:FilePath fallback for {len(files)} files", flush=True)
    for file_name in files:
        # cargo gives "File:Chaos Orb inventory icon.png"; FilePath wants the
        # bare page name.
        bare = file_name.split(":", 1)[1] if file_name.startswith("File:") else file_name
        url = f"{WIKI_INDEX}?" + urllib.parse.urlencode({"title": f"Special:FilePath/{bare}"})
        try:
            # The initial request is a HEAD; urllib re-issues the redirect hop
            # as a GET, so the image bytes are downloaded and discarded. Only
            # the final URL is wanted, and this path runs for a handful of
            # stragglers, so the wasted bytes are not worth a manual redirect
            # walk.
            status, final_url, _ = fetcher.get(url, method="HEAD", timeout=30)
        except RuntimeError as err:
            consecutive_failures += 1
            if consecutive_failures >= CONSECUTIVE_FAILURE_LIMIT:
                raise RuntimeError(
                    f"{consecutive_failures} consecutive requests failed (last: {err}). "
                    f"poewiki is rate-limiting or blocking this host; wait for the window "
                    f"to clear and re-run rather than retrying now"
                )
            continue
        consecutive_failures = 0
        if status != 404 and "/images/" in final_url:
            resolved[file_name] = final_url
    return resolved


# --- assembly ----------------------------------------------------------------


def build_items(universe: list, base_items: dict, cargo: dict, icon_urls: dict,
                carried_icons: dict = None):
    """Return (items, skipped) — id -> {name, icon, category} for every named id.

    An id with no name in either source is a drop-table placeholder rather than
    an item (RePoE carries 420 unnamed ``RandomFossilOutcome<N>`` entries in the
    Currency namespace). They are reported and excluded rather than shipped with
    a fabricated name.

    ``carried_icons`` (id -> URL) is the previous asset's icons, passed only by
    a ``--skip-icons`` run, which resolves nothing: without it the names-only
    output would drop every icon while ``--icons-out`` kept them, desyncing the
    committed pair. A normal run passes None so the resolved set is the whole
    truth and a removed icon really disappears.
    """
    items = {}
    skipped = []
    for item_id in universe:
        name = base_items[item_id].get("name") or cargo.get(item_id, {}).get("name") or ""
        if not name:
            skipped.append(item_id)
            continue
        icon_file = cargo.get(item_id, {}).get("icon") or ""
        icon = icon_urls.get(icon_file) or None
        if icon is None and carried_icons:
            icon = carried_icons.get(item_id)
        items[item_id] = {"name": name, "icon": icon, "category": category_for(item_id)}
    return items, skipped


def load_existing_icons(path: str):
    """Return id -> icon URL from an already-written asset, or None if unusable."""
    try:
        with open(path, encoding="utf-8") as handle:
            existing = json.load(handle)
    except (OSError, ValueError):
        return None
    if not isinstance(existing, dict):
        return None
    return {
        item_id: entry["icon"]
        for item_id, entry in existing.items()
        if isinstance(entry, dict) and entry.get("icon")
    }


def check_safe_name_uniqueness(items: dict) -> list:
    """Return the safe-name collisions, if any.

    The server derives the name half of an icon's cache filename from the id
    with the same scheme. Since POE-136 a hash of the source URL follows it, so
    two ids collapsing to one token only collide when they also share a URL —
    in which case they legitimately share artwork anyway. The check stays as
    defence in depth: a colliding token is still an unreadable cache directory
    and a sign the id space grew a shape this scheme was not written for.
    """
    by_token = defaultdict(list)
    for item_id in items:
        by_token[safe_name(item_id)].append(item_id)
    return [(token, ids) for token, ids in sorted(by_token.items()) if len(ids) > 1]


def write_json(path: str, payload) -> int:
    directory = os.path.dirname(path)
    if directory:
        os.makedirs(directory, exist_ok=True)
    text = json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text)
    return len(text.encode("utf-8"))


def report(universe, items, skipped, cargo, icon_files_count) -> None:
    named = len(items)
    named_cargo, cargo_known = _cargo_name_coverage(items, cargo)
    with_icon = sum(1 for entry in items.values() if entry["icon"])
    print("")
    print("Coverage")
    print(f"  id universe (RePoE, {len(CATEGORIES)} exchange categories): {len(universe)}")
    print(f"  unnamed drop-table placeholders excluded:     {len(skipped)}")
    print(f"  named items:                                 {named}")
    print(f"  names {named_cargo}/{cargo_known} of the ids poewiki knows "
          f"({_pct(named_cargo, cargo_known)})")
    print(f"  icons {with_icon}/{named} of named items ({_pct(with_icon, named)}), "
          f"{icon_files_count} distinct icon files")
    print("  per category:")
    per_category = defaultdict(lambda: [0, 0, 0])
    for item_id in universe:
        stats = per_category[category_of(item_id)]
        stats[0] += 1
        if item_id in items:
            stats[1] += 1
            if items[item_id]["icon"]:
                stats[2] += 1
    for category in CATEGORIES:
        total, named_count, icon_count = per_category[category]
        print(f"    {category:<16} universe {total:>5}  names {named_count:>5}  "
              f"icons {icon_count:>5} ({_pct(icon_count, named_count)})")

    print("  per game category:")
    per_game = defaultdict(lambda: [0, 0])
    for item_id in universe:
        stats = per_game[category_for(item_id)]
        stats[0] += 1
        if item_id in items:
            stats[1] += 1
    for category in GAME_CATEGORIES:
        total, named_count = per_game[category]
        print(f"    {category:<16} universe {total:>5}  names {named_count:>5}")
    # None means the id matched no rule, which the gate below fails on; the
    # count is printed anyway so a failing run says how big the hole is.
    unruled = per_game[None][0]
    if unruled:
        print(f"    {'(no rule)':<16} universe {unruled:>5}  names {per_game[None][1]:>5}")


def _cargo_name_coverage(items: dict, cargo: dict):
    """Return (named, total) over the ids poewiki lists as items.

    This is the name gate's denominator, and it is deliberately the independent
    source: poewiki says "this metadata id is a real, tradeable item" and RePoE
    supplies the name. Measuring names against the RePoE universe instead would
    either fail permanently on the unnamed placeholders or, once they are
    excluded, pass by construction — the excluded set is defined by the same
    field the gate measures.
    """
    total = len(cargo)
    named = sum(1 for item_id in cargo if item_id in items)
    return named, total


def _pct(part: int, whole: int) -> str:
    if whole == 0:
        return "n/a"
    return f"{100.0 * part / whole:.1f}%"


# --- main --------------------------------------------------------------------


def parse_args(argv):
    parser = argparse.ArgumentParser(
        description="Generate the Currency Exchange item names + icons + categories asset.",
    )
    parser.add_argument("--out", default="internal/exchange/itemdata/items.json",
                        help="id -> {name, icon, category} asset embedded by internal/exchange")
    parser.add_argument("--icons-out", default="internal/exchange/itemdata/icon-urls.json",
                        help="flat id -> icon URL map consumed by scripts/download-gem-icons.py")
    parser.add_argument("--rate", type=float, default=2.0,
                        help="maximum requests per second to poewiki (default: 2)")
    parser.add_argument("--name-coverage", type=float, default=0.95,
                        help="fail below this share of the named universe (default: 0.95)")
    parser.add_argument("--min-items", type=int, default=1500,
                        help="sanity floor on the named universe, so a collapsed upstream "
                             "cannot satisfy --name-coverage vacuously (default: 1500)")
    parser.add_argument("--min-icons", type=int, default=1200,
                        help="sanity floor on resolved icons (default: 1200)")
    parser.add_argument("--skip-icons", action="store_true",
                        help="names only: skip the icon pass and the icon gate, "
                             "carrying the icons already in --out (which must "
                             "exist) and leaving --icons-out untouched")
    parser.add_argument("--verbose", action="store_true", help="log retries")
    return parser.parse_args(argv)


def main(argv) -> int:
    args = parse_args(argv)
    started = time.monotonic()

    # A rule table that names a category the sidebar does not have would ship a
    # category the desktop filter cannot render. Checked before the first
    # request: it is a table typo, not an upstream fact.
    stray = unknown_rule_categories()
    if stray:
        print(f"FAIL: category rules name {len(stray)} categories outside the "
              f"{len(GAME_CATEGORIES)} game categories: {', '.join(stray)}", file=sys.stderr)
        print("Nothing written.", file=sys.stderr)
        return 1

    fetcher = Fetcher(args.rate, verbose=args.verbose)

    # Checked before the first request: a names-only run rewrites --out without
    # resolving anything, so with no previous asset to carry icons from it would
    # publish a zero-icon --out beside an untouched --icons-out.
    carried_icons = None
    if args.skip_icons:
        carried_icons = load_existing_icons(args.out)
        if carried_icons is None:
            print("ERROR: names-only run needs an existing asset; "
                  "run without --skip-icons", file=sys.stderr)
            return 1

    try:
        base_items = fetch_base_items(fetcher)
        universe = id_universe(base_items)
        print(f"  {len(universe)} ids in the exchange categories", flush=True)
        print("Fetching poewiki cargo rows", flush=True)
        cargo = fetch_cargo_rows(fetcher)
    except (RuntimeError, ValueError) as err:
        print(f"ERROR: upstream unreadable: {err}", file=sys.stderr)
        return 2

    icon_files = sorted({row["icon"] for row in cargo.values() if row["icon"]})
    if args.skip_icons:
        print(f"Skipping the icon pass (--skip-icons); carrying "
              f"{len(carried_icons)} icons from {args.out}", flush=True)
        icon_urls = {}
    else:
        try:
            icon_urls = resolve_icon_files(fetcher, icon_files)
        except RuntimeError as err:
            print(f"ERROR: icon resolution failed: {err}", file=sys.stderr)
            return 2

    items, skipped = build_items(universe, base_items, cargo, icon_urls, carried_icons)
    report(universe, items, skipped, cargo, len(icon_files))
    if skipped:
        shapes = sorted({re.sub(r"\d+$", "<N>", s.split("/")[-1]) for s in skipped})
        print(f"  excluded shapes: {', '.join(shapes[:8])}"
              + (" ..." if len(shapes) > 8 else ""))

    failures = []

    # An id no rule matches would ship with a null category, which reaches the
    # client as "unfiltered" — a silent hole. A widened upstream bucket lands
    # here, and the fix is one more row in CATEGORY_RULES.
    uncategorized = [item_id for item_id in universe if category_for(item_id) is None]
    for item_id in uncategorized[:10]:
        print(f"  NO RULE  {item_id}", file=sys.stderr)
    if len(uncategorized) > 10:
        print(f"  ... and {len(uncategorized) - 10} more", file=sys.stderr)
    if uncategorized:
        failures.append(f"{len(uncategorized)} universe ids match no category rule")

    collisions = check_safe_name_uniqueness(items)
    for token, ids in collisions[:10]:
        print(f"  COLLISION {token}: {', '.join(ids)}", file=sys.stderr)
    if collisions:
        failures.append(f"{len(collisions)} safe-name collisions across ids")

    named = len(items)
    named_cargo, cargo_known = _cargo_name_coverage(items, cargo)
    if cargo_known and named_cargo / cargo_known < args.name_coverage:
        failures.append(f"name coverage {_pct(named_cargo, cargo_known)} of the ids "
                        f"poewiki knows, below {args.name_coverage:.0%}")
    if named < args.min_items:
        failures.append(f"named universe {named} below --min-items {args.min_items}")

    with_icon = sum(1 for entry in items.values() if entry["icon"])
    if not args.skip_icons and with_icon < args.min_icons:
        failures.append(f"resolved icons {with_icon} below --min-icons {args.min_icons}")

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        print("Nothing written.", file=sys.stderr)
        return 1

    items_bytes = write_json(args.out, items)
    print(f"\nWrote {args.out}: {len(items)} entries, {items_bytes} bytes")
    if not args.skip_icons:
        flat = {item_id: entry["icon"] for item_id, entry in items.items() if entry["icon"]}
        icons_bytes = write_json(args.icons_out, flat)
        print(f"Wrote {args.icons_out}: {len(flat)} entries, {icons_bytes} bytes")
    print(f"{fetcher.requests} requests in {time.monotonic() - started:.0f}s")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
