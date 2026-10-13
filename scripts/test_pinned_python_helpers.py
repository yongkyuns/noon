from pathlib import Path
import tempfile
import unittest

from pinned_python_helpers import load_helpers


class PinnedHelpers(unittest.TestCase):
    def test_selected_definitions_keep_bodies_defaults_and_deferred_annotations(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.py"
            source.write_text("import unavailable_engine\n"
                              "def selected(value: Unknown = 3):\n    return dependency(value)\n"
                              "def unrelated():\n    raise RuntimeError('not selected')\n")
            namespace = load_helpers(source, {"selected"}, {"dependency": lambda value: value + 1})
            self.assertEqual(namespace["selected"](), 4)
            self.assertEqual(namespace["selected"].__annotations__, {"value": "Unknown"})
            self.assertNotIn("unrelated", namespace)
            self.assertEqual(namespace["selected"].__code__.co_filename, str(source))
            with self.assertRaises(ValueError): load_helpers(source, {"missing"}, {})


if __name__ == "__main__": unittest.main()
