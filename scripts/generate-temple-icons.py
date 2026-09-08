#!/usr/bin/env python3
"""Generate the temple icon map, ``internal/icons/urls/temple.json``.

The temple endpoint (``GET /api/analysis/temple-market``, POE-255) serves an
icon path per room-tier and per recipe member. Icons are served by the existing
cache (``GET /api/gem-icon/{name}``, ADR-012): it needs a committed
name-to-upstream-URL map, and adding one is a new ``*.json`` file under
``internal/icons/urls/`` with no Go change.

Unlike the gem and currency-exchange maps, whose URLs come from poewiki, these
come from poe.ninja's own ``icon`` field on the item-overview lines the
collector already polls (``web.poecdn.com/gen/image/...``). That URL is
content-addressed and stable per artwork, so the map only needs regenerating
when an item's art changes or a name is added to the recipe table.

The names are read from ``internal/temple/recipes.go`` rather than restated
here, so this script cannot drift from the served item set. All 86 room-tier
lines share one artwork (the Chronicle of Atzoatl / TempleMap image), so they
get the single entry ``ROOM_ICON_NAME``, which must equal
``temple.RoomIconName``. ``TestIconMap_coversTheServedSetAndTheRoomArtwork``
re-checks both halves against the committed file on every test run.

Run it from the repository root, once per league or after editing the recipe
table::

    python3 scripts/generate-temple-icons.py --league Allflame

The output is deterministic — one key per line, sorted — so an unchanged
upstream produces a zero-length diff.

After regenerating, pre-seed the production icon volume BEFORE deploying: the
production VPS cannot fetch upstream, so a map entry deployed ahead of its
cached bytes is a permanent 502 for that name (ADR-012). See
docs/GEM-ICONS.md, "Adding an icon", steps 3-6.

Exit codes: 0 success, 1 a coverage gate failed, 2 an upstream could not be
read.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

# poe.ninja's item-overview endpoint, the same one internal/collector polls.
NINJA_URL = "https://poe.ninja/poe1/api/economy/stash/current/item/overview"

# The seven categories, mirroring internal/temple/feeds.go. The room category is
# read only for its shared artwork.
ROOM_CATEGORY = "IncursionTemple"
ITEM_CATEGORIES = [
    "Vial",
    "UniqueArmour",
    "UniqueAccessory",
    "UniqueWeapon",
    "UniqueJewel",
    "UniqueFlask",
]

# The single key every room-tier's icon path is built from. Must equal
# temple.RoomIconName.
ROOM_ICON_NAME = "Chronicle of Atzoatl"

REPO_ROOT = Path(__file__).resolve().parent.parent
RECIPES_GO = REPO_ROOT / "internal" / "temple" / "recipes.go"
OUTPUT = REPO_ROOT / "internal" / "icons" / "urls" / "temple.json"

RECIPE_LINE = re.compile(
    r'\{Vial: "([^"]+)", Base: "([^"]+)", Upgraded: "([^"]+)"\}'
)


def served_item_names(recipes_go: Path) -> list[str]:
    """The union of every recipe's three members, sorted — temple.ItemNames()."""
    matches = RECIPE_LINE.findall(recipes_go.read_text(encoding="utf-8"))
    if not matches:
        sys.exit(f"error: no recipe rows parsed from {recipes_go}")
    names = {name for row in matches for name in row}
    return sorted(names)


def fetch(league: str, category: str) -> list[dict]:
    query = urllib.parse.urlencode({"league": league, "type": category})
    url = f"{NINJA_URL}?{query}"
    request = urllib.request.Request(url, headers={"User-Agent": "profitofexile-temple-icons"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response).get("lines", [])
    except (urllib.error.URLError, json.JSONDecodeError) as err:
        # Print the cause before exiting 2. A bare exit code says only "an
        # upstream could not be read", which is the same answer for a DNS
        # failure, a 429 and a truncated body — three different next steps.
        print(f"error: fetch {category} from {url}: {err}", file=sys.stderr)
        sys.exit(2)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--league", default="Allflame", help="poe.ninja league name")
    args = parser.parse_args()

    wanted = served_item_names(RECIPES_GO)
    icons: dict[str, str] = {}

    rooms = fetch(args.league, ROOM_CATEGORY)
    room_icons = {line["icon"] for line in rooms if line.get("icon")}
    if len(room_icons) != 1:
        sys.exit(
            f"error: expected one shared artwork across {len(rooms)} {ROOM_CATEGORY} "
            f"lines, found {len(room_icons)}"
        )
    icons[ROOM_ICON_NAME] = room_icons.pop()

    for category in ITEM_CATEGORIES:
        for line in fetch(args.league, category):
            name, icon = line.get("name"), line.get("icon")
            if name in wanted and icon and name not in icons:
                icons[name] = icon

    missing = [name for name in wanted if name not in icons]
    if missing:
        sys.exit(f"error: no icon on the {args.league} feed for: {', '.join(missing)}")

    OUTPUT.write_text(
        json.dumps({key: icons[key] for key in sorted(icons)}, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(f"wrote {OUTPUT} ({len(icons)} entries: 1 room artwork + {len(wanted)} items)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
