#!/usr/bin/env python3
"""Build the optional native extension beside the unchanged Python frontend.

Usage: python scripts/build-native-python.py --profile release --output build/python
Then: PYTHONPATH=build/python python -m noon_native path/to/scene.py
No Python engine, JavaScript shim, or source rewriting is generated here.
"""
from __future__ import annotations
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import sysconfig
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def publish_extension(built: Path, destination: Path) -> None:
    # Replacing a mapped Mach-O image in place invalidates its code signature.
    # A fresh inode also lets existing interpreters retain their original image.
    with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as file:
        temporary = Path(file.name)
    try:
        shutil.copy2(built, temporary)
        # Cargo can exit successfully after a failed post-link strip. Import in a
        # fresh process before publication so a damaged image cannot replace the
        # working extension, and never map the candidate into this build process.
        subprocess.run([sys.executable, "-I", "-c",
                        "import importlib.util, sys; "
                        "from importlib.machinery import ExtensionFileLoader; "
                        "loader = ExtensionFileLoader('_noon_native', sys.argv[1]); "
                        "spec = importlib.util.spec_from_loader('_noon_native', loader); "
                        "module = importlib.util.module_from_spec(spec); "
                        "loader.exec_module(module)", str(temporary)], check=True)
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def main() -> None:
    if sys.version_info < (3, 11):
        raise SystemExit("Noon requires CPython 3.11 or newer; run this script with that interpreter")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("dev", "release"), default="release")
    parser.add_argument("--minimal", action="store_true", help="omit native window, GPU capture and optional resources")
    parser.add_argument("--output", type=Path, default=ROOT / "build/python")
    args = parser.parse_args()
    output = args.output.resolve()
    command = ["cargo", "build", "-p", "noon-python"]
    if args.minimal:
        command.append("--no-default-features")
    if args.profile == "release":
        command.append("--release")
    environment = dict(os.environ, PYO3_PYTHON=sys.executable, PYO3_BUILD_EXTENSION_MODULE="1")
    subprocess.run(command, cwd=ROOT, env=environment, check=True)
    target = Path(environment.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    profile = "release" if args.profile == "release" else "debug"
    library = ("_noon_native.dll" if sys.platform == "win32" else
               "lib_noon_native.dylib" if sys.platform == "darwin" else "lib_noon_native.so")
    built = target / profile / library
    if not built.is_file():
        raise FileNotFoundError(f"Cargo did not produce {built}")
    output.mkdir(parents=True, exist_ok=True)
    suffix = sysconfig.get_config_var("EXT_SUFFIX")
    if not isinstance(suffix, str):
        raise RuntimeError("CPython did not report its extension suffix")
    publish_extension(built, output / ("_noon_native" + suffix))
    for source in (ROOT / "web/python").glob("*.py"):
        if not source.name.startswith("test_"):
            shutil.copy2(source, output / source.name)
    # Engine/resource support is compiled into the extension, never supplied by a browser.
    print(f"Native Python profile built at {output}")


if __name__ == "__main__":
    main()
