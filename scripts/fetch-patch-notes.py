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

A "New and Changed Gems" thread has no table of contents; its first post holds
a `<div id="gem-forum-post">`, which selects the gem layout instead:

    OUT/post.txt   the whole post as text
    OUT/gems.tsv   name, status (new/updated), icon_url, diff

Gem names and icons come from the item JSON the page passes to its item
renderer (`factory([{"item": {...}, "destination": "#item-fragment-N"}])`).
A fragment inside a `<div class="gem">` with its `<h4 class="gem-header">` is
an updated gem and its diff is the text of that block; a fragment outside one
(the "New Gems" list) is a new gem with an empty diff. A fragment without an
item, an item without a fragment, or a header that differs from its item's
name aborts as above.

Usage:
    python3 scripts/fetch-patch-notes.py URL OUT
"""
import argparse, csv, html.parser, json, os, re, sys, urllib.request

UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"
THREAD_URL = re.compile(r"^https://(www\.)?pathofexile\.com/forum/view-thread/\d+")
# The page wraps everything in a layout <div class="content">; the first post's
# body is the first one sitting directly inside a table cell.
FIRST_POST = re.compile(r'<td[^>]*>\s*<div class="content">')
INDEX_NAME = "00-index.txt"
GEM_POST = 'id="gem-forum-post"'
ITEM_RENDERER = "factory("


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

    BLOCK = {"h2": "# ", "h3": "## ", "h4": "### ", "h6": "#### "}
    SKIP = {"style", "script", "button"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.sections = []  # [(id, [lines])]
        self.toc = []  # [(anchor, title)]
        self._lines = None
        self._prefix = ""
        self._line = ""
        self._list_depth = 0
        self._skip = []  # open tags whose content is dropped
        self._toc_href = None
        self._toc_text = ""
        self._div_depth = 0
        self._gem = None
        self._header_open = False
        self.gems = []  # [{"fragment", "header", "lines"}]
        self.new_fragments = []

    def _flush(self):
        text = re.sub(r"\s+", " ", self._line).strip()
        if text and self._lines is not None:
            self._lines.append(self._prefix + text)
        self._prefix = self._line = ""

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        classes = (a.get("class") or "").split()
        if self._skip:
            if tag == self._skip[-1]:
                self._skip.append(tag)
            return
        if tag == "div":
            self._div_depth += 1
            self._start_div(a.get("id") or "", classes)
        if tag in self.SKIP or (tag == "a" and (
                a.get("href") == "#top" or "show-differences" in classes)):
            self._skip.append(tag)
        elif tag == "div" and "lbox-container" in classes:
            self._flush()
            self._lines = []
            self.sections.append((a.get("id") or "", self._lines))
        elif tag == "a" and (a.get("href") or "").startswith("#") and len(self.sections) == 1:
            self._toc_href, self._toc_text = a["href"][1:], ""
        elif tag in self.BLOCK:
            self._flush()
            self._prefix = self.BLOCK[tag]
            self._header_open = tag == "h4" and "gem-header" in classes and self._gem is not None
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

    def _start_div(self, div_id, classes):
        if "gem" in classes:
            self._flush()
            self._gem = {"depth": self._div_depth, "fragment": None, "header": None,
                         "start": len(self._lines) if self._lines is not None else 0}
        elif div_id.startswith("item-fragment-"):
            if self._gem is not None:
                self._gem["fragment"] = div_id
            else:
                self.new_fragments.append(div_id)

    def _end_div(self):
        gem = self._gem
        if gem is not None and gem["depth"] == self._div_depth:
            lines = self._lines[gem["start"]:] if self._lines is not None else []
            if gem["header"] is not None and lines[:1] == ["### " + gem["header"]]:
                lines = lines[1:]
            self.gems.append({"fragment": gem["fragment"], "header": gem["header"], "lines": lines})
            self._gem = None
        self._div_depth -= 1

    def handle_endtag(self, tag):
        if self._skip:
            if tag == self._skip[-1]:
                self._skip.pop()
            return
        if tag == "h4" and self._header_open:
            self._gem["header"] = re.sub(r"\s+", " ", self._line).strip()
            self._header_open = False
        if tag == "div":
            self._flush()
            self._end_div()
        elif tag == "a" and self._toc_href is not None:
            self.toc.append((self._toc_href, self._toc_text.strip()))
            self._toc_href = None
        elif tag == "ul":
            self._flush()
            self._list_depth = max(0, self._list_depth - 1)
        elif tag in ("li", "p") or tag in self.BLOCK or (tag == "strong" and self._prefix == "### "):
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


def post_text(post_html: str) -> str:
    """Render the whole post, one blank line between its blocks."""
    parser = _PostParser()
    parser.feed(post_html)
    parser.close()
    return "\n\n".join("\n".join(lines) for _, lines in parser.sections if lines) + "\n"


def page_items(page: str) -> dict:
    """Map each `#item-fragment-N` destination to its item JSON."""
    start = page.find(ITEM_RENDERER)
    if start < 0:
        raise ExtractError(f"item JSON not found: no {ITEM_RENDERER!r} call on the page")
    try:
        entries, _ = json.JSONDecoder().raw_decode(page, start + len(ITEM_RENDERER))
        return {e["destination"].lstrip("#"): e["item"] for e in entries}
    except (ValueError, KeyError, TypeError, AttributeError) as e:
        raise ExtractError(f"item JSON is not a list of {{item, destination}}: {e}") from e


def icon_url(icon: str) -> str:
    if icon.startswith("//"):
        return "https:" + icon
    if icon.startswith("https://"):
        return icon
    raise ExtractError(f"unexpected icon URL {icon!r}")


def extract_gems(page: str):
    """Return (post text, [(name, status, icon_url, diff)]) for a gems thread."""
    post_html = first_post_html(page)
    parser = _PostParser()
    parser.feed(post_html)
    parser.close()
    items = page_items(page)

    gems, seen = [], set()

    def item_for(fragment, what):
        if fragment not in items:
            raise ExtractError(f"{what} has no item JSON for #{fragment}")
        seen.add(fragment)
        item = items[fragment]
        return item["typeLine"], icon_url(item["icon"])

    for fragment in parser.new_fragments:
        name, icon = item_for(fragment, "new gem")
        gems.append((name, "new", icon, ""))
    for gem in parser.gems:
        if gem["header"] is None or gem["fragment"] is None:
            raise ExtractError(f"updated gem block lacks its header or item fragment: {gem}")
        name, icon = item_for(gem["fragment"], f"updated gem {gem['header']!r}")
        if name != gem["header"]:
            raise ExtractError(f"gem header {gem['header']!r} does not match its item {name!r}")
        if not gem["lines"]:
            raise ExtractError(f"updated gem {name!r} has no diff text")
        diff = " | ".join(line.lstrip("# ") for line in gem["lines"])
        gems.append((name, "updated", icon, diff))
    if not gems:
        raise ExtractError("gem post has no gems")
    unplaced = sorted(set(items) - seen)
    if unplaced:
        raise ExtractError(f"item JSON entries with no gem in the post: {unplaced}")
    return post_text(post_html), gems


def write_gems(text: str, gems, out: str) -> list:
    """Write post.txt and gems.tsv; return the written paths."""
    os.makedirs(out, exist_ok=True)
    text_path, tsv_path = os.path.join(out, "post.txt"), os.path.join(out, "gems.tsv")
    with open(text_path, "w", encoding="utf-8") as f:
        f.write(text)
    with open(tsv_path, "w", encoding="utf-8", newline="") as f:
        writer = csv.writer(f, delimiter="\t", lineterminator="\n")
        writer.writerow(["name", "status", "icon_url", "diff"])
        writer.writerows(gems)
    return [text_path, tsv_path]


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
        description="Save a pathofexile.com patch-notes or gems thread's first post as text.",
    )
    parser.add_argument("url", help="forum thread URL, https://www.pathofexile.com/forum/view-thread/<id>")
    parser.add_argument("out", help="output directory")
    args = parser.parse_args(argv)

    if not THREAD_URL.match(args.url):
        print(f"not a pathofexile.com forum thread URL: {args.url}", file=sys.stderr)
        return 2
    gems = None
    try:
        page = fetch(args.url)
        if GEM_POST in first_post_html(page):
            text, gems = extract_gems(page)
        else:
            post_title, sections = extract_sections(page)
    except ExtractError as e:
        print(f"{args.url}: {e} — the forum markup may have changed", file=sys.stderr)
        return 1
    if gems is not None:
        paths = write_gems(text, gems, args.out)
        new = sum(1 for g in gems if g[1] == "new")
        print(f"DONE: {len(gems)} gems ({new} new) -> {args.out}")
    else:
        paths = write_sections(args.url, post_title, sections, args.out)
        print(f"DONE: {len(sections)} sections -> {args.out}")
    for path in paths:
        print(f"  {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
