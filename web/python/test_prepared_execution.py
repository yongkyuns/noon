"""Optional Python dispatch only; the real engine is qualified in WASM tests."""
import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class PreparedExecutionTests(unittest.TestCase):
    def run_case(self, body):
        source = '''
import sys
from types import ModuleType, SimpleNamespace
bridge = ModuleType("js")
bridge.noonResolveAnimationOptions = lambda *args: None
sys.modules["js"] = bridge
from _manim_scene import LiveExecution

def facade(context):
    value = LiveExecution.__new__(LiveExecution)
    value._context = context
    return value
'''
        python_dir = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", source + textwrap.dedent(body)], cwd=python_dir,
            env={**os.environ, "PYTHONPATH": str(python_dir)},
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_prepare_uses_one_engine_owned_boundary_call(self):
        self.run_case('''
            calls = []
            def prepare():
                calls.append("prepare")
                return 2.5
            assert facade(SimpleNamespace(livePrepareSegment=prepare)).prepare() == 2.5
            assert calls == ["prepare"]
        ''')

    def test_disabled_feature_has_no_timing_or_live_fallback(self):
        self.run_case('''
            for context in [SimpleNamespace(), SimpleNamespace(livePrepareSegment=None)]:
                try:
                    facade(context).prepare()
                except NotImplementedError as error:
                    assert "prepared-prefix" in str(error)
                else:
                    raise AssertionError("disabled feature must fail explicitly")
        ''')

    def test_source_failure_is_not_retried(self):
        self.run_case('''
            calls = []
            failure = ValueError("pending source rejected")
            def prepare():
                calls.append("prepare")
                raise failure
            try:
                facade(SimpleNamespace(livePrepareSegment=prepare)).prepare()
            except ValueError as error:
                assert error is failure
            else:
                raise AssertionError("source failure must propagate")
            assert calls == ["prepare"]
        ''')
