import tempfile
import unittest
from pathlib import Path

import noon


class FilePathHelpers(unittest.TestCase):
    def test_extensions_preserve_existing_suffix_and_matching_identity(self):
        path = Path("movie.mp4")
        self.assertIs(noon.add_extension_if_not_present(path, ".mp4"), path)
        self.assertEqual(noon.add_extension_if_not_present(path, ".png"), Path("movie.mp4.png"))
        self.assertEqual(noon.add_extension_if_not_present(Path(".hidden"), ".png"), Path(".hidden.png"))
        with self.assertRaises(ValueError):
            noon.add_extension_if_not_present(path, "/invalid")

    def test_path_protocol_preserves_ne_dispatch_and_return_identity(self):
        calls = []
        marker = object()
        class Suffix:
            def __ne__(self, other): calls.append("ne"); return True
            def __eq__(self, other): calls.append("eq"); return True
            def __add__(self, other): calls.append("add"); return ".combined"
        class UserPath:
            suffix = Suffix()
            def with_suffix(self, suffix): calls.append(suffix); return marker
        self.assertIs(noon.add_extension_if_not_present(UserPath(), ".png"), marker)
        self.assertEqual(calls, ["ne", "add", ".combined"])

    def test_directory_creation_and_cleanup_do_not_touch_siblings(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "nested" / "target"
            sibling = root / "retained.txt"
            sibling.write_text("keep")
            self.assertEqual(noon.guarantee_existence(target), target.resolve(strict=True))
            (target / "old.txt").write_text("remove")
            self.assertEqual(noon.guarantee_existence(target), target.resolve(strict=True))
            self.assertTrue((target / "old.txt").exists())
            self.assertEqual(noon.guarantee_empty_existence(target), target.resolve(strict=True))
            self.assertEqual(list(target.iterdir()), [])
            self.assertEqual(sibling.read_text(), "keep")
            with self.assertRaises(NotADirectoryError):
                noon.guarantee_empty_existence(sibling)
            self.assertEqual(sibling.read_text(), "keep")

    def test_search_precedence_order_and_missing_message(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            default = root / "default"
            default.mkdir()
            explicit = root / "picture"
            explicit.write_text("direct")
            (default / "picture.png").write_text("png")
            (default / "picture.svg").write_text("svg")
            search = noon.seek_full_path_from_defaults
            self.assertEqual(search(explicit, default, [".svg", ".png"]), explicit)
            # Upstream builds candidates before checking the explicit path.
            with self.assertRaises(TypeError):
                search(explicit, default, None)
            self.assertEqual(search("picture", default, [".svg", ".png"]), default / "picture.svg")
            (default / "picture").mkdir()
            self.assertEqual(search("picture", default, [".svg", ".png"]), default / "picture")
            with self.assertRaisesRegex(OSError, "could not find absent"):
                search("absent", default, [".png"])


if __name__ == "__main__":
    unittest.main()
