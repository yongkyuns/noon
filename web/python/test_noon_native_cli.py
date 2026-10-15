"""CLI forwarding and cleanup only; real rendering has separate media proofs."""
import importlib.util
import io
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import AsyncMock, Mock, patch
from contextlib import redirect_stdout


class NativeCli(unittest.TestCase):
    def load(self, host):
        path = Path(__file__).with_name("noon_native.py")
        spec = importlib.util.spec_from_file_location("native_cli_under_test", path)
        module = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {"_noon_native_host": host}):
            spec.loader.exec_module(module)
        return module

    def test_output_flags_are_forwarded_to_shared_export_without_a_python_frame_loop(self):
        host = SimpleNamespace(run_scene=Mock(), run_source=AsyncMock(), close_scene=Mock(),
                               export_scene=Mock(), export_source=AsyncMock(return_value={"frames": 3}))
        module = self.load(host)
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "scene.py"
            source.write_text("scene bytes")
            argv = ["noon_native", str(source), "--output", str(Path(directory) / "out"),
                    "--portable-source", "--quality", "l", "--resolution", "65,33",
                    "--fps", "60000/1001", "--format", "png", "--start-frame", "4",
                    "--max-frames", "20", "--final-hold", ".1", "--overwrite", "--fallback"]
            out = io.StringIO()
            with patch.object(sys, "argv", argv), redirect_stdout(out):
                module.main()
        args, options = host.export_source.call_args
        self.assertEqual(args[0], "scene bytes")
        self.assertEqual(options["frame_rate"], "60000/1001")
        self.assertEqual((options["quality"], options["resolution"], options["format"]),
                         ("l", "65,33", "png"))
        self.assertEqual((options["start_frame"], options["max_frames"], options["final_hold"]),
                         (4, 20, .1))
        self.assertTrue(options["portable"] and options["overwrite"] and options["fallback"])
        self.assertIn('"frames": 3', out.getvalue())
        host.run_source.assert_not_called()

    def test_sampled_mode_keeps_existing_scene_retirement(self):
        scene = SimpleNamespace(time=.5)
        host = SimpleNamespace(run_scene=Mock(), run_source=AsyncMock(return_value=scene),
                               close_scene=Mock(), export_scene=Mock(), export_source=AsyncMock())
        module = self.load(host)
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "scene.py"
            source.write_text("sampled source")
            with patch.object(sys, "argv", ["noon_native", str(source), "--sample-hz", "8"]):
                module.main()
        self.assertEqual(host.run_source.call_args.kwargs["sample_hz"], 8)
        host.close_scene.assert_called_once_with(scene)
        host.export_source.assert_not_called()


if __name__ == "__main__":
    unittest.main()
