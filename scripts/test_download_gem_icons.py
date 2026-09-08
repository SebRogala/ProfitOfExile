import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("download-gem-icons.py")


def load_script():
    spec = importlib.util.spec_from_file_location("download_gem_icons", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class FakeResponse:
    def __init__(self, body):
        self.body = body

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def read(self):
        return self.body


class DownloadGemIconsTests(unittest.TestCase):
    def setUp(self):
        self.icons = load_script()
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.maps = self.root / "maps"
        self.maps.mkdir()
        self._write_map("gems", {"Gem One": "https://example.invalid/gem.png"})
        self._write_map("items", {"Item One": "https://example.invalid/item.png"})

    def tearDown(self):
        self.temp.cleanup()

    def _write_map(self, name, entries):
        (self.maps / f"{name}.json").write_text(json.dumps(entries))

    def _fake_urlopen(self, request, timeout):
        return FakeResponse(self.icons.PNG_MAGIC + request.full_url.encode())

    def test_pull_directory_fans_out_to_one_subdirectory_per_map(self):
        out = self.root / "icons-cache"
        with patch.object(self.icons.urllib.request, "urlopen", self._fake_urlopen):
            self.assertEqual(self.icons.main(["pull", str(self.maps), str(out)]), 0)

        for category, name, url in [
            ("gems", "Gem One", "https://example.invalid/gem.png"),
            ("items", "Item One", "https://example.invalid/item.png"),
        ]:
            expected = out / category / self.icons.cache_file_name(name, url)
            self.assertTrue(expected.is_file(), expected)
        self.assertEqual(list(out.glob("*.png")), [])

    def test_pull_single_file_keeps_writing_to_the_directory_given(self):
        out = self.root / "currency-exchange"
        with patch.object(self.icons.urllib.request, "urlopen", self._fake_urlopen):
            self.assertEqual(self.icons.main(["pull", str(self.maps / "items.json"), str(out)]), 0)

        expected = out / self.icons.cache_file_name("Item One", "https://example.invalid/item.png")
        self.assertTrue(expected.is_file(), expected)

    def test_prune_directory_applies_wrong_pair_guard_per_category(self):
        out = self.root / "icons-cache"
        for category, name, url in [
            ("gems", "Gem One", "https://example.invalid/gem.png"),
            ("items", "Item One", "https://example.invalid/item.png"),
        ]:
            category_dir = out / category
            category_dir.mkdir(parents=True)
            (category_dir / self.icons.cache_file_name(name, url)).write_bytes(b"wanted")
            (category_dir / "stranded.png").write_bytes(b"stale")

        # Each pair has a real wanted file, so both stale files are safe to sweep.
        self.assertEqual(self.icons.main(["prune", str(out), "--map", str(self.maps)]), 0)
        self.assertFalse((out / "gems" / "stranded.png").exists())
        self.assertFalse((out / "items" / "stranded.png").exists())

    def test_prune_map_directory_refuses_only_the_unmatched_pair(self):
        out = self.root / "icons-cache"
        gems = out / "gems"
        gems.mkdir(parents=True)
        gems_wanted = self.icons.cache_file_name("Gem One", "https://example.invalid/gem.png")
        (gems / gems_wanted).write_bytes(b"wanted")
        (gems / "stranded.png").write_bytes(b"stale")

        items = out / "items"
        items.mkdir(parents=True)
        (items / "not-from-items-map.png").write_bytes(b"wrong-pair")

        self.assertEqual(self.icons.main(["prune", str(out), "--map", str(self.maps)]), 1)
        self.assertFalse((gems / "stranded.png").exists())
        self.assertTrue((items / "not-from-items-map.png").exists())

    def test_prune_single_file_override_targets_the_directory_given(self):
        out = self.root / "currency-exchange"
        out.mkdir()
        wanted = self.icons.cache_file_name("Item One", "https://example.invalid/item.png")
        (out / wanted).write_bytes(b"wanted")
        stale = out / "stranded.png"
        stale.write_bytes(b"stale")

        self.assertEqual(
            self.icons.main(["prune", str(out), "--map", str(self.maps / "items.json")]),
            0,
        )
        self.assertFalse(stale.exists())


if __name__ == "__main__":
    unittest.main()
