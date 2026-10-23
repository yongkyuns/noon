"""Caller-owned inputs and observations for full-coordinate scalar path edits."""
import numpy as np

PREFIX = np.array([
    [.123456789012345, .234567890123456, .345678901234567],
    [.345678901234567, .456789012345678, .567890123456789],
    [.567890123456789, .678901234567890, .789012345678901],
    [.789012345678901, .890123456789012, .901234567890123],
    [1.123456789012345, .234567890123456, 1.345678901234567],
    [1.345678901234567, .456789012345678, 1.567890123456789],
    [1.567890123456789, .678901234567890, 1.789012345678901],
    [1.789012345678901, .890123456789012, 1.901234567890123],
], dtype=np.float64)
INPUTS = np.array([
    [.123456789012345, .987654321098765, -.234567890123456],
    [.345678901234567, -.876543210987654, -.456789012345678],
    [1.234567890123450, -.456789012345678, 2.345678901234560],
    [2.345678901234560, 1.234567890123450, 3.456789012345670],
], dtype=np.float64)
METHODS = (
    "start_new_path", "add_line_to", "add_cubic_bezier_curve_to",
    "add_cubic_bezier_curve", "set_points_as_corners",
    "add_points_as_corners", "close_path", "add_quadratic_bezier_curve_to",
)


def scalar_xyz_specs():
    return [
        dict(method=method, prefixRows=count, stride=stride, planarInput=planar, posed=posed)
        for method in METHODS
        for count in (1, 4, 5, 8)
        for stride in (4, 5)
        for planar in (False, True)
        for posed in (False, True)
        # CE's mutable-stride corners write uninitialized slots; the original
        # requirement remains open and is deliberately not sampled as an oracle.
        if not (method in ("set_points_as_corners", "add_points_as_corners") and stride != 4)
    ] + [dict(method="close_path", prefixRows=4, stride=4, planarInput=False,
              posed=False, coincidentXY=True)]


def observe_scalar_xyz(m, spec):
    obj = m.VMobject()
    prefix = PREFIX[:spec["prefixRows"]].copy()
    if spec.get("coincidentXY"):
        prefix[-1, :2] = prefix[0, :2]
    obj.set_points(prefix)
    if spec["posed"]:
        obj.stretch(2., dim=2, about_point=np.zeros(3))
        obj.shift(np.array([1.5, -2.5, 3.]))
    obj.n_points_per_cubic_curve = spec["stride"]
    incoming = INPUTS.copy()
    if spec["planarInput"]:
        incoming[:, 2] = 0.
    before = incoming.copy()
    before_points = obj.get_all_points().tolist()
    method = spec["method"]
    try:
        if method == "start_new_path" or method == "add_line_to":
            result = getattr(obj, method)(incoming[2])
        elif method == "add_cubic_bezier_curve_to":
            result = obj.add_cubic_bezier_curve_to(*incoming[:3])
        elif method == "add_cubic_bezier_curve":
            result = obj.add_cubic_bezier_curve(*incoming)
        elif method in ("set_points_as_corners", "add_points_as_corners"):
            result = getattr(obj, method)(incoming[2:])
        elif method == "add_quadratic_bezier_curve_to":
            result = obj.add_quadratic_bezier_curve_to(incoming[0], incoming[2])
        else:
            result = obj.close_path()
        outcome = dict(kind="returned", returnsSelf=result is obj)
    except Exception as error:
        outcome = dict(kind="raised", type=type(error).__name__, message=str(error))
    return dict(spec=spec, prefixXYZ=prefix.tolist(), inputXYZ=incoming.tolist(),
                beforeXYZ=before_points, outcome=outcome, pointsXYZ=obj.get_all_points().tolist(),
                inputUnchanged=bool(np.array_equal(incoming, before)))
