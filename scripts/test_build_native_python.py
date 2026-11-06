import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("native_builder", Path(__file__).with_name("build-native-python.py"))
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class NativeBuildTests(unittest.TestCase):
    def test_rebuild_preserves_the_image_already_opened_by_an_interpreter(self):
        with tempfile.TemporaryDirectory() as directory:
            built, installed = Path(directory) / "built", Path(directory) / "installed"
            built.write_bytes(b"replacement library")
            installed.write_bytes(b"running library")
            with installed.open("rb") as running:
                builder.publish_extension(built, installed)
                self.assertEqual(running.read(), b"running library")
            self.assertEqual(installed.read_bytes(), b"replacement library")


if __name__ == "__main__":
    unittest.main()
