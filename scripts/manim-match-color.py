"""Compare the Python match_color method with untouched pinned Manim code."""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import sys


def observations(method):
    output = []
    for value in [None, "red", (0.2, 0.4, 0.8), object()]:
        calls = []
        class Donor:
            def get_color(self):
                calls.append("get")
                return value
        class Recipient:
            def set_color(self, color):
                calls.append(["set", color is value])
                return self
        recipient = Recipient()
        result = method(recipient, mobject=Donor())
        output.append([result is recipient, calls])
    calls = []
    class SelfMatch:
        def get_color(self):
            calls.append("get")
            return self
        def set_color(self, color):
            calls.append(["set", color is self])
            return "setter result"
    obj = SelfMatch()
    output.append([method(obj, obj), calls])
    for which in ["get", "set"]:
        calls = []
        class Failure:
            def get_color(self):
                calls.append("get")
                if which == "get": raise RuntimeError("unavailable")
                return "blue"
            def set_color(self, color):
                calls.append("set")
                raise RuntimeError("unavailable")
        obj = Failure()
        try: method(obj, obj)
        except RuntimeError: output.append([which, calls])
    return output


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    source = args.manim_source / "manim/mobject/mobject.py"
    tree = ast.parse(source.read_text(), filename=str(source))
    cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "Mobject")
    method = next(node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name == "match_color")
    future = ast.parse("from __future__ import annotations").body[0]
    namespace = {}
    exec(compile(ast.Module(body=[future, method], type_ignores=[]), str(source), "exec"), namespace)
    expected = observations(namespace["match_color"])
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import noon
    import _manim_compat
    errors = [] if observations(noon.Mobject.match_color) == expected else ["method observations differ"]
    if _manim_compat.Group.match_color is not noon.Mobject.match_color:
        errors.append("group method differs")
    if args.installed_manim:
        import inspect
        from manim import Mobject, Circle, Square, VGroup, RED, BLUE, GREEN
        if Path(inspect.getsourcefile(Mobject)).read_bytes() != source.read_bytes():
            raise ValueError("installed Mobject source differs from pin")
        if observations(Mobject.match_color) != expected: errors.append("installed method differs")
        donor = Circle(fill_color=RED, fill_opacity=0, stroke_color=BLUE)
        recipient = Square(fill_color=RED, fill_opacity=0.25, stroke_opacity=0.6)
        points = recipient.points.copy()
        if recipient.match_color(donor) is not recipient: errors.append("installed return identity differs")
        if recipient.get_color() != BLUE: errors.append("installed hidden-fill selection differs")
        if recipient.get_fill_opacity() != 0.25 or recipient.get_stroke_opacity() != 0.6:
            errors.append("installed match changed opacity")
        if not (recipient.points == points).all(): errors.append("installed match changed geometry")
        family = VGroup(recipient, VGroup(recipient))
        donor.set_color(GREEN)
        family.match_color(donor)
        if recipient.get_color() != GREEN or family[0] is not family[1][0]:
            errors.append("installed aliased family behavior differs")
    print(json.dumps({"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                      "observations": len(expected), "installed_method_checked": args.installed_manim,
                      "errors": errors}, indent=2))
    return bool(errors)


if __name__ == "__main__": raise SystemExit(main())
