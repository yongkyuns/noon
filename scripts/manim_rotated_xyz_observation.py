"""Rotated full-coordinate fits, including command-only straight lines."""
import numpy as np
from manim_depth_affine_observation import POINTS


def rotated_xyz_specs():
    return [dict(shape=shape, family=family, angle=angle, dimension=dimension,
                 stretch=stretch, pivot=pivot, length=length)
            for shape in ("path4", "path8", "line") for family in (False, True)
            for angle in (.37, float(np.pi / 2)) for dimension in (0, 1, 2)
            for stretch in (False, True) for pivot in ("center", "point", "edge")
            for length in (0., 7.1234567890123, -7.1234567890123)]


def observe_rotated_xyz(m, spec):
    if spec["shape"] == "line":
        leaves = [m.Line(np.array([-.25, 1.5, 0.]), np.array([3.25, -.5, 0.]), stroke_width=0)]
        if spec["family"]:
            leaves.append(m.Line(np.array([2.5, 1.25, 0.]), np.array([4.25, -1.75, 0.]), stroke_width=0))
    else:
        leaves = [m.VMobject(stroke_width=0).set_points(POINTS[:int(spec["shape"][4:])].copy())]
        if spec["family"]:
            leaves.append(m.VMobject(stroke_width=0).set_points(POINTS[4:].copy()))
    target = (m.VGroup(leaves[0], m.VGroup(leaves[0], leaves[1])) if spec["family"] else leaves[0])
    initial = [leaf.get_all_points().tolist() for leaf in leaves]
    target.stretch(2., dim=2, about_point=np.zeros(3))
    target.shift(np.array([1.5, -2.5, 3.125]))
    target.rotate(spec["angle"], about_point=np.array([.25, -.75, 0.]))
    before = [leaf.get_all_points().tolist() for leaf in leaves]
    kwargs = ({} if spec["pivot"] == "center" else
              {"about_point": np.array([.1234567890123, -.2345678901234, 4.25])}
              if spec["pivot"] == "point" else {"about_edge": np.array([1., -1., 1.])})
    try:
        result = target.rescale_to_fit(spec["length"], spec["dimension"], spec["stretch"], **kwargs)
        outcome = dict(kind="returned", returnsSelf=result is target)
    except Exception as error:
        outcome = dict(kind="raised", type=type(error).__name__, message=str(error))
    return dict(spec=spec, initialXYZ=initial, beforeXYZ=before, outcome=outcome,
                pointsXYZ=[leaf.get_all_points().tolist() for leaf in leaves],
                lengths=[float(target.length_over_dim(d)) for d in range(3)])
