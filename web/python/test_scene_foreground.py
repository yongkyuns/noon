"""Python foreground APIs are thin projections of shared Rust membership semantics."""

import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class SceneForegroundFacadeTests(unittest.TestCase):
    def run_source(self, source: str) -> None:
        python_dir = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", textwrap.dedent(source)],
            cwd=python_dir,
            env={**os.environ, "PYTHONPATH": str(python_dir)},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_foreground_methods_dispatch_shared_membership_kinds_without_local_state(self):
        self.run_source("""
            import noon

            scene = noon.Scene()
            edits = []
            scene._edit_membership = lambda kind, values=(), **kwargs: edits.append(
                (kind, values, kwargs)
            )
            first = object()
            second = object()

            assert scene.add_foreground_mobjects(first, second) is scene
            assert edits[-1] == ("add_foreground", (first, second), {})
            assert scene.remove_foreground_mobject(first) is scene
            assert edits[-1] == ("remove_foreground", (first,), {})
            assert not hasattr(scene, "__dict__") or "foreground_mobjects" not in scene.__dict__
        """)

    def test_foreground_property_is_a_derived_host_query(self):
        self.run_source("""
            import noon

            first = object()
            second = object()
            class Operations:
                @staticmethod
                def _canonical_scene_foreground_mobjects(scene):
                    assert isinstance(scene, noon.Scene)
                    return [first, second]

            noon._scene_operations = lambda: Operations
            scene = noon.Scene()
            assert scene.foreground_mobjects == [first, second]
            assert "foreground_mobjects" not in scene.__dict__
        """)

    def test_ordinary_add_no_longer_projects_a_python_foreground_list(self):
        self.run_source("""
            import noon
            import _manim_compat as compat

            compat._leaf_mobjects = lambda value: [value]
            scene = noon.Scene()
            edits = []
            scene._edit_membership = lambda kind, values=(), **kwargs: edits.append(
                (kind, values, kwargs)
            )
            added = object()
            scene.add(added)
            assert edits == [("add", (added,), {"key": None})]
        """)

    def test_keyed_add_keeps_caller_identity_without_foreground_rewriting(self):
        self.run_source("""
            import noon
            import _manim_compat as compat

            compat._leaf_mobjects = lambda value: [value]
            scene = noon.Scene()
            edits = []
            scene._edit_membership = lambda kind, values=(), **kwargs: edits.append(
                (kind, values, kwargs)
            )
            added = object.__new__(noon.Mobject)
            scene.add(added, key="stable")
            assert edits == [("add", (added,), {"key": "stable"})]
        """)

    def test_remove_back_clear_and_replace_have_no_python_foreground_cleanup(self):
        self.run_source("""
            import noon

            scene = noon.Scene()
            edits = []
            scene._edit_membership = lambda kind, values=(), **kwargs: edits.append(
                (kind, values, kwargs)
            )
            first = object()
            second = object()
            assert scene.bring_to_back(first) is scene
            assert scene.remove(second) is scene
            assert scene.replace(first, second) is scene
            assert scene.clear() is scene
            assert edits == [
                ("bring_to_back", (first,), {}),
                ("remove", (second,), {}),
                ("replace", (first, second), {}),
                ("clear", (), {}),
            ]
        """)

    def test_failed_shared_edit_has_no_python_foreground_state_to_rollback(self):
        self.run_source("""
            import noon

            scene = noon.Scene()
            def reject(*args, **kwargs):
                raise RuntimeError("rejected")
            scene._edit_membership = reject
            try:
                scene.add_foreground_mobject(object())
            except RuntimeError as error:
                assert str(error) == "rejected"
            else:
                raise AssertionError("expected rejection")
            assert "foreground_mobjects" not in scene.__dict__
        """)

    def test_old_python_foreground_policy_helpers_are_deleted(self):
        self.run_source("""
            import noon

            assert not hasattr(noon.Scene, "_identity_list_update")
            assert not hasattr(noon.Scene, "_foreground_add_order")
            assert not hasattr(noon.Scene, "_restructure_foreground")
        """)


if __name__ == "__main__":
    unittest.main()
