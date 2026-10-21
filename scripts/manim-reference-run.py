#!/usr/bin/env python3
"""Run a pinned Manim reference with explicit, process-local font registration."""
import json
import logging
import os
from pathlib import Path
import runpy
import sys


class ExactFontFilter(logging.Filter):
    def filter(self, record):
        message = record.getMessage()
        if message.startswith("Font ") and " not in " in message:
            raise RuntimeError("Manim reference requested an unavailable font: " + message)
        return True


def register_fonts():
    fonts = json.loads(os.environ.get("NOON_MANIM_REFERENCE_FONTS", "[]"))
    if not isinstance(fonts, list) or any(not isinstance(path, str) for path in fonts):
        raise ValueError("NOON_MANIM_REFERENCE_FONTS must be a JSON array of font paths")
    if fonts:
        import manimpango
        for font in fonts:
            path = Path(font).resolve(strict=True)
            if not path.is_file() or not manimpango.register_font(str(path)):
                raise RuntimeError(f"could not register reference font: {path}")
    from manim import logger
    logger.addFilter(ExactFontFilter())
    family = os.environ.get("NOON_MANIM_REFERENCE_TEXT_FONT")
    if family is not None:
        if not family or family != family.strip():
            raise ValueError("NOON_MANIM_REFERENCE_TEXT_FONT must name an explicit font family")
        import manimpango
        if family not in manimpango.list_fonts():
            raise RuntimeError(f"reference default font is unavailable: {family}")
        from manim import MarkupText, Text
        # Process-local reference configuration, shared by semantic and PNG runs.
        # Original scene classes and explicit per-object font arguments stay intact.
        Text.set_default(font=family)
        MarkupText.set_default(font=family)


def main():
    register_fonts()
    arguments = sys.argv[1:]
    if not arguments:
        raise ValueError("expected -m module or a Python script")
    if arguments[0] == "-m":
        module, *arguments = arguments[1:]
        sys.argv = [module, *arguments]
        runpy.run_module(module, run_name="__main__", alter_sys=True)
    else:
        script, *arguments = arguments
        sys.argv = [script, *arguments]
        runpy.run_path(script, run_name="__main__")


if __name__ == "__main__":
    main()
