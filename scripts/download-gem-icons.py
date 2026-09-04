#!/usr/bin/env python3
"""Seed, migrate and prune an icon cache directory.

poewiki 403s datacenter IPs, so the server (internal/gemicon) cannot fetch icons
at runtime in production — its disk cache must be seeded from an allowed IP.

This writes files using the SAME cache-filename scheme as the server:

    <safe_name(name)>-<short_hash(url)>.png

`safe_name` mirrors gemicon.safeFileName (runs of [^A-Za-z0-9] -> "_", trimmed)
and `short_hash` mirrors gemicon.shortHash (first 16 hex characters of the
SHA-256 of the SOURCE URL). The URL is in the filename because the server has no
other invalidation: it returns its disk copy unconditionally when the file
exists, so before POE-136 a corrected URL kept serving the old artwork forever.
Content-addressing makes a URL edit a different filename, hence a cache miss,
hence a fetch.

That is also why this script has to agree with the server exactly: production
reads what this writes and cannot recover by fetching. `_self_check()` runs at
import and pins the full filename for one vector; the Go side pins the same one
in TestFilePath_isSafeNameDashURLHashPNG / TestShortHash_pinnedVector
(internal/gemicon/gemicon_test.go). Change the scheme and both must move.

MAP may be a single flat name -> URL file OR a DIRECTORY of category files
(POE-135): a directory is merged from its `*.json` in sorted order, exactly as
the server's loader merges the embedded ones, and a key present in two of them
aborts naming both files. Merging last-writer-wins instead would silently pull
one category's artwork for the other's name.

The server keeps ONE cache root (ICON_CACHE_DIR, /data/icons-cache in
production) with ONE SUB-DIRECTORY PER ICON SET, because every set shares this
filename scheme and a flat directory would let two keys reduce to the same file
whenever they also share a source URL. So point OUT at the sub-directory for the
map you are pulling, never at the root:

    icons-cache/gems               internal/gemicon/urls  (a directory)
    icons-cache/currency-exchange  internal/exchange/itemdata/icon-urls.json

Usage:
    python3 scripts/download-gem-icons.py pull [MAP] [OUT]
    python3 scripts/download-gem-icons.py migrate [OUT] [--map MAP]
    python3 scripts/download-gem-icons.py prune [OUT] [--map MAP] [--dry-run] [--force]

    pull     Fetch every mapped icon that is not already on disk under its
             content-addressed name. The only mode that touches the network.
    migrate  Rename old name-only files (`<safe>.png`, the pre-POE-136 scheme)
             to their content-addressed names, offline. This is what turns an
             existing production cache over without a full re-crawl.
    prune    Delete every `*.png` in OUT that the current map does not produce —
             the superseded files a URL change leaves behind. `--dry-run` lists
             them and deletes nothing. Refuses outright (both with and without
             `--dry-run`) when nothing on disk, or under half of it, is produced
             by the map: that is a wrong --map/OUT pairing, not a sweep.
             `--force` overrides.

`migrate` and `prune` default `--map` to the gem set; pass the item map when you
are working on the `currency-exchange/` sub-directory.

Then ship OUT into the matching sub-directory of the prod icon-cache volume
(see docs/GEM-ICONS.md) and the server serves every icon from disk with no
upstream fetch.
"""
import argparse, glob, hashlib, json, os, re, sys, time, urllib.request

DEFAULT_MAP = "internal/gemicon/urls"
DEFAULT_OUT = "icons-cache/gems"
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
_unsafe = re.compile(r"[^A-Za-z0-9]+")


def safe_name(name: str) -> str:
    """The NAME half of the cache filename — gemicon.safeFileName."""
    return _unsafe.sub("_", name).strip("_")


def short_hash(url: str) -> str:
    """The URL half of the cache filename — gemicon.shortHash.

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

    The vector is pinned in Go too (internal/gemicon/gemicon_test.go). Checking
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
            f"= {got!r}, want {want!r} — the server (internal/gemicon) pins this vector"
        )


_self_check()


def load_map(path: str) -> dict:
    """Read one flat map file, or merge every *.json in a directory.

    Mirrors internal/gemicon's loader: sorted file order, and a duplicate key
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


def pull(map_path: str, out: str) -> int:
    """Fetch every mapped icon missing from out. The only mode that fetches."""
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
    print(f"\nDONE: {ok}/{total} present ({skip} already had), {len(fail)} failed -> {out}")
    for n, e in fail[:30]:
        print(f"  FAIL  {n}: {e}")
    return 0


def migrate(map_path: str, out: str) -> int:
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
    print(f"DONE: renamed {renamed}, already_addressed {already_addressed}, "
          f"no_old_file {no_old_file} of {len(m)} map entries -> {out}")
    return 0


class PruneRefused(Exception):
    """prune's blast-radius guard tripped — see prune's docstring."""


def prune(map_path: str, out: str, dry_run: bool, force: bool = False) -> int:
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

    The guard binds `--dry-run` too. A refused dry run prints the refusal
    instead of the list: the list is what an operator reads to decide, and
    handing them 700 lines of "WOULD DELETE" from a wrong pairing is exactly
    the confirmation that gets it run for real.
    """
    m = load_map(map_path)
    wanted = {cache_file_name(name, url) for name, url in m.items()}
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
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        description="Seed, migrate and prune an icon cache directory.",
    )
    modes = parser.add_subparsers(dest="mode", required=True)

    p = modes.add_parser("pull", help="fetch every mapped icon missing from OUT")
    p.add_argument("map", nargs="?", default=DEFAULT_MAP, metavar="MAP",
                   help=f"map file or directory of category files (default: {DEFAULT_MAP})")
    p.add_argument("out", nargs="?", default=DEFAULT_OUT, metavar="OUT",
                   help=f"cache sub-directory to write into (default: {DEFAULT_OUT})")

    g = modes.add_parser("migrate", help="rename old name-only files to content-addressed names")
    g.add_argument("out", nargs="?", default=DEFAULT_OUT, metavar="OUT",
                   help=f"cache sub-directory to rename in place (default: {DEFAULT_OUT})")
    g.add_argument("--map", default=DEFAULT_MAP, metavar="MAP",
                   help=f"map file or directory (default: {DEFAULT_MAP})")

    r = modes.add_parser("prune", help="delete files the current map does not produce")
    r.add_argument("out", nargs="?", default=DEFAULT_OUT, metavar="OUT",
                   help=f"cache sub-directory to sweep (default: {DEFAULT_OUT})")
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
    try:
        return prune(args.map, args.out, args.dry_run, args.force)
    except PruneRefused as e:
        # Non-zero and on stderr so a refusal stops a chained migration script
        # rather than reading as a clean sweep of nothing.
        print(e, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
