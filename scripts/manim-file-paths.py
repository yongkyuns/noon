"""Compare user filesystem helpers with unchanged pinned Manim definitions."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import shutil
import sys
import tempfile

from pinned_python_helpers import load_helpers

NAMES = {"add_extension_if_not_present", "guarantee_existence",
         "guarantee_empty_existence", "seek_full_path_from_defaults"}


def observations(namespace):
    result = [["signature", name, str(inspect.signature(namespace[name]))]
              for name in sorted(NAMES)]
    calls = []
    marker = object()
    class Suffix:
        def __ne__(self, other): calls.append("ne"); return True
        def __eq__(self, other): calls.append("eq"); return True
        def __add__(self, other): calls.append("add"); return ".composed"
    class UserPath:
        suffix = Suffix()
        def with_suffix(self, suffix): calls.append(["with_suffix", suffix]); return marker
        def mkdir(self, **kwargs): calls.append(["mkdir", kwargs])
        def resolve(self, **kwargs): calls.append(["resolve", kwargs]); return marker
    result.append(["path_protocol", namespace["add_extension_if_not_present"](UserPath(), ".png") is marker, calls.copy()])
    calls.clear()
    result.append(["directory_protocol", namespace["guarantee_existence"](UserPath()) is marker, calls.copy()])
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        def normalize(value):
            if isinstance(value, Path):
                return str(value).replace(str(root), "<tmp>")
            return value
        def record(name, *args):
            try:
                value = namespace[name](*args)
                result.append([name, "return", normalize(value)])
                return value
            except Exception as error:
                result.append([name, "error", type(error).__name__,
                               str(error).replace(str(root), "<tmp>")])
        extension = namespace["add_extension_if_not_present"]
        for path in [Path("plain"), Path("movie.mp4"), Path("archive.tar.gz"),
                     Path(".hidden"), Path("name."), Path("folder/name"), Path("/")]:
            for suffix in [".png", ".mp4", "", "invalid", "/bad"]:
                record("add_extension_if_not_present", path, suffix)
        path = Path("movie.mp4")
        result.append(["matching_identity", extension(path, ".mp4") is path])
        target = root / "nested" / "target"
        sibling = root / "retained.txt"
        sibling.write_text("keep")
        record("guarantee_existence", target)
        (target / "child").mkdir()
        (target / "child" / "old.txt").write_text("remove")
        record("guarantee_existence", target)
        result.append(["preserved", (target / "child" / "old.txt").read_text()])
        record("guarantee_empty_existence", target)
        result.append(["cleared", list(target.iterdir()) == [], sibling.read_text()])
        record("guarantee_empty_existence", root / "new" / "child")
        record("guarantee_existence", sibling)
        record("guarantee_empty_existence", sibling)
        result.append(["sibling_after_errors", sibling.read_text()])
        link = root / "linked"
        link.symlink_to(target, target_is_directory=True)
        record("guarantee_existence", link)
        record("guarantee_empty_existence", link)
        result.append(["symlink_error_preserves_target", target.exists(), link.is_symlink()])
        default = root / "default"
        default.mkdir()
        explicit = root / "picture"
        explicit.write_text("direct")
        for suffix in [".png", ".svg"]:
            (default / ("picture" + suffix)).write_text(suffix)
        record("seek_full_path_from_defaults", explicit, default, [".svg", ".png"])
        record("seek_full_path_from_defaults", "picture", default, [".svg", ".png"])
        record("seek_full_path_from_defaults", "picture", default, [".png", ".svg"])
        record("seek_full_path_from_defaults", "picture", default, iter([".svg"]))
        (default / "picture").mkdir()
        record("seek_full_path_from_defaults", "picture", default, [".svg"])
        record("seek_full_path_from_defaults", "absent", default, [".png", ".png"])
        record("seek_full_path_from_defaults", "absent", default, [])
        record("add_extension_if_not_present", "text.png", ".png")
        record("seek_full_path_from_defaults", explicit, default, None)
        record("seek_full_path_from_defaults", "absent", None, [".png"])
    return result


def qualify(manim_source, *, installed_manim=False):
    source = Path(manim_source) / "manim/utils/file_ops.py"
    reference = load_helpers(source, NAMES, {"Path": Path, "shutil": shutil})
    expected = observations(reference)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import noon
    actual = observations({name: getattr(noon, name) for name in NAMES})
    errors = [] if actual == expected else ["user path observations differ from pinned source"]
    for name in NAMES:
        if name not in noon.__all__:
            errors.append(f"{name}: missing public export")
    if installed_manim:
        from manim.utils import file_ops
        if Path(file_ops.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed Manim file-operations source differs from the pin")
        if observations(vars(file_ops)) != expected:
            errors.append("installed full Manim path observations differ")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "symbols": len(NAMES), "observations": len(expected),
            "installed_module_checked": installed_manim, "errors": errors}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    report = qualify(args.manim_source, installed_manim=args.installed_manim)
    print(json.dumps(report, indent=2))
    return bool(report["errors"])


if __name__ == "__main__":
    raise SystemExit(main())
