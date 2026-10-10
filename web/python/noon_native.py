"""Native CPython entry point for Noon's finite, sampled authoring profile.

The scene source is shared with Pyodide. This host is renderer-free and uses
explicit logical samples, not a native realtime window. Optional text/resources,
spatial/family construction and callback structural/content producers are outside
this binding's initial profile; they are not silently replaced or emulated.
"""
from __future__ import annotations

from _noon_native_host import run_scene, run_source, close_scene


def main():
    import argparse
    import asyncio
    from pathlib import Path
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--sample-hz", type=float, default=60.0)
    parser.add_argument("--portable-source", action="store_true")
    args = parser.parse_args()
    async def run():
        scene = await run_source(args.source.read_text(encoding="utf8"),
                                 sample_hz=args.sample_hz, portable=args.portable_source,
                                 filename=str(args.source))
        try:
            return scene.time
        finally:
            close_scene(scene)
    asyncio.run(run())


if __name__ == "__main__":
    main()
