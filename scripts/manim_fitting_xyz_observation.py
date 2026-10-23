"""Full-coordinate dimension fitting and query controls from actual ManimCE."""
import numpy as np
from manim_depth_affine_observation import POINTS


def fitting_xyz_specs():
    return [dict(rows=rows, family=family, dimension=dimension, stretch=stretch,
                 pivot=pivot, length=length)
            for rows in (0, 1, 4, 5, 8) for family in (False, True)
            for dimension in (0, 1, 2) for stretch in (False, True)
            for pivot in ("center", "point", "min", "max")
            for length in (0., 7., -7.)]


def observe_fitting_xyz(m, spec):
    leaves = [m.VMobject().set_points(POINTS[:spec["rows"]].copy())]
    if spec["family"]:
        leaves.append(m.VMobject().set_points(POINTS[4:].copy()))
        target = m.VGroup(leaves[0], m.VGroup(leaves[0], leaves[1]))
    else:
        target = leaves[0]
    target.shift(np.array([1.5, -2.5, 3.125]))
    prefix = [leaf.get_all_points().tolist() for leaf in leaves]
    before = dict(depth=float(target.depth), lengths=[float(target.length_over_dim(d)) for d in range(3)])
    pivot = spec["pivot"]
    kwargs = ({} if pivot == "center" else
              {"about_point": np.array([8., -9., 4.25])} if pivot == "point" else
              {"about_edge": np.array([0., 0., -1. if pivot == "min" else 1.])})
    try:
        result = target.rescale_to_fit(spec["length"], spec["dimension"], spec["stretch"], **kwargs)
        outcome = dict(kind="returned", returnsSelf=result is target)
    except Exception as error:
        outcome = dict(kind="raised", type=type(error).__name__, message=str(error))
    return dict(spec=spec, prefixXYZ=prefix, before=before, outcome=outcome,
                after=dict(depth=float(target.depth), lengths=[float(target.length_over_dim(d)) for d in range(3)]),
                pointsXYZ=[leaf.get_all_points().tolist() for leaf in leaves])
