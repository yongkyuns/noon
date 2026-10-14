"""Original-source public sine easing observations for explicit user data."""
import argparse
from functools import wraps
import hashlib
import json
from pathlib import Path
import sys

import numpy as np
from pinned_python_helpers import load_helpers
from manim_easing_oracle import easing_observations

NAMES = ("ease_in_sine", "ease_out_sine", "ease_in_out_sine")


def observations(namespace):
    return easing_observations(namespace, NAMES)


def qualify(source_root, *, installed_manim=False):
    source = source_root / "manim/utils/rate_functions.py"
    original = load_helpers(source, set(NAMES) | {"unit_interval"}, {"wraps": wraps, "np": np})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_rate_functions as noon
    import noon as public
    errors = [] if observations(vars(noon)) == expected else ["sine easing observations differ"]
    for name in NAMES:
        if getattr(public.rate_functions, name) is not getattr(noon, name) or noon.easing_from_rate_func(getattr(noon, name)) != name:
            errors.append(f"public easing identity {name} differs")
    if installed_manim:
        from manim.utils import rate_functions
        if Path(rate_functions.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed complete rate module differs from pin")
        if observations(vars(rate_functions)) != expected:
            errors.append("installed sine easing observations differ")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "functions": len(NAMES), "observations": len(expected),
            "installed_module_checked": installed_manim, "errors": errors}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    result = qualify(args.source_root, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result["errors"]))
