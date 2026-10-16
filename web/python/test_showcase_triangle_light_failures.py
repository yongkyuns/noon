"""Regressions for the failed preview: source assembly, not a mock renderer."""
import ast
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "web/python/examples/showcase_triangle_light.py"
TREE = ast.parse(SOURCE.read_text())


class ArtworkToken:
    """Identity-only artwork stand-in; deliberately has no rendering semantics."""
    def __init__(self, *args, **kwargs):
        pass

    def set_fill(self, **kwargs):
        return self


def artwork_membership():
    nodes = [node for node in TREE.body
             if isinstance(node, (ast.Import, ast.ImportFrom, ast.FunctionDef, ast.Assign))
             and not (isinstance(node, ast.ImportFrom) and node.module == "noon")]
    namespace = {}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(SOURCE), "exec"), namespace)
    for constructor in ("ImageMobject", "panel", "oval", "artwork_path"):
        namespace[constructor] = ArtworkToken
    namespace["wood_texture"] = lambda: b"not-rendered"
    return namespace["laboratory_artwork"]()


class TrianglePreviewFailureTests(unittest.TestCase):
    def test_fading_mouth_is_not_owned_by_the_room_group(self):
        room, arms, mouth, hands = artwork_membership()
        self.assertFalse(mouth in room, "FadeOut requires mouth to be admitted as a scene root")
        self.assertTrue(all(part in room for part in (*arms, *hands)),
                        "ordinary Transform targets should retain the existing room composition")

    def test_mouth_has_a_separate_initial_fade_in_before_its_fade_out(self):
        scene = next(n for n in TREE.body if isinstance(n, ast.ClassDef))
        construct = next(n for n in scene.body if isinstance(n, ast.FunctionDef) and n.name == "construct")
        introduction = next(n.value for n in construct.body if isinstance(n, ast.Expr)
                            and isinstance(n.value, ast.Call) and ast.unparse(n.value.func) == "self.play")
        direct_fades = [ast.unparse(arg) for arg in introduction.args]
        self.assertIn("FadeIn(mouth)", direct_fades)
        self.assertIn("FadeIn(Group(*room))", direct_fades)
        self.assertEqual(next(k.value.value for k in introduction.keywords if k.arg == "run_time"), 0.8)
        self.assertTrue(any(isinstance(n, ast.Call) and ast.unparse(n) == "FadeOut(mouth)"
                            for n in ast.walk(construct)), "keep the intended pose transition")


if __name__ == "__main__":
    unittest.main()
