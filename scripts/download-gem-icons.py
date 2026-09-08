#!/usr/bin/env python3
"""Seed, migrate and prune an icon cache directory.

poewiki 403s datacenter IPs, so the server (internal/icons) cannot fetch icons
at runtime in production — its disk cache must be seeded from an allowed IP.

This writes files using the SAME cache-filename scheme as the server:

    <safe_name(name)>-<short_hash(url)>.png

`safe_name` mirrors icons.safeFileName (runs of [^A-Za-z0-9] -> "_", trimmed)
and `short_hash` mirrors icons.shortHash (first 16 hex characters of the
SHA-256 of the SOURCE URL). The URL is in the filename because the server has no
other invalidation: it returns its disk copy unconditionally when the file
exists, so before POE-136 a corrected URL kept serving the old artwork forever.
Content-addressing makes a URL edit a different filename, hence a cache miss,
hence a fetch.

That is also why this script has to agree with the server exactly: production
reads what this writes and cannot recover by fetching. `_self_check()` runs at
import and pins the full filename for one vector; the Go side pins the same one
in TestFilePath_isSafeNameDashURLHashPNG / TestShortHash_pinnedVector
(internal/icons/icons_test.go). Change the scheme and both must move.

MAP may be a single flat name -> URL file OR a DIRECTORY of category files
(POE-135). A directory is validated by merging its `*.json` in sorted order,
exactly as the server's alias index does, and then pulled one file at a time.
Each category file goes to OUT/<file basename>; a single file keeps writing to
OUT itself. A key present in two category files aborts before any pull starts.

The server keeps ONE cache root (ICON_CACHE_DIR, /data/icons-cache in
production) with ONE SUB-DIRECTORY PER SOURCE MAP, because every set shares this
filename scheme and a flat directory would let two keys reduce to the same file
whenever they also share a source URL. For a map directory, point OUT at the
root; for a single map file, OUT remains the directory to write:

    icons-cache                    internal/icons/urls  (a directory)
    icons-cache/currency-exchange  internal/exchange/itemdata/icon-urls.json

Usage:
    python3 scripts/download-gem-icons.py pull [MAP] [OUT]
    python3 scripts/download-gem-icons.py migrate [OUT] [--map MAP]
    python3 scripts/download-gem-icons.py prune [OUT] [--map MAP] [--dry-run] [--force]

    pull     Fetch every mapped icon that is not already on disk under its
             content-addressed name. A map directory fans out by basename. The
             only mode that touches the network.
    migrate  Rename old name-only files (`<safe>.png`, the pre-POE-136 scheme)
             to their content-addressed names, offline. This is what turns an
             existing production cache over without a full re-crawl.
    prune    Sweep every OUT/<map basename> pair when MAP is a directory; a
             single MAP file sweeps OUT itself. Delete every `*.png` the paired
             map does not produce. `--dry-run` lists them and deletes nothing.
             The wrong-pair refusal is evaluated independently for each pair, and
             `--force` overrides it.

`migrate` and `prune` default `--map` to the embedded category directory; pass
the item map when you are working on the `currency-exchange/` sub-directory.

Then ship OUT into the matching sub-directory of the prod icon-cache volume
(see docs/ICONS.md) and the server serves every icon from disk with no
upstream fetch.
"""
import argparse, glob, hashlib, json, os, re, sys, time, urllib.request

DEFAULT_MAP = "internal/icons/urls"
DEFAULT_OUT = "icons-cache"
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
_unsafe = re.compile(r"[^A-Za-z0-9]+")


def safe_name(name: str) -> str:
    """The NAME half of the cache filename — icons.safeFileName."""
    return _unsafe.sub("_", name).strip("_")


def short_hash(url: str) -> str:
    """The URL half of the cache filename — icons.shortHash.

    First 16 hex characters (8 bytes) of the SHA-256 of the URL. Short because
    it only has to discriminate within one safe_name bucket, which normally
    holds exactly one URL.
    """
    return hashlib.sha256(url.encode("utf-8")).hexdigest()[:16]


def cache_file_name(name: str, url: str) -> str:
    """The full cache filename the server reads for (name, url).

    The "-" is unambiguous: safe_name emits [A-Za-z0-9_] only, so the last "-"
    always starts the hash. ".png" is constant on both sides — every source is a
    poewiki `*_inventory_icon.png` and the server serves a constant image/png.
    """
    return f"{safe_name(name)}-{short_hash(url)}.png"


def _self_check() -> None:
    """Fail at import if this file's scheme has drifted from the server's.

    The vector is pinned in Go too (internal/icons/icons_test.go). Checking
    the FULL filename rather than the hash alone is deliberate: a changed joiner
    or extension writes files the server never looks for just as surely as a
    changed hash, and on production that is a permanent 502 per icon, because
    poewiki 403s the VPS and the server cannot recover by fetching.
    """
    url = "https://www.poewiki.net/images/c/c6/Absolution_inventory_icon.png"
    want = "Absolution-e2b9dfdb1dd1d6a0.png"
    got = cache_file_name("Absolution", url)
    if got != want:
        raise AssertionError(
            f"cache filename scheme drifted: cache_file_name('Absolution', {url!r}) "
            f"= {got!r}, want {want!r} — the server (internal/icons) pins this vector"
        )


_self_check()


def load_map(path: str) -> dict:
    """Read one flat map file, or merge every *.json in a directory.

    Mirrors internal/icons's loader: sorted file order, and a duplicate key
    across two files is fatal and names both files, never a silent winner.
    """
    if not os.path.isdir(path):
        with open(path) as f:
            return json.load(f)
    files = sorted(glob.glob(os.path.join(path, "*.json")))
    if not files:
        raise SystemExit(f"no *.json in {path}")
    merged, source = {}, {}
    for file in files:
        with open(file) as f:
            part = json.load(f)
        for name in sorted(part):
            if name in merged:
                raise SystemExit(
                    f"duplicate icon key {name!r} in {source[name]} and {file}"
                )
            merged[name] = part[name]
            source[name] = file
    return merged


def map_pairs(map_path: str, out: str):
    """Return (flat map file, cache directory) pairs for a command.

    A directory map is validated as one merged source before its individual
    category pairs are returned. A flat map is the one deliberate exception to
    the root layout: its OUT argument is already the destination directory.
    """
    if not os.path.isdir(map_path):
        return [(map_path, out)]

    files = sorted(glob.glob(os.path.join(map_path, "*.json")))
    if not files:
        raise SystemExit(f"no *.json in {map_path}")
    # Validate duplicate keys before a command changes any pair.
    load_map(map_path)
    return [
        (file, os.path.join(out, os.path.splitext(os.path.basename(file))[0]))
        for file in files
    ]


def pull_one(map_path: str, out: str):
    """Fetch one flat map into one cache directory."""
    os.makedirs(out, exist_ok=True)
    m = load_map(map_path)
    total, ok, skip, fail = len(m), 0, 0, []
    for i, (name, url) in enumerate(sorted(m.items()), 1):
        fn = os.path.join(out, cache_file_name(name, url))
        if os.path.exists(fn) and os.path.getsize(fn) > 0:
            skip += 1
            ok += 1
            continue
        try:
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=20) as r:
                data = r.read()
            if not data.startswith(PNG_MAGIC):
                raise ValueError(f"not a PNG (got {len(data)}b, head={data[:16]!r})")
            with open(fn, "wb") as f:
                f.write(data)
            ok += 1
        except Exception as e:  # noqa: BLE001 - report and continue
            fail.append((name, str(e)))
        if i % 100 == 0:
            print(f"  {i}/{total} (ok={ok} fail={len(fail)})", flush=True)
        time.sleep(0.12)  # be polite to the wiki
    return ok, skip, fail


def pull(map_path: str, out: str) -> int:
    """Fetch every mapped icon missing from its paired cache directory."""
    pairs = map_pairs(map_path, out)
    total_ok, total_skip, total_fail = 0, 0, []
    for pair_map, pair_out in pairs:
        ok, skip, fail = pull_one(pair_map, pair_out)
        total_ok += ok
        total_skip += skip
        total_fail.extend((pair_map, name, error) for name, error in fail)
        print(f"  {pair_map}: {ok}/{ok + len(fail)} present -> {pair_out}")
    total = total_ok + len(total_fail)
    print(f"\nDONE: {total_ok}/{total} present ({total_skip} already had), "
          f"{len(total_fail)} failed -> {out}")
    for pair_map, name, error in total_fail[:30]:
        print(f"  FAIL  {pair_map}: {name}: {error}")
    return 0


def migrate_one(map_path: str, out: str) -> tuple[int, int, int]:
    """Rename pre-POE-136 `<safe>.png` files to their content-addressed names.

    Offline on purpose: an existing production cache already holds the bytes,
    and re-pulling them would be a full poewiki crawl from an allowed IP. A file
    is renamed only when the old name exists and the new one does not, so the
    mode is idempotent and never overwrites a correctly named file.

    The two ways an entry can go unrenamed are reported apart, because they mean
    opposite things. `already_addressed` (the new name is on disk) is the
    idempotent re-run and is healthy. `no_old_file` (neither name is on disk) is
    an entry this directory never held — expected only for map entries added
    since the copy was taken, and otherwise the sign of an incomplete staging
    copy or the wrong --map. A single `skipped` count hid one inside the other.
    """
    m = load_map(map_path)
    renamed, already_addressed, no_old_file = 0, 0, 0
    for name, url in sorted(m.items()):
        old = os.path.join(out, safe_name(name) + ".png")
        new = os.path.join(out, cache_file_name(name, url))
        if os.path.exists(new):
            already_addressed += 1
        elif os.path.exists(old):
            os.rename(old, new)
            renamed += 1
        else:
            no_old_file += 1
    return renamed, already_addressed, no_old_file


def migrate(map_path: str, out: str) -> int:
    """Migrate every paired cache directory without fetching."""
    totals = [0, 0, 0]
    for pair_map, pair_out in map_pairs(map_path, out):
        values = migrate_one(pair_map, pair_out)
        totals = [left + right for left, right in zip(totals, values)]
        print(f"  {pair_map}: renamed {values[0]}, already_addressed {values[1]}, "
              f"no_old_file {values[2]} of {len(load_map(pair_map))} -> {pair_out}")
    print(f"DONE: renamed {totals[0]}, already_addressed {totals[1]}, "
          f"no_old_file {totals[2]} -> {out}")
    return 0


class PruneRefused(Exception):
    """prune's blast-radius guard tripped — see prune's docstring."""


def prune_one(map_path: str, out: str, dry_run: bool, force: bool = False) -> None:
    """Delete every *.png in out the current map does not produce.

    A URL correction leaves the file under the old hash behind; nothing in the
    server ever reads or removes it. The set of wanted filenames is a pure
    function of the map, so this is safe to re-run — but it is only as correct
    as the map/directory pairing, which is why both are printed.

    That pairing is also the one way to lose a whole seeded directory: pointing
    the gem map at `currency-exchange/` produces a wanted set disjoint from
    everything on disk, and every file is "superseded". The bytes cost a
    poewiki crawl from an allowed IP to recreate, so two blast-radius checks
    refuse instead of deleting — none of the on-disk files are wanted, or more
    than half of them would go. Both are shapes a correct sweep does not have:
    a real one drops the handful of files a URL edit stranded. The refusal
    prints the map, the directory and the counts, because the pairing is the
    thing to check. `--force` is the override for the rare legitimate case (a
    map that genuinely dropped most of its entries).

    A missing category directory is also a refusal: it is an unseeded pair, not
    a clean no-op. An existing empty (or already-clean) directory is allowed,
    because there is nothing to delete. The guard binds `--dry-run` too. A
    refused dry run prints the refusal
    instead of the list: the list is what an operator reads to decide, and
    handing them 700 lines of "WOULD DELETE" from a wrong pairing is exactly
    the confirmation that gets it run for real.
    """
    m = load_map(map_path)
    wanted = {cache_file_name(name, url) for name, url in m.items()}
    if not os.path.isdir(out) and not force:
        raise PruneRefused(
            f"prune refused: cache directory {out} does not exist for {map_path}"
        )
    on_disk = sorted(glob.glob(os.path.join(out, "*.png")))
    superseded = [p for p in on_disk if os.path.basename(p) not in wanted]
    keeping = len(on_disk) - len(superseded)

    # An empty (or already-clean) directory is not a blast radius: with nothing
    # to delete there is nothing to refuse, and tripping on "0 of 0 wanted"
    # would fail every no-op re-run.
    if superseded and not force:
        if keeping == 0:
            raise PruneRefused(
                f"prune refused: 0 of {len(on_disk)} files in {out} are produced by "
                f"{map_path} — wrong --map/OUT pairing? pass --force to override"
            )
        if len(superseded) * 2 > len(on_disk):
            raise PruneRefused(
                f"prune refused: {len(superseded)} of {len(on_disk)} files in {out} "
                f"would be deleted, more than half; only {keeping} are produced by "
                f"{map_path} — wrong --map/OUT pairing? pass --force to override"
            )

    for path in superseded:
        if not dry_run:
            os.remove(path)
        print(("  WOULD DELETE  " if dry_run else "  DELETED  ") + path)
    verb = "would delete" if dry_run else "deleted"
    print(f"DONE: {verb} {len(superseded)} superseded file(s) in {out} "
          f"({len(wanted)} wanted by {map_path})")
    return None


def prune(map_path: str, out: str, dry_run: bool, force: bool = False) -> int:
    """Prune every paired cache directory and return 1 if any pair refuses."""
    refused = False
    for pair_map, pair_out in map_pairs(map_path, out):
        try:
            prune_one(pair_map, pair_out, dry_run, force)
        except PruneRefused as e:
            refused = True
            print(e, file=sys.stderr)
    return 1 if refused else 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        description="Seed, migrate and prune an icon cache directory.",
    )
    modes = parser.add_subparsers(dest="mode", required=True)

    p = modes.add_parser("pull", help="fetch every mapped icon missing from OUT")
    p.add_argument("map", nargs="?", default=DEFAULT_MAP, metavar="MAP",
                   help=f"map file or directory of category files (default: {DEFAULT_MAP})")
    p.add_argument("out", nargs="?", default=DEFAULT_OUT, metavar="OUT",
                   help=f"cache root, or single-map cache directory (default: {DEFAULT_OUT})")

    g = modes.add_parser("migrate", help="rename old name-only files to content-addressed names")
    g.add_argument("out", nargs="?", default=DEFAULT_OUT, metavar="OUT",
                   help=f"cache root, or single-map cache directory (default: {DEFAULT_OUT})")
    g.add_argument("--map", default=DEFAULT_MAP, metavar="MAP",
                   help=f"map file or directory (default: {DEFAULT_MAP})")

    r = modes.add_parser("prune", help="delete files the current map does not produce")
    r.add_argument("out", nargs="?", default=DEFAULT_OUT, metavar="OUT",
                   help=f"cache root, or single-map cache directory (default: {DEFAULT_OUT})")
    r.add_argument("--map", default=DEFAULT_MAP, metavar="MAP",
                   help=f"map file or directory (default: {DEFAULT_MAP})")
    r.add_argument("--dry-run", action="store_true",
                   help="list what would be deleted and delete nothing")
    r.add_argument("--force", action="store_true",
                   help="delete even when the blast-radius guard refuses")

    args = parser.parse_args(argv)
    if args.mode == "pull":
        return pull(args.map, args.out)
    if args.mode == "migrate":
        return migrate(args.map, args.out)
    return prune(args.map, args.out, args.dry_run, args.force)


if __name__ == "__main__":
    sys.exit(main())
