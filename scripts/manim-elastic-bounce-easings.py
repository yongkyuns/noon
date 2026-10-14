"""Original-source public elastic/bounce easing observations for explicit user data."""
import argparse
import json
from pathlib import Path
import sys

from manim_easing_oracle import easing_observations, qualify_easings

NAMES = ('ease_in_elastic', 'ease_out_elastic', 'ease_in_out_elastic', 'ease_in_bounce', 'ease_out_bounce', 'ease_in_out_bounce')


def observations(namespace):
    return easing_observations(namespace, NAMES)


def qualify(source_root, *, installed_manim=False):
    return qualify_easings(source_root, NAMES, installed_manim=installed_manim, module_only=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    result = qualify(args.source_root, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result["errors"]))
