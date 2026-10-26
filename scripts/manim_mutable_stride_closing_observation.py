"""Public mutable-stride closing observations, independently pinned to CE."""
import numpy as np


def observe_mutable_stride_closing(module, point_count):
    value = module.VMobject()
    value.n_points_per_cubic_curve = 5
    value.set_points(np.array([[float(i), -float(i), 0.] for i in range(point_count)]))
    before = value.get_all_points().copy()
    subpaths = value.get_subpaths()
    result = value.close_path()
    return {
        'pointCount': point_count,
        'stride': value.n_points_per_cubic_curve,
        'samples': list(value._bezier_t_values),
        'inputPointsXY': [[float(p[0]), float(p[1])] for p in before],
        'lastSubpathStartXY': [float(v) for v in subpaths[-1][0][:2]],
        'returnsSelf': result is value,
        'pointsXY': [[float(p[0]), float(p[1])] for p in value.get_all_points()],
    }
