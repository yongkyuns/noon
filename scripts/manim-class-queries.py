"""Compare base class queries and generic Group metadata with pinned methods."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import types
import warnings

from pinned_python_helpers import load_helpers

NAMES = {"get_group_class", "get_mobject_type_class"}


def metadata_observations(group_class):
    """Only arbitrary Python metadata: no geometry, scene or paint observation."""
    result = []
    for value in (None, True, -1, 2, float("nan"), float("inf"), "text", [], {}, object()):
        for keyword in (None, "value", "opacity"):
            group = object.__new__(group_class)
            with warnings.catch_warnings(record=True) as caught:
                warnings.simplefilter("always")
                try:
                    returned = (group.set_opacity(value) if keyword is None
                                else group.set_opacity(**{keyword: value}))
                    outcome = ["value", returned is group, group.opacity is value]
                except Exception as error:
                    outcome = ["error", type(error).__name__]
            result.append([type(value).__name__, keyword, outcome,
                           [[type(w.message).__name__, str(w.message)] for w in caught]])
    return result


def reference_metadata_group(root):
    methods = load_helpers(root / "manim/mobject/mobject.py", {"__getattr__"},
                           {"warnings": warnings, "types": types}, class_name="Mobject")
    return type("Group", (), {"__getattr__": methods["__getattr__"]})


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


def qualify(manim_source, *, installed_manim=False):
    manim_source = Path(manim_source)
    expected = observations(reference_classes(manim_source))
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import noon
    actual = observations(vars(noon) | {name: getattr(noon, name) for name in ["Mobject", "Group", "VMobject", "VGroup", "Circle", "Square"]})
    errors = [] if actual == expected else ["class-query observations differ from pinned methods"]
    metadata_expected = metadata_observations(reference_metadata_group(manim_source))
    if metadata_observations(noon.Group) != metadata_expected:
        errors.append("generic Group metadata/warning/call-boundary observations differ")
    files = ["manim/mobject/mobject.py", "manim/mobject/types/vectorized_mobject.py"]
    if installed_manim:
        import manim
        import manim.mobject.mobject as mobject
        import manim.mobject.types.vectorized_mobject as vector
        for relative, module in zip(files, [mobject, vector]):
            if Path(module.__file__).read_bytes() != (manim_source / relative).read_bytes():
                raise ValueError("installed class module differs from pin")
        if observations(vars(manim)) != expected:
            errors.append("installed class-query observations differ")
        if metadata_observations(manim.Group) != metadata_expected:
            errors.append("installed generic Group metadata observations differ")
    return {"source_sha256": {relative: hashlib.sha256((manim_source / relative).read_bytes()).hexdigest() for relative in files},
                      "observations": len(expected), "installed_module_checked": installed_manim,
                      "generic_group_metadata_observations": len(metadata_expected),
                      "errors": errors}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    report = qualify(args.manim_source, installed_manim=args.installed_manim)
    print(json.dumps(report, indent=2))
    return bool(report["errors"])


if __name__ == "__main__": raise SystemExit(main())
