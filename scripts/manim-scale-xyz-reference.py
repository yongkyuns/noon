"""Capture ManimCE v0.21.0 generic full-coordinate scale observations."""
import hashlib
import inspect
import json
from pathlib import Path
import sys
import manim as m
from manim_scale_xyz_observation import scale_xyz_specs, observe_scale_xyz

assert m.__version__ == "0.21.0"
Path(sys.argv[1]).write_text(json.dumps(dict(
    manimVersion=m.__version__,
    generatorSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    observerSha256=hashlib.sha256(Path(__file__).with_name("manim_scale_xyz_observation.py").read_bytes()).hexdigest(),
    helperSha256=hashlib.sha256(Path(__file__).with_name("manim_depth_affine_observation.py").read_bytes()).hexdigest(),
    methodSourceSha256={name: hashlib.sha256(inspect.getsource(method).encode()).hexdigest()
                       for name, method in (("Mobject.scale", m.Mobject.scale),
                                            ("VMobject.scale", m.VMobject.scale),
                                            ("Mobject.apply_points_function_about_point", m.Mobject.apply_points_function_about_point),
                                            ("Mobject.get_critical_point", m.Mobject.get_critical_point),
                                            ("VMobject.get_points_defining_boundary", m.VMobject.get_points_defining_boundary))},
    cases=[observe_scale_xyz(m, spec) for spec in scale_xyz_specs()],
), indent=2) + "\n")
