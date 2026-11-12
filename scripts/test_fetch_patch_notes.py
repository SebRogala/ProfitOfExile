import csv
import email.message
import importlib.util
import io
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("fetch-patch-notes.py")
FIXTURE = Path(__file__).with_name("testdata") / "patch-notes-thread.html"
GEMS_FIXTURE = Path(__file__).with_name("testdata") / "gems-thread.html"
URL = "https://www.pathofexile.com/forum/view-thread/4013680"
GEMS_URL = "https://www.pathofexile.com/forum/view-thread/4013898"


def load_script():
    spec = importlib.util.spec_from_file_location("fetch_patch_notes", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class FakeResponse:
    def __init__(self, body):
        self.body = body.encode("utf-8")
        self.headers = email.message.Message()
        self.headers["Content-Type"] = "text/html; charset=UTF-8"

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def read(self):
        return self.body


class ThreadFixtureCase(unittest.TestCase):
    fixture = FIXTURE

    def setUp(self):
        self.notes = load_script()
        self.temp = tempfile.TemporaryDirectory()
        self.out = Path(self.temp.name) / "pn"
        self.page = self.fixture.read_text(encoding="utf-8")

    def tearDown(self):
        self.temp.cleanup()

    def run_main(self, page, url=URL):
        requested = []

        def fake_urlopen(request, timeout):
            requested.append(request.full_url)
            return FakeResponse(page)

        stdout, stderr = io.StringIO(), io.StringIO()
        with patch.object(self.notes.urllib.request, "urlopen", fake_urlopen), \
                redirect_stdout(stdout), redirect_stderr(stderr):
            code = self.notes.main([url, str(self.out)])
        return code, stderr.getvalue(), requested

    def section(self, name):
        return (self.out / name).read_text(encoding="utf-8")


class FetchPatchNotesTests(ThreadFixtureCase):

    def test_writes_one_file_per_contents_entry_in_contents_order(self):
        code, _, _ = self.run_main(self.page)

        self.assertEqual(code, 0)
        self.assertEqual(
            sorted(p.name for p in self.out.iterdir()),
            [
                "00-index.txt",
                "01-the-covens-of-umbra-challenge-league.txt",
                "02-endgame-changes.txt",
                "03-league-changes.txt",
                "04-bug-fixes.txt",
            ],
        )

    def test_index_names_the_post_source_and_each_section_file(self):
        self.run_main(self.page)

        self.assertEqual(
            self.section("00-index.txt"),
            "# Content Update 3.30.0 — Path of Exile: Covens of Umbra\n"
            f"Source: {URL}\n"
            "\n"
            "- 01-the-covens-of-umbra-challenge-league.txt  The Covens of Umbra Challenge League (2 lines)\n"
            "- 02-endgame-changes.txt  Endgame Changes (6 lines)\n"
            "- 03-league-changes.txt  League Changes (3 lines)\n"
            "- 04-bug-fixes.txt  Bug Fixes (3 lines)\n",
        )

    def test_nested_list_items_are_indented_under_their_parent_bullet(self):
        self.run_main(self.page)

        self.assertEqual(
            self.section("02-endgame-changes.txt"),
            "## Endgame Changes\n"
            "- Scarabs have been removed from the game. A new item type, Threads, have been added in their place.\n"
            "- Below are the new Thread items available:\n"
            "  - Abyss Thread\n"
            "  - Incursion Thread\n"
            "- Added a new Currency Item, Frayed Thread which can be used to grant a random Thread.\n",
        )

    def test_bold_line_outside_a_list_becomes_a_sub_header_and_inline_bold_stays_inline(self):
        self.run_main(self.page)

        self.assertEqual(
            self.section("03-league-changes.txt"),
            "## League Changes\n"
            "### Incursion and Atziri\n"
            "- The Temple of Atzoatl now has fewer rooms & more architects.\n",
        )

    def test_bold_words_inside_a_paragraph_stay_in_that_paragraph(self):
        page = self.page.replace("Thanks for your", "Thanks for your <strong>continued</strong>", 1)

        self.run_main(page)

        self.assertTrue(
            self.section("04-bug-fixes.txt").endswith(
                "\nWe really hope you enjoy Path of Exile: Covens of Umbra. Thanks for your continued support!\n"
            )
        )

    def test_sign_off_is_kept_and_return_to_top_links_are_dropped(self):
        self.run_main(self.page)

        self.assertEqual(
            self.section("04-bug-fixes.txt"),
            "## Bug Fixes\n"
            "- Fixed an issue where the Lighting Rods in the Temple of Atzoatl were very loud.\n"
            "We really hope you enjoy Path of Exile: Covens of Umbra. Thanks for your support!\n",
        )

    def test_replies_and_inline_css_are_not_written(self):
        self.run_main(self.page)

        written = "".join(p.read_text(encoding="utf-8") for p in self.out.iterdir())
        self.assertNotIn("A REPLY THAT MUST NOT APPEAR", written)
        self.assertNotIn("margin", written)

    def test_page_without_a_first_post_fails_without_writing(self):
        page = self.page.replace('<td colspan="2"><div class="content">', '<td colspan="2"><div class="body">', 1)

        code, error, _ = self.run_main(page)

        self.assertEqual(code, 1)
        self.assertIn("first post not found", error)
        self.assertFalse(self.out.exists())

    def test_contents_entry_without_its_section_block_fails_without_writing(self):
        page = self.page.replace('<div id="leaguechanges"', '<div id="renamed"', 1)

        code, error, _ = self.run_main(page)

        self.assertEqual(code, 1)
        self.assertIn("'League Changes' (#leaguechanges) has no section block", error)
        self.assertFalse(self.out.exists())

    def test_post_without_a_table_of_contents_fails(self):
        start = self.page.index("<h3>Table of Contents</h3>")
        end = self.page.index("</li></ul><br/>", start) + len("</li></ul>")
        page = self.page[:start] + self.page[end:]

        code, error, _ = self.run_main(page)

        self.assertEqual(code, 1)
        self.assertIn("table of contents not found", error)

    def test_section_with_only_a_title_fails(self):
        start = self.page.index("<ul><li>Fixed an issue")
        end = self.page.index("</ul>", start) + len("</ul>")
        sign_off_end = self.page.index("</a>!", end) + len("</a>!")
        page = self.page[:start] + self.page[sign_off_end:]

        code, error, _ = self.run_main(page)

        self.assertEqual(code, 1)
        self.assertIn("section 'Bug Fixes' (#bugfixes) is empty", error)

    def test_non_forum_url_is_refused_before_any_fetch(self):
        code, _, requested = self.run_main(self.page, url="https://example.invalid/forum/view-thread/1")

        self.assertEqual(code, 2)
        self.assertEqual(requested, [])



class FetchGemsThreadTests(ThreadFixtureCase):
    fixture = GEMS_FIXTURE

    def gem_rows(self):
        with open(self.out / "gems.tsv", encoding="utf-8", newline="") as f:
            return list(csv.DictReader(f, delimiter="\t"))

    def test_gem_post_writes_post_text_and_gem_table_only(self):
        code, _, _ = self.run_main(self.page, url=GEMS_URL)

        self.assertEqual(code, 0)
        self.assertEqual(sorted(p.name for p in self.out.iterdir()), ["gems.tsv", "post.txt"])

    def test_gem_outside_an_updated_block_is_listed_as_new_with_no_diff(self):
        self.run_main(self.page, url=GEMS_URL)

        self.assertEqual(
            self.gem_rows()[0],
            {
                "name": "Arcane Link",
                "status": "new",
                "icon_url": "https://web.poecdn.com/public/news/2026-11-10/gems/ArcaneLink.png",
                "diff": "",
            },
        )

    def test_updated_gem_diff_joins_its_headings_bullets_and_paragraphs(self):
        self.run_main(self.page, url=GEMS_URL)

        self.assertEqual(
            [(r["name"], r["status"], r["diff"]) for r in self.gem_rows()[1:]],
            [
                ("Flame Link", "updated",
                 "Modified"
                 " | - New: Linked target gains 219 to 329 Added Fire Damage"
                 " | - Old: Linked target gains 169 to 254 Added Fire Damage"
                 " | - New: Linked target gains Added Fire Damage equal to 3% of your Maximum Life"
                 " | - Old: Linked target gains Added Fire Damage equal to 5% of your Maximum Life"),
                ("Transfusion Support", "updated",
                 "Added"
                 " | - A Minion within 6 metres gains a Power, Frenzy or Endurance Charge for every"
                 " 400 Life you spend on Upfront Costs with Supported Spells"
                 " | Removed"
                 " | - A nearby Minion gains a Soul for every 200 Life you spend on Upfront Costs"
                 " with Supported Spells"
                 " | New: Supports spells. Does not support instant or blessing spells."
                 " | Old: Supports non-blessing spells."),
            ],
        )

    def test_post_text_drops_toggle_labels_styles_scripts_and_replies(self):
        self.run_main(self.page, url=GEMS_URL)

        text = self.section("post.txt")
        self.assertIn("### Flame Link\n#### Modified\n- New: Linked target gains 219 to 329", text)
        for dropped in ("Differences", "display:flex", "addEventListener", "A REPLY THAT MUST NOT APPEAR"):
            self.assertNotIn(dropped, text)

    def test_gem_header_that_differs_from_its_item_name_fails_without_writing(self):
        page = self.page.replace('<h4 class="gem-header">Flame Link</h4>', '<h4 class="gem-header">Flame Lnk</h4>', 1)

        code, error, _ = self.run_main(page, url=GEMS_URL)

        self.assertEqual(code, 1)
        self.assertIn("gem header 'Flame Lnk' does not match its item 'Flame Link'", error)
        self.assertFalse(self.out.exists())

    def test_gem_fragment_without_item_json_fails(self):
        page = self.page.replace('"destination":"#item-fragment-9"', '"destination":"#item-fragment-99"', 1)

        code, error, _ = self.run_main(page, url=GEMS_URL)

        self.assertEqual(code, 1)
        self.assertIn("updated gem 'Transfusion Support' has no item JSON for #item-fragment-9", error)

    def test_item_json_entry_without_a_gem_in_the_post_fails(self):
        page = self.page.replace('<div class="itemContentLayout iW1 iH1" id="item-fragment-0"></div>', "", 1)

        code, error, _ = self.run_main(page, url=GEMS_URL)

        self.assertEqual(code, 1)
        self.assertIn("item JSON entries with no gem in the post: ['item-fragment-0']", error)

    def test_page_without_item_json_fails(self):
        page = self.page.replace("factory([", "render([", 1)

        code, error, _ = self.run_main(page, url=GEMS_URL)

        self.assertEqual(code, 1)
        self.assertIn("item JSON not found", error)


if __name__ == "__main__":
    unittest.main()
