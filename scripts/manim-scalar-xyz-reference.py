"""Capture pinned ManimCE scalar edits with genuine nonzero Z coordinates."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path

import manim as m
from manim_scalar_xyz_observation import METHODS, observe_scalar_xyz, scalar_xyz_specs


def main():
    assert m.__version__ == "0.21.0"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    cases = [observe_scalar_xyz(m, spec) for spec in scalar_xyz_specs()]
    args.output.write_text(json.dumps(dict(
        manimVersion=m.__version__,
        generatorSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        observerSha256=hashlib.sha256(Path(__file__).with_name("manim_scalar_xyz_observation.py").read_bytes()).hexdigest(),
        methodSourceSha256={name: hashlib.sha256(inspect.getsource(getattr(m.VMobject, name)).encode()).hexdigest() for name in METHODS},
        cases=cases,
        scope="Full XYZ scalar path edits, prefix lengths, stride classification and world-coordinate poses. Original CE requirements and prior controls remain unchanged; no support-completion claim.",
    ), indent=2) + "\n")


if __name__ == "__main__":
    main()
