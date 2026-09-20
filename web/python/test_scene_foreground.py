"""Foreground compatibility metadata projects ordering through shared Rust membership edits."""

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

    def test_foreground_stays_last_across_adds_and_removal_only_changes_metadata(self):
        self.run_source("""
            import noon
            import _manim_compat as compat

            compat._leaf_mobjects = lambda value: [value]
            scene = noon.Scene()
            edits = []
            scene._edit_membership = lambda kind, values=(), key=None: edits.append(
                (kind, values, key)
            )
            back = object()
            front = object()
            later = object()

            assert scene.add_foreground_mobject(front) is scene
            assert scene.foreground_mobjects == [front]
            assert edits[-1] == ("add", (front,), None)

            scene.add(back)
            assert edits[-1] == ("add", (back, front), None)
            assert scene.foreground_mobjects == [front]

            assert scene.remove_foreground_mobject(front) is scene
            assert scene.foreground_mobjects == []
            assert edits[-1] == ("add", (back, front), None)

            scene.add(later)
            assert edits[-1] == ("add", (later,), None)
        """)

    def test_multiple_foreground_order_is_stable_and_readding_moves_to_foreground_tail(self):
        self.run_source("""
            import noon
            import _manim_compat as compat

            compat._leaf_mobjects = lambda value: [value]
            scene = noon.Scene()
            edits = []
            scene._edit_membership = lambda kind, values=(), key=None: edits.append(
                (kind, values, key)
            )
            first = object()
            second = object()
            third = object()

            scene.add_foreground_mobjects(first, second)
            assert scene.foreground_mobjects == [first, second]
            assert edits[-1] == ("add", (first, second), None)

            scene.add_foreground_mobject(first)
            assert scene.foreground_mobjects == [second, first]
            assert edits[-1] == ("add", (second, first), None)

            scene.add(third)
            assert edits[-1] == ("add", (third, second, first), None)
        """)

    def test_failed_foreground_add_does_not_commit_compatibility_metadata(self):
        self.run_source("""
            import noon

            scene = noon.Scene()
            existing = object()
            scene.foreground_mobjects = [existing]

            def reject(*args, **kwargs):
                raise RuntimeError("rejected")

            scene._edit_membership = reject
            candidate = object()
            try:
                scene.add_foreground_mobject(candidate)
            except RuntimeError as error:
                assert str(error) == "rejected"
            else:
                raise AssertionError("expected rejection")
            assert scene.foreground_mobjects == [existing]
        """)

    def test_remove_clear_and_bring_to_back_retire_foreground_status_after_success(self):
        self.run_source("""
            import noon

            scene = noon.Scene()
            edits = []
            scene._edit_membership = lambda kind, values=(), key=None: edits.append(
                (kind, values, key)
            )
            first = object()
            second = object()
            scene.foreground_mobjects = [first, second]

            assert scene.bring_to_back(first) is scene
            assert edits[-1] == ("bring_to_back", (first,), None)
            assert scene.foreground_mobjects == [second]

            assert scene.remove(second) is scene
            assert edits[-1] == ("remove", (second,), None)
            assert scene.foreground_mobjects == []

            scene.foreground_mobjects = [first]
            assert scene.clear() is scene
            assert edits[-1] == ("clear", (), None)
            assert scene.foreground_mobjects == []
        """)

    def test_keyed_single_add_keeps_key_on_new_object_while_foreground_is_reordered(self):
        self.run_source("""
            import noon
            import _manim_compat as compat

            compat._leaf_mobjects = lambda value: [value]
            scene = noon.Scene()
            edits = []
            scene._edit_membership = lambda kind, values=(), key=None: edits.append(
                (kind, values, key)
            )
            foreground = object()
            added = object()
            scene.foreground_mobjects = [foreground]

            scene.add(added, key="stable")
            assert edits == [("add", (added, foreground), "stable")]

            try:
                scene.add(object(), object(), key="invalid")
            except ValueError as error:
                assert "one ordinary Mobject" in str(error)
            else:
                raise AssertionError("expected keyed batch rejection")
        """)

    def test_failed_replace_does_not_change_foreground_metadata(self):
        self.run_source("""
            import noon

            scene = noon.Scene()
            old = object()
            replacement = object()
            scene.foreground_mobjects = [old]

            def reject(*args, **kwargs):
                raise RuntimeError("rejected")

            scene._edit_membership = reject
            try:
                scene.replace(old, replacement)
            except RuntimeError as error:
                assert str(error) == "rejected"
            else:
                raise AssertionError("expected replacement rejection")
            assert scene.foreground_mobjects == [old]
        """)

    def test_replace_retires_foreground_before_a_later_add_can_resurrect_it(self):
        self.run_source("""
            import noon
            import _manim_compat as compat

            compat._leaf_mobjects = lambda value: [value]
            scene = noon.Scene()
            edits = []
            scene._edit_membership = lambda kind, values=(), key=None: edits.append(
                (kind, values, key)
            )
            old = object()
            replacement = object()
            later = object()
            scene.foreground_mobjects = [old]

            assert scene.replace(old, replacement) is scene
            assert edits[-1] == ("replace", (old, replacement), None)
            assert scene.foreground_mobjects == []

            scene.add(later)
            assert edits[-1] == ("add", (later,), None)
        """)

    def test_failed_group_removal_does_not_change_foreground_metadata(self):
        self.run_source("""
            import noon
            import _manim_compat as compat

            scene = noon.Scene()
            child = object()
            group = object.__new__(compat.Group)
            group.submobjects = [child]
            scene.foreground_mobjects = [child]

            def reject(*args, **kwargs):
                raise RuntimeError("rejected")

            scene._edit_membership = reject
            try:
                scene.remove(group)
            except RuntimeError as error:
                assert str(error) == "rejected"
            else:
                raise AssertionError("expected removal rejection")
            assert scene.foreground_mobjects == [child]
        """)

    def test_group_removal_retires_foreground_child_before_later_add(self):
        self.run_source("""
            import noon
            import _manim_compat as compat

            compat._leaf_mobjects = lambda value: list(
                value.submobjects if isinstance(value, compat.Group) else [value]
            )
            scene = noon.Scene()
            edits = []
            scene._edit_membership = lambda kind, values=(), key=None: edits.append(
                (kind, values, key)
            )
            child = object()
            sibling = object()
            group = object.__new__(compat.Group)
            group.submobjects = [child, sibling]
            scene.foreground_mobjects = [child]

            assert scene.remove(group) is scene
            assert scene.foreground_mobjects == []

            later = object()
            scene.add(later)
            assert edits[-1] == ("add", (later,), None)
        """)


if __name__ == "__main__":
    unittest.main()
