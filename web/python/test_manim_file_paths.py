import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import noon
import _manim_file_paths as helpers


class FilePathHelpers(unittest.TestCase):
    def test_executable_search_uses_stem_and_short_circuits_missing_commands(self):
        with patch.object(helpers.shutil, "which", return_value=None) as which, \
             patch.object(helpers.os, "access") as access:
            self.assertFalse(noon.ensure_executable(Path("tool.exe")))
            which.assert_called_once_with("tool")
            access.assert_not_called()
        with patch.object(helpers.shutil, "which", return_value="/resolved/tool"), \
             patch.object(helpers.os, "access", return_value=True) as access:
            self.assertTrue(noon.ensure_executable(Path("tool.exe")))
            access.assert_called_once_with("/resolved/tool", helpers.os.X_OK)

    def test_explicit_executable_path_bypasses_search_without_launching(self):
        path = Path("folder/tool.exe")
        with patch.object(helpers.shutil, "which") as which, \
             patch.object(helpers.os, "access", return_value=False) as access:
            self.assertFalse(noon.ensure_executable(path))
            which.assert_not_called()
            access.assert_called_once_with(path, helpers.os.X_OK)

    def test_access_timestamp_changes_preserve_modification_time_and_file_content(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "retained.txt"
            path.write_text("keep")
            helpers.os.utime(path, times=(1000000000.0, 1100000000.0))
            with patch.object(helpers.time, "time", return_value=1234567890.0):
                self.assertIsNone(noon.modify_atime(str(path)))
            observed = path.stat()
            self.assertEqual(observed.st_atime, 1234567890.0)
            self.assertEqual(observed.st_mtime, 1100000000.0)
            self.assertEqual(path.read_text(), "keep")
            with patch.object(helpers.os, "utime") as utime:
                with self.assertRaises(FileNotFoundError):
                    noon.modify_atime(str(path.parent / "missing"))
                utime.assert_not_called()

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
