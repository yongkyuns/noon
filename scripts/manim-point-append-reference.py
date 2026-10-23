"""Capture the actual pinned CE caller coercion/public-hook behavior."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import manim as m
from manim_point_append_observation import METHODS, SPECS, observe_point_append, observe_point_append_hook

def main():
    assert m.__version__ == "0.21.0"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    cases = [observe_point_append(m, spec, method, prefix) for method in METHODS for prefix in (0, 4) for spec in SPECS]
    hooks = [observe_point_append_hook(m, spec, method) for method in METHODS[1:] for spec in SPECS]
    record = {
        "manimVersion": m.__version__,
        "generatorSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "observerSha256": hashlib.sha256(Path(__file__).with_name("manim_point_append_observation.py").read_bytes()).hexdigest(),
        "methodSourceSha256": {name: hashlib.sha256(inspect.getsource(getattr(m.VMobject, name)).encode()).hexdigest() for name in METHODS},
        "cases": cases, "hookCases": hooks,
        "familyCases": [observe_point_append(m, spec, method, prefix, m.VGroup)
                        for method in METHODS for prefix in (0, 4) for spec in SPECS],
        "scope": "Pinned caller coercion, return/exception behavior, XYZ prefixes, unchanged inputs and public append hooks. Writable point arrays, sharing, dimensions and broader callback behavior remain separate open requirements; this fixture makes no support-completion claim.",
    }
    args.output.write_text(json.dumps(record, indent=2) + "\n")

if __name__ == "__main__":
    main()
