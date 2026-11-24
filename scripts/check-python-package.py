#!/usr/bin/env python3
"""Validate local Python release artifacts, then install outside the checkout.

Build both artifacts with ``python -m build``: its default workflow builds the
wheel from the sdist. Then run this script with the wheel and ``--sdist FILE``.
Use ``--render --mp4`` on a worker with a native GPU adapter and FFmpeg/ffprobe.
Use ``--installed`` inside cibuildwheel's test environment to reuse the installed
checks without creating another environment or reinstalling the package.
No registry upload is performed. ``--check-only --sdist FILE`` needs no Rust,
GPU, installed Noon, or third-party Python packages.
"""
from __future__ import annotations

import argparse
import ast
import configparser
from email.parser import BytesParser
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import posixpath
import re
import shutil
import struct
import subprocess
import sys
import sysconfig
import tarfile
import tempfile
import tomllib
import venv
import zipfile

ROOT = Path(__file__).resolve().parents[1]
INCLUDE = re.compile(r'include_(?:bytes|str)!\s*\(\s*"([^"]+)"', re.MULTILINE)


class PackageError(ValueError):
    """An artifact is incomplete or does not match its declared contract."""


def require(condition, message):
    if not condition:
        raise PackageError(message)


def config(root=ROOT):
    return tomllib.loads((root / "pyproject.toml").read_text(encoding="utf8"))


def source_version(root=ROOT):
    tree = ast.parse((root / "web/python/noon.py").read_text(encoding="utf8"))
    for statement in tree.body:
        if isinstance(statement, ast.Assign) and any(
            isinstance(target, ast.Name) and target.id == "__version__"
            for target in statement.targets
        ):
            return ast.literal_eval(statement.value)
    raise PackageError("noon.__version__ must be a literal assignment")


def canonical_modules(root=ROOT):
    return {
        path.stem for path in (root / "web/python").glob("*.py")
        if path.stem in {"noon", "noon_native"}
        or path.stem.startswith(("_noon_", "_manim_"))
    }


def checked_configuration(root=ROOT):
    settings = config(root)
    workspace = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf8"))
    require(source_version(root) == workspace["workspace"]["package"]["version"],
            "Python facade and Rust workspace versions differ")
    declared = settings["tool"]["setuptools"]["py-modules"]
    require(len(declared) == len(set(declared)), "duplicate declared Python modules")
    require(set(declared) == canonical_modules(root),
            "py-modules must contain every canonical facade module and no test helpers")
    return settings


def check_file_names(names):
    require(len(names) == len(set(names)), "archive has duplicate file names")
    for name in names:
        path = PurePosixPath(name)
        require(not path.is_absolute() and ".." not in path.parts and "\\" not in name,
                f"unsafe archive path: {name}")


def normalized_requirement(requirement):
    name, constraints = re.match(r"([\w.-]+)(.*)", requirement.replace(" ", "")).groups()
    return name.lower().replace("_", "-"), tuple(sorted(constraints.split(",")))


def validate_wheel(path, root=ROOT):
    settings = checked_configuration(root)
    project = settings["project"]
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        check_file_names(names)
        files = set(name for name in names if not name.endswith("/"))
        metadata_paths = [name for name in files if name.endswith(".dist-info/METADATA")]
        require(len(metadata_paths) == 1, "wheel must contain exactly one distribution")
        dist_info = metadata_paths[0].rsplit("/", 1)[0]
        metadata = BytesParser().parsebytes(archive.read(metadata_paths[0]))
        require(metadata["Name"] == project["name"], "wheel distribution name differs from project")
        require(metadata["Version"] == source_version(root), "wheel version differs from facade")
        require(metadata["Requires-Python"] == project["requires-python"], "wrong Python requirement")
        require({normalized_requirement(value) for value in metadata.get_all("Requires-Dist", [])}
                == {normalized_requirement(value) for value in project["dependencies"]},
                "wheel does not declare the required NumPy/SciPy dependencies")
        wheel = BytesParser().parsebytes(archive.read(f"{dist_info}/WHEEL"))
        require(wheel["Root-Is-Purelib"] == "false", "native extension must not be an optional pure wheel")
        require(not any(tag.endswith("-none-any") for tag in wheel.get_all("Tag", [])),
                "native wheel has a pure-Python compatibility tag")
        native = [name for name in files if "/" not in name and name.startswith("_noon_native.")
                  and name.endswith((".so", ".pyd"))]
        require(len(native) == 1 and archive.getinfo(native[0]).file_size > 0,
                "wheel is missing its mandatory _noon_native extension")
        expected = {f"{name}.py" for name in settings["tool"]["setuptools"]["py-modules"]}
        actual = {name for name in files if name.endswith(".py")}
        require(actual == expected,
                f"wheel Python source mismatch: missing={sorted(expected - actual)}, extra={sorted(actual - expected)}")
        for name in expected:
            require(archive.read(name) == (root / "web/python" / name).read_bytes(),
                    f"wheel contains stale Python source: {name}")
        entries = configparser.ConfigParser()
        entries.read_string(archive.read(f"{dist_info}/entry_points.txt").decode("utf8"))
        require(dict(entries["console_scripts"]) == project["scripts"], "installed runner entry point differs")
        notices = project["license-files"]
        require(set(metadata.get_all("License-File", [])) == set(notices), "wheel license metadata is incomplete")
        for name in notices:
            require(archive.read(f"{dist_info}/licenses/{name}") == (root / name).read_bytes(),
                    f"wheel is missing or changed a license/notice: {name}")
        native_payloads = []
        for name in sorted(files):
            if re.search(r"(?:\.so(?:\.\d+)*|\.dylib|\.dll|\.pyd|\.a|\.lib)$", name, re.IGNORECASE):
                payload = archive.read(name)
                native_payloads.append({"path": name, "bytes": len(payload),
                                        "sha256": hashlib.sha256(payload).hexdigest()})
        additional_libraries = [item["path"] for item in native_payloads if item["path"] not in native]
        license_paths = sorted(name for name in files if f"{dist_info}/licenses/" in name
                               or any(word in PurePosixPath(name).name.lower()
                                      for word in ("license", "notice", "copying")))
        return {"file": str(path.resolve()), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "version": metadata["Version"], "python_modules": len(expected),
                "native_extension": native[0], "tags": wheel.get_all("Tag", []),
                "native_payloads": native_payloads, "license_notice_files": license_paths,
                "additional_native_libraries": {"paths": additional_libraries,
                                                "notice_matching": "required before publication"}}


def sdist_files(path):
    with tarfile.open(path, "r:gz") as archive:
        members = archive.getmembers()
        check_file_names([member.name for member in members])
        roots = {PurePosixPath(member.name).parts[0] for member in members}
        require(len(roots) == 1, "sdist must have one top-level directory")
        result = {}
        for member in members:
            require(member.isdir() or member.isfile(), f"sdist contains a link or special file: {member.name}")
            if member.isfile():
                relative = str(PurePosixPath(member.name).relative_to(next(iter(roots))))
                result[relative] = archive.extractfile(member).read()
        return result


def validate_source_closure(files):
    workspace = tomllib.loads(files["Cargo.toml"].decode("utf8"))
    for member in workspace["workspace"]["members"]:
        require(f"{member}/Cargo.toml" in files, f"sdist omitted workspace member: {member}")
        manifest = tomllib.loads(files[f"{member}/Cargo.toml"].decode("utf8"))
        require(f"{member}/{manifest.get('lib', {}).get('path', 'src/lib.rs')}" in files,
                f"sdist omitted Rust library source: {member}")
    checked = 0
    for name, contents in files.items():
        if not name.endswith(".rs"):
            continue
        for included in INCLUDE.findall(contents.decode("utf8")):
            resolved = posixpath.normpath(posixpath.join(posixpath.dirname(name), included))
            require(resolved in files, f"sdist omitted compile-time resource: {name} -> {included}")
            checked += 1
    return checked


def validate_sdist(path, root=ROOT):
    settings = checked_configuration(root)
    files = sdist_files(path)
    required = {
        "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "pyproject.toml", "MANIFEST.in",
        "README.md", "web/python/examples/first_lesson.py",
        "scripts/check-python-package.py", "scripts/test_python_package.py",
        "scripts/ci-python-wheel-linux.sh",
        *settings["project"]["license-files"],
        *(f"web/python/{name}.py" for name in settings["tool"]["setuptools"]["py-modules"]),
    }
    require(required <= files.keys(), f"sdist omitted required files: {sorted(required - files.keys())}")
    for name in required:
        require(files[name] == (root / name).read_bytes(), f"sdist contains stale source: {name}")
    expected_python = {name for name in required if name.startswith("web/python/")}
    actual_python = {name for name in files if name.startswith("web/python/") and name.endswith(".py")}
    require(actual_python == expected_python,
            f"sdist contains development Python helpers: {sorted(actual_python - expected_python)}")
    included = validate_source_closure(files)
    provider = tomllib.loads(files["Cargo.lock"].decode("utf8"))
    provider_versions = {package["version"] for package in provider["package"] if package["name"] == "typst-assets"}
    require(provider_versions == {"0.15.1"}, "update retained typst-assets notices for the new locked version")
    return {"file": str(path.resolve()), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "files": len(files), "literal_rust_resources": included}


def run(command, *, cwd, environment, capture=False):
    print("+ " + " ".join(str(part) for part in command), file=sys.stderr, flush=True)
    result = subprocess.run([str(part) for part in command], cwd=cwd, env=environment,
                            text=True, stdout=subprocess.PIPE if capture else sys.stderr,
                            stderr=subprocess.PIPE if capture else None)
    if result.returncode:
        if capture:
            print(result.stdout, file=sys.stderr)
            print(result.stderr, file=sys.stderr)
        raise PackageError(f"command failed ({result.returncode}): {command[0]}")
    return result.stdout if capture else None


IMPORT_CHECK = r'''
import importlib.metadata, json, pathlib, sys
import noon, noon_native, _noon_native, numpy, scipy
from _manim_user_vectors import rotation_matrix, shoelace
prefix = pathlib.Path(sys.prefix).resolve()
for module in (noon, noon_native, _noon_native, numpy, scipy):
    assert pathlib.Path(module.__file__).resolve().is_relative_to(prefix), module.__file__
assert noon.__version__ == importlib.metadata.version(sys.argv[1])
assert "js" not in sys.modules and "pyodide" not in sys.modules
numpy.testing.assert_allclose(rotation_matrix(0.0, (0.0, 0.0, 1.0)), numpy.eye(3))
assert shoelace(numpy.array([[0., 0.], [1., 1.], [2., 0.]])) == 1.0
print(json.dumps({"version": noon.__version__, "native_extension": _noon_native.__file__,
                  "numpy": numpy.__version__, "scipy": scipy.__version__}))
'''


def installed_smoke(wheel=None, *, render=False, mp4=False, root=ROOT):
    environment = dict(os.environ)
    for name in ("PYTHONPATH", "PYTHONHOME", "VIRTUAL_ENV"):
        environment.pop(name, None)
    environment["PIP_DISABLE_PIP_VERSION_CHECK"] = "1"
    with tempfile.TemporaryDirectory(prefix="noon-installed-") as directory:
        outside = Path(directory).resolve()
        require(not outside.is_relative_to(root.resolve()), "temporary consumer must be outside the checkout")
        if wheel is None:
            python = Path(sys.executable)
            binaries = Path(sysconfig.get_path("scripts"))
        else:
            environment_path = outside / "venv"
            venv.EnvBuilder(with_pip=True).create(environment_path)
            binaries = environment_path / ("Scripts" if os.name == "nt" else "bin")
            python = binaries / ("python.exe" if os.name == "nt" else "python")
        runner = binaries / ("noon.exe" if os.name == "nt" else "noon")
        consumer = outside / "consumer"
        consumer.mkdir()
        if wheel is not None:
            run([python, "-I", "-m", "pip", "install", wheel.resolve()], cwd=consumer, environment=environment)
        run([python, "-I", "-m", "pip", "check"], cwd=consumer, environment=environment)
        imports = json.loads(run([python, "-I", "-c", IMPORT_CHECK, config(root)["project"]["name"]], cwd=consumer,
                                 environment=environment, capture=True))
        expected_version = f"Noon {source_version(root)}"
        for command in ([runner, "--version"], [python, "-I", "-m", "noon_native", "--version"]):
            require(run(command, cwd=consumer, environment=environment, capture=True).strip() == expected_version,
                    "installed runner reports the wrong version")
        source = consumer / "first_lesson.py"
        shutil.copyfile(root / "web/python/examples/first_lesson.py", source)
        run([runner, source.name, "--headless"], cwd=consumer, environment=environment)
        report = {"imports": imports, "console_script": True, "module_runner": True,
                  "headless_first_lesson": True, "outside_checkout": True}
        if render or mp4:
            snapshot = consumer / "lesson.png"
            run([runner, source.name, "--snapshot", snapshot, "--time", "1.75", "--width", "320", "--height", "180"],
                cwd=consumer, environment=environment)
            data = snapshot.read_bytes()
            require(data.startswith(b"\x89PNG\r\n\x1a\n") and data[12:16] == b"IHDR", "snapshot is not a PNG")
            require(struct.unpack(">II", data[16:24]) == (320, 180), "snapshot dimensions differ")
            report["snapshot"] = {"width": 320, "height": 180, "bytes": len(data)}
        if mp4:
            movie = consumer / "lesson.mp4"
            run([runner, source.name, "--mp4", movie, "--width", "320", "--height", "180",
                 "--fps", "30000/1001", "--start-frame", "15", "--frames", "3"], cwd=consumer, environment=environment)
            probe = json.loads(run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-count_frames",
                                   "-show_entries", "stream=width,height,avg_frame_rate,nb_read_frames",
                                   "-of", "json", movie], cwd=consumer, environment=environment, capture=True))
            stream, = probe["streams"]
            require((stream["width"], stream["height"], stream["nb_read_frames"], stream["avg_frame_rate"])
                    == (320, 180, "3", "30000/1001"), f"unexpected MP4 stream: {stream}")
            run(["ffmpeg", "-v", "error", "-i", movie, "-f", "null", "-"], cwd=consumer, environment=environment)
            report["mp4"] = stream
        return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("wheel", type=Path, nargs="?")
    parser.add_argument("--sdist", type=Path)
    parser.add_argument("--installed", action="store_true", help="test the current interpreter's installed package (for cibuildwheel)")
    parser.add_argument("--check-only", action="store_true", help="inspect archives without installing")
    parser.add_argument("--render", action="store_true", help="require a native-rendered PNG")
    parser.add_argument("--mp4", action="store_true", help="require and independently decode an MP4 (also renders PNG)")
    parser.add_argument("--report", type=Path, help="also save the JSON validation result")
    args = parser.parse_args()
    if not args.wheel and not args.sdist and not args.installed:
        parser.error("provide a wheel, --sdist, or --installed")
    if args.installed and (args.wheel or args.check_only):
        parser.error("--installed cannot be combined with a wheel or --check-only")
    if (args.render or args.mp4) and (args.check_only or not (args.wheel or args.installed)):
        parser.error("render/MP4 checks require an installed wheel")
    report = {}
    try:
        if args.sdist:
            report["sdist"] = validate_sdist(args.sdist)
        if args.wheel:
            report["wheel"] = validate_wheel(args.wheel)
            if not args.check_only:
                report["installed"] = installed_smoke(args.wheel, render=args.render, mp4=args.mp4)
        elif args.installed:
            report["installed"] = installed_smoke(render=args.render, mp4=args.mp4)
    except (PackageError, OSError, KeyError, tarfile.TarError, zipfile.BadZipFile) as error:
        raise SystemExit(f"Python package validation failed: {error}") from error
    output = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(output, encoding="utf8")
    print(output, end="")


if __name__ == "__main__":
    main()
