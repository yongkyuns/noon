"""Manim-shaped supported render flags; output semantics are resolved in Rust.

Examples: python -m noon -qh scene.py Demo --fps 60 -o demo.mp4
          python -m noon render scene.py Demo -r 640,360 --format png -o frames

This first CLI profile selects one Scene. PNG remains a Noon sequence bundle.
Config files, animation-number ranges, alpha/video formats other than MP4 and
last-still output are explicitly unsupported, not silently approximated.
"""
from __future__ import annotations
import argparse
import asyncio
from pathlib import Path
import sys


class Unsupported(argparse.Action):
    def __call__(self, parser, namespace, values, option_string=None):
        parser.error(f"{option_string} is not supported by this export profile")


def parse_args(argv=None):
    args = list(sys.argv[1:] if argv is None else argv)
    execution_only = bool(args and args[0] == "run")
    if args and args[0] in ("render", "run"):
        args.pop(0)
    parser = argparse.ArgumentParser(prog="noon", description=__doc__, allow_abbrev=False)
    parser.add_argument("source", type=Path)
    parser.add_argument("scene", nargs="?", help="Scene class name in the source file")
    parser.add_argument("--portable-source", action="store_true")
    if execution_only:
        parser.add_argument("--sample-hz", type=float, default=60.0)
    else:
        parser.add_argument("-q", "--quality", help="Manim quality l/m/h/p/k")
        parser.add_argument("-r", "--resolution", help="Pixel width,height")
        parser.add_argument("--fps", "--frame_rate", dest="frame_rate", help="Numeric FPS or explicit P/Q")
        parser.add_argument("-o", "--output_file", "--output", dest="output", type=Path)
        profile = parser.add_mutually_exclusive_group()
        profile.add_argument("--format")
        profile.add_argument("--png", dest="format", action="store_const", const="png")
        parser.add_argument("--width", type=int, help="Noon explicit pixel-width override")
        parser.add_argument("--height", type=int, help="Noon explicit pixel-height override")
        parser.add_argument("--max-frames", type=int, default=108000, help="Failure safety cap, not truncation")
        parser.add_argument("--start-frame", type=int, default=0, help="Noon frame-grid crop, NOT Manim -n")
        parser.add_argument("--final-hold", type=float, default=0.0, help="Explicit frozen terminal hold")
        parser.add_argument("-p", "--preview", action="store_true", help="Open the finalized MP4")
        parser.add_argument("--overwrite", action="store_true")
        parser.add_argument("--fallback-adapter", action="store_true")
        parser.add_argument("--ffmpeg", type=Path)
        # These names must not become aliases for unrelated frame/profile values.
        for flags in (("-n", "--from_animation_number"), ("--renderer",), ("--config_file",),
                      ("--media_dir",), ("--background_color",), ("--background_opacity",)):
            parser.add_argument(*flags, action=Unsupported)
        for flags in (("-a", "--write_all"), ("-s", "--save_last_frame"),
                      ("-t", "--transparent"), ("--save_sections",)):
            parser.add_argument(*flags, action=Unsupported, nargs=0)
    options = parser.parse_args(args)
    options.execution_only = execution_only
    if options.scene is not None and not options.scene.isidentifier():
        parser.error("scene name must be a Python class identifier")
    return parser, options


def preview_file(path):
    """Native OS integration only; never used to capture or pace frames."""
    import os
    import subprocess
    path = str(Path(path).resolve())
    if sys.platform == "win32":
        os.startfile(path)
    else:
        subprocess.run(["open" if sys.platform == "darwin" else "xdg-open", path], check=True, timeout=30)


def main(argv=None):
    parser, args = parse_args(argv)
    # Parsing/help does not import the engine. Capability/option validation happens
    # before reading/executing source or creating its output files.
    try:
        from _noon_native_host import run_source, export_source, close_scene
        if args.execution_only:
            async def execute():
                scene = await run_source(args.source.read_text(encoding="utf8"),
                                         filename=str(args.source), scene_name=args.scene,
                                         portable=args.portable_source, sample_hz=args.sample_hz)
                close_scene(scene)
            asyncio.run(execute())
            return 0
        from _noon_render_options import resolve_render_options
        try:
            resolved = resolve_render_options(quality=args.quality, resolution=args.resolution,
                                              frame_rate=args.frame_rate, format=args.format,
                                              pixel_width=args.width, pixel_height=args.height)
        except (ValueError, TypeError, NotImplementedError) as error:
            parser.error(str(error))
        if args.preview and resolved.format != "mp4":
            parser.error("preview is currently supported only for finalized MP4 output")
        if args.max_frames <= 0 or args.start_frame < 0:
            parser.error("max-frames must be positive and start-frame must be nonnegative")
        # Destination placement is a CLI/platform concern, not scene semantics.
        # Unlike Manim's configurable media tree, this explicit initial profile
        # defaults to a local SceneName.mp4 (or a SceneName PNG bundle).
        output = args.output or Path(args.scene or args.source.stem)
        if resolved.format == "mp4" and not output.suffix:
            output = output.with_suffix(".mp4")
        async def render():
            return await export_source(
                args.source.read_text(encoding="utf8"), output,
                filename=str(args.source), scene_name=args.scene,
                portable=args.portable_source,
                width=resolved.width, height=resolved.height, fps=resolved.fps,
                format=resolved.format, max_frames=args.max_frames,
                start_frame=args.start_frame, final_hold=args.final_hold,
                overwrite=args.overwrite, fallback=args.fallback_adapter, ffmpeg=args.ffmpeg,
            )
        result = asyncio.run(render())
    except (ImportError, OSError, ValueError, TypeError, RuntimeError) as error:
        parser.exit(1, f"noon: {error}\n")
    print(f"Finalized {result['frames']} frames: {result['path']}")
    if args.preview:
        try:
            preview_file(result["path"])
        except Exception as error:
            parser.exit(1, f"noon: output is finalized, but preview failed: {error}\n")
    return 0
