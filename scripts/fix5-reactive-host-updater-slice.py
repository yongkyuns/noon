from pathlib import Path

path = Path("web/python/test_moving_dots_primitives.py")
text = path.read_text()
old = "import _manim_updaters as updaters\n\nupdaters.install()\n"
new = "import _manim_updaters as updaters\nimport noon as api\n\nupdaters.install()\n"
if text.count(old) != 1:
    raise SystemExit(f"expected one updater-test import tail, found {text.count(old)}")
text = text.replace(old, new, 1)
text = text.replace("manim.RED", "api.RED")
path.write_text(text)
