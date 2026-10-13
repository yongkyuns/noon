"""Compare base Mobject class queries with unchanged pinned class methods."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from pinned_python_helpers import load_helpers

NAMES = {"get_group_class", "get_mobject_type_class"}


def observations(classes):
    result = []
    for name in ["Mobject", "Group", "VMobject", "VGroup", "Circle", "Square"]:
        cls = classes[name]
        obj = object.__new__(cls)
        result.append([name, obj.get_group_class().__name__, obj.get_mobject_type_class().__name__, cls.get_mobject_type_class().__name__])
    class Custom(classes["VGroup"]): pass
    result.append([object.__new__(Custom).get_group_class().__name__, Custom.get_mobject_type_class().__name__])
    class Override(Custom):
        def get_group_class(self): return Custom
        @staticmethod
        def get_mobject_type_class(): return Custom
    result.append([object.__new__(Override).get_group_class() is Custom, Override.get_mobject_type_class() is Custom])
    return result


def reference_classes(root):
    mobject_source = root / "manim/mobject/mobject.py"
    vector_source = root / "manim/mobject/types/vectorized_mobject.py"
    scope = {}
    m = load_helpers(mobject_source, NAMES, scope, class_name="Mobject")
    base = type("Mobject", (), {name: m[name] for name in NAMES})
    group = type("Group", (base,), {})
    m.update(Mobject=base, Group=group)
    v = load_helpers(vector_source, NAMES, {}, class_name="VMobject")
    vector = type("VMobject", (base,), {name: v[name] for name in NAMES})
    vgroup = type("VGroup", (vector,), {})
    v.update(VMobject=vector, VGroup=vgroup)
    return {"Mobject": base, "Group": group, "VMobject": vector, "VGroup": vgroup,
            "Circle": type("Circle", (vector,), {}), "Square": type("Square", (vector,), {})}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    expected = observations(reference_classes(args.manim_source))
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import noon
    actual = observations(vars(noon) | {name: getattr(noon, name) for name in ["Mobject", "Group", "VMobject", "VGroup", "Circle", "Square"]})
    errors = [] if actual == expected else ["class-query observations differ from pinned methods"]
    files = ["manim/mobject/mobject.py", "manim/mobject/types/vectorized_mobject.py"]
    if args.installed_manim:
        import manim
        import manim.mobject.mobject as mobject
        import manim.mobject.types.vectorized_mobject as vector
        for relative, module in zip(files, [mobject, vector]):
            if Path(module.__file__).read_bytes() != (args.manim_source / relative).read_bytes():
                raise ValueError("installed class module differs from pin")
        if observations(vars(manim)) != expected:
            errors.append("installed class-query observations differ")
    print(json.dumps({"source_sha256": {relative: hashlib.sha256((args.manim_source / relative).read_bytes()).hexdigest() for relative in files},
                      "observations": len(expected), "installed_module_checked": args.installed_manim,
                      "errors": errors}, indent=2))
    return bool(errors)


if __name__ == "__main__": raise SystemExit(main())
