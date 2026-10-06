"""Module resolution is lazy; operation results and lifecycle checks stay live."""
from pathlib import Path
import os
import subprocess
import sys
import textwrap
import unittest


class OperationModuleTests(unittest.TestCase):
    def test_lazy_modules_keep_operation_dispatch_live(self):
        program = textwrap.dedent('''
            import builtins
            import sys
            from unittest.mock import patch
            from types import ModuleType
            # Import fixture only; no simulated engine or parity claim.
            bridge = ModuleType("js")
            bridge.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = bridge
            import noon

            names = {
                '_semantic_operations': '_manim_semantic_handles',
                '_callback_operations': '_manim_updaters',
                '_scene_operations': '_manim_scene',
            }
            assert '_manim_latex' not in sys.modules
            assert '_manim_animate' not in sys.modules
            for accessor, name in names.items():
                resolver = getattr(noon, accessor)
                resolver.cache_clear()
                calls = []
                original = builtins.__import__
                def traced(module_name, *args, **kwargs):
                    if module_name == name:
                        calls.append(module_name)
                    return original(module_name, *args, **kwargs)
                # Complete circular initialization before counting steady accesses.
                expected = original(name)
                with patch.object(builtins, '__import__', traced):
                    for _ in range(2048):
                        assert resolver() is expected
                assert len(calls) == 1, (name, len(calls))

            obj = object.__new__(noon.Mobject)
            callbacks = noon._callback_operations()
            # Cache the module, never an operation or its result. Rebinding an
            # operation remains visible after earlier calls warmed the resolver.
            with patch.object(callbacks, '_canonical_get_center', return_value=noon.RIGHT) as query:
                for _ in range(2048):
                    assert obj.get_center() is noon.RIGHT
                assert query.call_count == 2048
            marker = RuntimeError('retired owner')
            with patch.object(callbacks, '_canonical_get_center', side_effect=marker) as query:
                try:
                    obj.get_center()
                except RuntimeError as error:
                    assert error is marker
                else:
                    raise AssertionError('ownership failure was cached away')
                assert query.call_count == 1
        ''')
        root = Path(__file__).resolve().parent
        result = subprocess.run([sys.executable, '-c', program], cwd=root,
                                env=dict(os.environ, PYTHONPATH=str(root)),
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_failed_lazy_import_can_be_retried(self):
        program = textwrap.dedent('''
            import builtins
            from unittest.mock import patch
            import noon
            resolver = noon._semantic_operations
            original = builtins.__import__
            def unavailable(name, *args, **kwargs):
                if name == '_manim_semantic_handles':
                    raise ImportError('intentional missing binding')
                return original(name, *args, **kwargs)
            with patch.object(builtins, '__import__', unavailable):
                try:
                    resolver()
                except ImportError as error:
                    assert str(error) == 'intentional missing binding'
                else:
                    raise AssertionError('missing module manufactured')
            expected = original('_manim_semantic_handles')
            assert resolver() is expected
        ''')
        root = Path(__file__).resolve().parent
        result = subprocess.run([sys.executable, '-c', program], cwd=root,
                                env=dict(os.environ, PYTHONPATH=str(root)),
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


    def test_inactive_provisional_dispatch_rechecks_the_next_phase(self):
        program = textwrap.dedent('''
            from types import SimpleNamespace
            import _manim_updaters as callbacks
            class NoProvisionalMarkers:
                def __getattr__(self, name):
                    raise AssertionError("inactive dispatch inspected " + name)
            guard = callbacks._ACTIVE_CANONICAL_CONTEXT.set(None)
            try:
                assert callbacks._canonical_provisional_context(NoProvisionalMarkers()) is None
                scene, owner, local = object(), object(), object()
                frame = {"time": 0, "delta_time": 0, "token": {"sequence": 2}, "objects": []}
                initial = callbacks._CanonicalCallbackContext(frame, owner, scene=scene)
                current = callbacks._CanonicalCallbackContext(frame, owner, scene=scene)
                obj = SimpleNamespace(_callback_provisional_context=initial,
                                      _callback_provisional_handle=local)
                assert callbacks._canonical_provisional_context(obj) is None
                active = callbacks._ACTIVE_CANONICAL_CONTEXT.set(current)
                try:
                    assert callbacks._canonical_provisional_context(obj) == (current, local)
                    # Current region, not the constructor's context, owns the view.
                    assert current is not initial
                    for attribute, replacement in (("token", {"sequence": 3}),
                                                    ("_scene", object()),
                                                    ("_authoring_context", object())):
                        previous = getattr(current, attribute)
                        setattr(current, attribute, replacement)
                        assert callbacks._canonical_provisional_context(obj) is None
                        setattr(current, attribute, previous)
                        assert callbacks._canonical_provisional_context(obj) == (current, local)
                finally:
                    callbacks._ACTIVE_CANONICAL_CONTEXT.reset(active)
                assert callbacks._canonical_provisional_context(obj) is None
            finally:
                callbacks._ACTIVE_CANONICAL_CONTEXT.reset(guard)
        ''')
        root = Path(__file__).resolve().parent
        result = subprocess.run([sys.executable, '-c', program], cwd=root,
                                env=dict(os.environ, PYTHONPATH=str(root)),
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_empty_constructor_options_do_not_skip_later_validation(self):
        program = textwrap.dedent('''
            from types import SimpleNamespace
            from unittest.mock import Mock
            from _manim_semantic_handles import _apply_shared_constructor_options as apply
            builder = SimpleNamespace(setZIndex=Mock())
            apply(builder, {})
            builder.setZIndex.assert_not_called()
            options = {"z_index": 2}
            apply(builder, options)
            builder.setZIndex.assert_called_once_with(2.0)
            assert options == {"z_index": 2}
            builder.setZIndex.reset_mock()
            try:
                apply(builder, {"unknown_option": True, "z_index": 5})
            except TypeError:
                pass
            else:
                raise AssertionError("unknown constructor option accepted")
            builder.setZIndex.assert_not_called()
            try:
                apply(builder, {"z_index": float("inf")})
            except ValueError:
                pass
            else:
                raise AssertionError("nonfinite constructor option accepted")
            builder.setZIndex.assert_not_called()
        ''')
        root = Path(__file__).resolve().parent
        result = subprocess.run([sys.executable, '-c', program], cwd=root,
                                env=dict(os.environ, PYTHONPATH=str(root)),
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main()
