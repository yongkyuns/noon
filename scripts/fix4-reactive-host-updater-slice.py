from pathlib import Path

path = Path("web/python/test_moving_dots_primitives.py")
text = path.read_text()
old = '''import unittest

import _manim_compat as manim
import _manim_reactive as reactive
import _manim_updaters as updaters
'''
new = '''import sys
import types
import unittest

fake_js = types.ModuleType("js")
fake_js.noonResolveAnimationOptions = lambda *args: None
sys.modules["js"] = fake_js

import _manim_compat as manim
import _manim_geometry  # noqa: F401 - installs match_points/layout semantics
import _manim_reactive as reactive
import _manim_updaters as updaters

updaters.install()
'''
if text.count(old) != 1:
    raise SystemExit(f"expected one MovingDots test import block, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
