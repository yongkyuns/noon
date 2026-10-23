"""Capture deterministic ManimCE depth affine and family alias behavior."""
import hashlib
import inspect
import json
from pathlib import Path
import sys
import manim as m
from manim_depth_affine_observation import depth_affine_specs, observe_depth_affine

assert m.__version__ == "0.21.0"
Path(sys.argv[1]).write_text(json.dumps(dict(
    manimVersion=m.__version__,
    generatorSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    observerSha256=hashlib.sha256(Path(__file__).with_name("manim_depth_affine_observation.py").read_bytes()).hexdigest(),
    methodSourceSha256={name: hashlib.sha256(inspect.getsource(getattr(m.VMobject, name)).encode()).hexdigest()
                       for name in ("shift", "stretch", "get_critical_point", "get_points_defining_boundary")},
    cases=[observe_depth_affine(m, spec) for spec in depth_affine_specs()],
), indent=2) + "\n")
