"""Capture pinned ManimCE dimension fitting with nonzero Z and family aliases."""
import hashlib
import inspect
import json
from pathlib import Path
import sys
import manim as m
from manim_fitting_xyz_observation import fitting_xyz_specs, observe_fitting_xyz

assert m.__version__ == "0.21.0"
Path(sys.argv[1]).write_text(json.dumps(dict(
    manimVersion=m.__version__,
    generatorSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    observerSha256=hashlib.sha256(Path(__file__).with_name("manim_fitting_xyz_observation.py").read_bytes()).hexdigest(),
    helperSha256=hashlib.sha256(Path(__file__).with_name("manim_depth_affine_observation.py").read_bytes()).hexdigest(),
    methodSourceSha256={name: hashlib.sha256(inspect.getsource(getattr(m.VMobject, name)).encode()).hexdigest()
                       for name in ("length_over_dim", "rescale_to_fit", "match_dim_size", "scale")},
    cases=[observe_fitting_xyz(m, spec) for spec in fitting_xyz_specs()],
), indent=2) + "\n")
