"""Lazy facade publication avoids repeated imports without eager resources."""
import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class LazyExportTests(unittest.TestCase):
    def test_publish_once_preserves_identity_lazy_failure_and_public_rebinding(self):
        source = textwrap.dedent('''
            import importlib
            from unittest.mock import patch
            import types
            import sys
            import noon

            # Basic facade import does not initialize the optional resource APIs.
            assert '_manim_latex' not in sys.modules
            assert '_manim_animate' not in sys.modules
            for name in ('Create', 'Uncreate', 'FadeIn', 'FadeOut', 'Transform'):
                assert name not in vars(noon)
                # Resolve the defining module before isolating the export lookup.
                module = importlib.import_module(noon._PUBLIC_EXPORTS[name])
                expected = getattr(module, name)
                with patch.object(importlib, 'import_module', wraps=importlib.import_module) as resolve:
                    for _ in range(1200):
                        assert getattr(noon, name) is expected
                    assert resolve.call_count == 1, (name, resolve.call_count)
                assert vars(noon)[name] is expected

            # An absent optional export stays absent and can be prepared later.
            module_name = '_noon_optional_export_test'
            module = types.ModuleType(module_name)
            sys.modules[module_name] = module
            noon._PUBLIC_EXPORTS['OptionalTest'] = module_name
            try:
                noon.OptionalTest
            except AttributeError:
                pass
            else:
                raise AssertionError('missing export was manufactured')
            assert 'OptionalTest' not in vars(noon)
            marker = object()
            module.OptionalTest = marker
            assert noon.OptionalTest is marker
            assert noon.OptionalTest is marker
            assert '_manim_latex' not in sys.modules

            # Public rebinding is ordinary Python; deletion restores lazy lookup.
            original = noon.Transform
            noon.Transform = marker
            assert noon.Transform is marker
            del noon.Transform
            assert noon.Transform is original
            assert not hasattr(noon, 'DefinitelyUnknownExport')
        ''')
        root = Path(__file__).resolve().parent
        env = dict(os.environ, PYTHONPATH=str(root))
        result = subprocess.run([sys.executable, '-c', source], cwd=root, env=env,
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main()
