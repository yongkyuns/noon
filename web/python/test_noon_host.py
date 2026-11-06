import importlib.util
from pathlib import Path
import sys
from types import ModuleType
import unittest
from unittest.mock import patch


class HostCapabilityTests(unittest.TestCase):
    def test_browser_wait_primitives_observe_the_current_host_capability(self):
        ffi = ModuleType("pyodide.ffi")
        ffi.can_run_sync = lambda: True
        ffi.run_sync = lambda value: ("original", value)
        spec = importlib.util.spec_from_file_location("host_fixture", Path(__file__).with_name("_noon_host.py"))
        module = importlib.util.module_from_spec(spec)
        with patch.object(sys, "platform", "emscripten"), patch.dict(sys.modules, {
            "js": ModuleType("js"), "pyodide.ffi": ffi,
        }):
            spec.loader.exec_module(module)
            self.assertTrue(module.can_wait_sync())
            ffi.can_run_sync = lambda: False
            ffi.run_sync = lambda value: ("replacement", value)
            self.assertFalse(module.can_wait_sync())
            self.assertEqual(module.wait_sync(7), ("replacement", 7))


if __name__ == "__main__":
    unittest.main()
