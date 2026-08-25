from pathlib import Path

root = Path(__file__).resolve().parents[1]
path = root / "scripts" / "manim-differential.py"
source = path.read_text()
anchor = 'import noon as noon  # noqa: E402\n\ntry:\n'
replacement = '''import noon as noon  # noqa: E402
import _manim_compat as _manim_compat  # noqa: E402

# The browser worker installs this facade before user code runs. Differential
# fixtures must exercise that same public Manim-compatible surface rather than
# the lower-level authoring primitives that happen to back it.
_manim_compat.install()

try:
'''
if anchor not in source:
    raise SystemExit("Noon import anchor not found")
path.write_text(source.replace(anchor, replacement, 1))

(root / "parity-diagnostics.json").unlink(missing_ok=True)
for cache in (root / "web" / "python").glob("__pycache__"):
    for child in cache.iterdir():
        child.unlink()
    cache.rmdir()

Path(__file__).unlink()
(root / ".github" / "workflows" / "finalize-parity-o1.yml").unlink(missing_ok=True)
