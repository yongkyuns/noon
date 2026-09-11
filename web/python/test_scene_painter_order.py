"""Scene painter-order methods remain thin projections of shared membership authority."""

import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class ScenePainterOrderFacadeTests(unittest.TestCase):
    def test_front_reuses_add_and_back_uses_one_membership_batch(self):
        source = textwrap.dedent("""
            import noon

            scene = noon.Scene()
            edits = []
            adds = []

            def edit(kind, values=(), *, key=None):
                edits.append((kind, values, key))

            def add(*values, **kwargs):
                adds.append((values, kwargs))
                return scene

            scene._edit_membership = edit
            scene.add = add
            first = object()
            second = object()

            assert scene.bring_to_front(first, second) is scene
            assert adds == [((first, second), {})]
            assert edits == []

            assert scene.bring_to_back(second, first) is scene
            assert edits == [("bring_to_back", (second, first), None)]
            assert adds == [((first, second), {})]
        """)
        python_dir = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env={**os.environ, "PYTHONPATH": str(python_dir)},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
