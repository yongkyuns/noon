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

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("dev", "release"), default="release")
    parser.add_argument("--output", type=Path, default=ROOT / "build/python")
    parser.add_argument("--export-video", action="store_true",
                        help="Include the native GPU and installed-FFmpeg export adapter")
    args = parser.parse_args()
    output = args.output.resolve()
    command = ["cargo", "build", "-p", "noon-python"]
    if args.profile == "release":
        command.append("--release")
    if args.export_video:
        command += ["--features", "export"]
    environment = dict(os.environ, PYO3_PYTHON=sys.executable)
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
    shutil.copy2(built, output / ("_noon_native" + suffix))
    for source in (ROOT / "web/python").glob("*.py"):
        if not source.name.startswith("test_"):
            shutil.copy2(source, output / source.name)
    # No optional fonts, text compiler, renderer, JS or WASM distribution is copied.
    print(f"Native Python profile built at {output}")


if __name__ == "__main__":
    main()
