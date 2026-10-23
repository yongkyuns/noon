"""Full-coordinate depth affine controls, including shared family aliases."""
import numpy as np

POINTS = np.array([
    [.125, .25, 1.125], [.5, .75, 40.25],
    [1., 1.5, -30.5], [2., 3., 7.375],
    [3., -2., 8.125], [4., -1., 60.25],
    [5., 0., -50.5], [6., 1., 14.375],
], dtype=np.float64)
PIVOTS = ("center", "point", "min", "max", "zero_edge")


def depth_affine_specs():
    return [dict(rows=rows, family=family, pivot=pivot, factor=factor)
            for rows, family in [(0, False), (1, False), (4, False), (8, False), (4, True)]
            for pivot in PIVOTS for factor in (2., -.5, 0.)]


def observe_depth_affine(m, spec):
    leaves = [m.VMobject().set_points(POINTS[:spec["rows"]].copy())]
    if spec["family"]:
        leaves.append(m.VMobject().set_points(POINTS[4:].copy()))
        target = m.VGroup(leaves[0], m.VGroup(leaves[0], leaves[1]))
    else:
        target = leaves[0]
    prefix = [leaf.get_all_points().tolist() for leaf in leaves]
    target.shift(np.array([1.5, -2.5, 3.125]))
    pivot = spec["pivot"]
    kwargs = ({} if pivot == "center" else
              {"about_point": np.array([8., -9., 4.25])} if pivot == "point" else
              {"about_edge": np.array([0., 0., -1. if pivot == "min" else
                                        1. if pivot == "max" else 0.])})
    result = target.stretch(spec["factor"], dim=2, **kwargs)
    return dict(spec=spec, prefixXYZ=prefix, returnsSelf=result is target,
                pointsXYZ=[leaf.get_all_points().tolist() for leaf in leaves])
