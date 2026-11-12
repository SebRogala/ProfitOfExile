#!/usr/bin/env python3
"""Save the first post of a pathofexile.com patch-notes thread as plain text.

The patch-notes thread's first post is the notes; every later post is a
player reply and is dropped. The post opens with a table of contents whose
links (`#endgamechanges`, ...) name the `<div id=...>` block holding each
top-level section. This writes one text file per section plus an index:

    OUT/00-index.txt
    OUT/01-the-covens-of-umbra-challenge-league.txt
    OUT/02-new-content-and-features.txt
    ...

Headers become `#` / `##` lines, a bold line outside a list (a league
sub-section such as "Incursion and Atziri") a `###` line, and list items `- `
bullets, indented two spaces per nesting level. The inline `<style>` block, images and the
"Return to top" links are dropped.

The forum markup is not an API, so every structural assumption is checked:
no first post, no table of contents, or a contents entry without its section
block aborts with a non-zero exit instead of writing a partial set.

Usage:
    python3 scripts/fetch-patch-notes.py URL OUT
"""
import argparse, html.parser, os, re, sys, urllib.request

UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"
THREAD_URL = re.compile(r"^https://(www\.)?pathofexile\.com/forum/view-thread/\d+")
# The page wraps everything in a layout <div class="content">; the first post's
# body is the first one sitting directly inside a table cell.
FIRST_POST = re.compile(r'<td[^>]*>\s*<div class="content">')
INDEX_NAME = "00-index.txt"


class ExtractError(Exception):
    """The page does not have the structure this extractor relies on."""


def first_post_html(page: str) -> str:
    """Return the inner HTML of the thread's first post."""
    m = FIRST_POST.search(page)
    if not m:
        raise ExtractError('first post not found: no <td> holding <div class="content">')
    depth, pos = 1, m.end()
    tag = re.compile(r"<(/?)div\b[^>]*>", re.I)
    while depth:
        t = tag.search(page, pos)
        if not t:
            raise ExtractError("first post is not closed: unbalanced <div> nesting")
        depth += -1 if t.group(1) else 1
        pos = t.end()
    return page[m.end():t.start()]


class _PostParser(html.parser.HTMLParser):
    """Split the post into id-keyed section blocks and render each as text.

    The table of contents is read from the first block only, so an in-page
    link inside a later section is never mistaken for a contents entry.
    """

    BLOCK = {"h2": "# ", "h3": "## "}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.sections = []  # [(id, [lines])]
        self.toc = []  # [(anchor, title)]
        self._lines = None
        self._prefix = ""
        self._line = ""
        self._list_depth = 0
        self._skip = 0  # inside <style> or a "#top" link
        self._toc_href = None
        self._toc_text = ""

    def _flush(self):
        text = re.sub(r"\s+", " ", self._line).strip()
        if text and self._lines is not None:
            self._lines.append(self._prefix + text)
        self._prefix = self._line = ""

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "div" and "lbox-container" in (a.get("class") or "").split():
            self._flush()
            self._lines = []
            self.sections.append((a.get("id") or "", self._lines))
        elif tag == "style":
            self._skip += 1
        elif tag == "a" and a.get("href") == "#top":
            self._skip += 1
        elif tag == "a" and (a.get("href") or "").startswith("#") and len(self.sections) == 1:
            self._toc_href, self._toc_text = a["href"][1:], ""
        elif tag in self.BLOCK:
            self._flush()
            self._prefix = self.BLOCK[tag]
        elif tag == "ul":
            self._flush()
            self._list_depth += 1
        elif tag == "li":
            self._flush()
            self._prefix = "  " * (self._list_depth - 1) + "- "
        elif tag == "br":
            self._flush()
        elif tag == "strong" and not self._list_depth and not self._line.strip():
            # League sub-sections ("Incursion and Atziri") are a bold line between <br/>s.
            self._flush()
            self._prefix = "### "

    def handle_endtag(self, tag):
        if tag == "style" or (tag == "a" and self._skip):
            self._skip = max(0, self._skip - 1)
        elif tag == "a" and self._toc_href is not None:
            self.toc.append((self._toc_href, self._toc_text.strip()))
            self._toc_href = None
        elif tag == "ul":
            self._flush()
            self._list_depth = max(0, self._list_depth - 1)
        elif tag in ("li", "div", "h2", "h3") or (tag == "strong" and self._prefix == "### "):
            self._flush()

    def handle_data(self, data):
        if self._skip:
            return
        if self._toc_href is not None:
            self._toc_text += data
        self._line += data


def slug(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")


def extract_sections(page: str):
    """Return (post title, [(title, text)]) for the first post's top-level sections."""
    parser = _PostParser()
    parser.feed(first_post_html(page))
    parser.close()

    blocks = {sid: lines for sid, lines in parser.sections}
    toc = [(anchor, title) for anchor, title in parser.toc if anchor]
    if not toc:
        raise ExtractError("table of contents not found in the first post")
    header = blocks.get("top") or (parser.sections[0][1] if parser.sections else [])
    post_title = next((l[2:] for l in header if l.startswith("# ")), "")

    sections = []
    for anchor, title in toc:
        if anchor not in blocks:
            raise ExtractError(f"contents entry {title!r} (#{anchor}) has no section block")
        lines = blocks[anchor]
        if not any(l.strip("#- ") for l in lines[1:]):
            raise ExtractError(f"section {title!r} (#{anchor}) is empty")
        sections.append((title, "\n".join(lines) + "\n"))
    return post_title, sections


def write_sections(url: str, post_title: str, sections, out: str) -> list:
    """Write one file per section plus the index; return the written paths."""
    os.makedirs(out, exist_ok=True)
    width = max(2, len(str(len(sections))))
    index = [f"# {post_title}" if post_title else "# Patch notes", f"Source: {url}", ""]
    written = []
    for i, (title, text) in enumerate(sections, 1):
        name = f"{i:0{width}d}-{slug(title)}.txt"
        path = os.path.join(out, name)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)
        written.append(path)
        index.append(f"- {name}  {title} ({text.count(chr(10))} lines)")
    index_path = os.path.join(out, INDEX_NAME)
    with open(index_path, "w", encoding="utf-8") as f:
        f.write("\n".join(index) + "\n")
    return [index_path] + written


def fetch(url: str) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as r:
        charset = r.headers.get_content_charset() or "utf-8"
        return r.read().decode(charset)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        description="Save a pathofexile.com patch-notes thread's first post, one file per section.",
    )
    parser.add_argument("url", help="forum thread URL, https://www.pathofexile.com/forum/view-thread/<id>")
    parser.add_argument("out", help="output directory")
    args = parser.parse_args(argv)

    if not THREAD_URL.match(args.url):
        print(f"not a pathofexile.com forum thread URL: {args.url}", file=sys.stderr)
        return 2
    try:
        post_title, sections = extract_sections(fetch(args.url))
    except ExtractError as e:
        print(f"{args.url}: {e} — the forum markup may have changed", file=sys.stderr)
        return 1
    paths = write_sections(args.url, post_title, sections, args.out)
    print(f"DONE: {len(sections)} sections -> {args.out}")
    for path in paths:
        print(f"  {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
