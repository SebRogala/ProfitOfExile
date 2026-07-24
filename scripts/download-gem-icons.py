#!/usr/bin/env python3
"""Pre-populate the gem-icon cache.

poewiki 403s datacenter IPs, so the server (internal/gemicon) cannot fetch icons
at runtime in production — its disk cache must be seeded from an allowed IP.

This pulls every icon in internal/gemicon/gem-icon-urls.json and writes it using
the SAME filename scheme as the server's cache (safeFileName: runs of
[^A-Za-z0-9] -> "_", trimmed, + ".png"). The output directory can therefore be
dropped straight into the server's GEM_ICON_CACHE_DIR volume as cache hits.

Usage:
    python3 scripts/download-gem-icons.py [MAP_JSON] [OUT_DIR]

Then ship OUT_DIR into the prod icon-cache volume (see docs / ops notes) and the
server serves every icon from disk with no upstream fetch.
"""
import json, os, re, sys, time, urllib.request

MAP = sys.argv[1] if len(sys.argv) > 1 else "internal/gemicon/gem-icon-urls.json"
OUT = sys.argv[2] if len(sys.argv) > 2 else "gem-icons-cache"
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
_unsafe = re.compile(r"[^A-Za-z0-9]+")


def safe_name(name: str) -> str:
    return _unsafe.sub("_", name).strip("_")


def main() -> int:
    os.makedirs(OUT, exist_ok=True)
    with open(MAP) as f:
        m = json.load(f)
    total, ok, skip, fail = len(m), 0, 0, []
    for i, (name, url) in enumerate(sorted(m.items()), 1):
        fn = os.path.join(OUT, safe_name(name) + ".png")
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
            with open(fn, "wb") as out:
                out.write(data)
            ok += 1
        except Exception as e:  # noqa: BLE001 - report and continue
            fail.append((name, str(e)))
        if i % 100 == 0:
            print(f"  {i}/{total} (ok={ok} fail={len(fail)})", flush=True)
        time.sleep(0.12)  # be polite to the wiki
    print(f"\nDONE: {ok}/{total} present ({skip} already had), {len(fail)} failed -> {OUT}")
    for n, e in fail[:30]:
        print(f"  FAIL  {n}: {e}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
