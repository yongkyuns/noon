"""Original-source public polynomial easing observations for explicit user data."""
import argparse
import json
from pathlib import Path
import sys

from manim_easing_oracle import easing_observations, qualify_easings

NAMES = tuple(f"ease_{direction}_{degree}" for degree in ("quad", "cubic", "quart", "quint")
              for direction in ("in", "out", "in_out"))


def observations(namespace):
    return easing_observations(namespace, NAMES)


def qualify(source_root, *, installed_manim=False):
    return qualify_easings(source_root, NAMES, installed_manim=installed_manim, module_only=False)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    result = qualify(args.source_root, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result["errors"]))
