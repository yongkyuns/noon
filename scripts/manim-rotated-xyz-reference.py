"""Capture actual ManimCE rotated f64 XYZ fitting controls."""
import hashlib
import inspect
import json
from pathlib import Path
import sys
import manim as m
import numpy as np
from manim_rotated_xyz_observation import rotated_xyz_specs, observe_rotated_xyz

assert m.__version__ == "0.21.0"
line = m.Line(np.array([-.25, 1.5, 0.]), np.array([3.25, -.5, 0.]), stroke_width=0)
planar_initial = line.get_all_points().tolist()
planar_shift = [.1234567890123, -.2345678901234, 0.]
planar_pivot = [.2345678901234, -.3456789012345, 0.]
line.shift(np.array(planar_shift))
line.rotate(.37, about_point=np.zeros(3))
planar_before = line.get_all_points().tolist()
line.rescale_to_fit(7.1234567890123, 0, True, about_point=np.array(planar_pivot))
planar_control = dict(initialXYZ=planar_initial, shift=planar_shift, angle=.37,
                      pivot=planar_pivot, length=7.1234567890123,
                      beforeXYZ=planar_before, pointsXYZ=line.get_all_points().tolist())

Path(sys.argv[1]).write_text(json.dumps(dict(
    manimVersion=m.__version__,
    generatorSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    observerSha256=hashlib.sha256(Path(__file__).with_name("manim_rotated_xyz_observation.py").read_bytes()).hexdigest(),
    helperSha256=hashlib.sha256(Path(__file__).with_name("manim_depth_affine_observation.py").read_bytes()).hexdigest(),
    methodSourceSha256={name: hashlib.sha256(inspect.getsource(getattr(m.VMobject, name)).encode()).hexdigest()
                       for name in ("rotate", "stretch", "shift", "rescale_to_fit")},
    cases=[observe_rotated_xyz(m, spec) for spec in rotated_xyz_specs()],
    planarCommandControl=planar_control,
), indent=2) + "\n")
