"""Run Noon Python scenes with the shared Rust engine, without a browser.

Preparation executes source once; preview replays bounded deterministic Rust
history in the native window. Arbitrary callbacks can be sampled with --headless;
realtime callback preview remains an explicit unsupported profile.
"""
from __future__ import annotations

from _noon_native_host import run_scene, run_source, close_scene


def prepare_view(scene):
    """Consume a completed scene into the existing native renderer/window host."""
    from _manim_scene import _context
    try:
        return _context(scene).prepare_view()
    finally:
        close_scene(scene)


def main():
    import argparse
    import asyncio
    from pathlib import Path
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--scene", dest="scene_name", help="select a Scene class from a source with multiple scenes")
    parser.add_argument("--sample-hz", type=float, default=60.0)
    parser.add_argument("--portable-source", action="store_true")
    parser.add_argument("--headless", action="store_true", help="execute and validate without a GPU or window")
    parser.add_argument("--snapshot", type=Path, help="write one native-rendered PNG instead of opening a window")
    parser.add_argument("--time", type=float, default=0.0, help="snapshot time in seconds")
    parser.add_argument("--width", type=int, default=960)
    parser.add_argument("--height", type=int, default=540)
    args = parser.parse_args()
    if args.headless and args.snapshot:
        parser.error("--headless and --snapshot cannot be combined")
    async def run():
        scene = await run_source(args.source.read_text(encoding="utf8"),
                                 sample_hz=args.sample_hz, portable=args.portable_source,
                                 filename=str(args.source), retain_history=not args.headless,
                                 prepare_shaders=not args.headless, scene_name=args.scene_name)
        if args.headless:
            close_scene(scene)
            return None
        return prepare_view(scene)
    # Match script import lookup at the CLI boundary. Library invocations retain
    # their caller's import path, including concurrently executing sources.
    import sys
    previous_path = sys.path[:]
    try:
        sys.path.insert(0, str(args.source.resolve().parent))
        view = asyncio.run(run())
    finally:
        sys.path[:] = previous_path
    if view is not None:
        try:
            if args.snapshot:
                view.snapshot(args.snapshot, time=args.time, width=args.width, height=args.height)
            else:
                view.show(title=args.source.stem, width=args.width, height=args.height)
        finally:
            view.close()


if __name__ == "__main__":
    main()
