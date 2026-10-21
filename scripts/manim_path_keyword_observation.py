"""Public keyword calls shared by pinned ManimCE and Noon browser controls."""

PATH_KEYWORD_METHODS = (
    "append_points", "add_cubic_bezier_curve_to", "add_quadratic_bezier_curve_to",
    "append_vectorized_mobject", "move_to",
)


def observe_path_keyword(module, method):
    import numpy as np

    points = np.array([[0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]])
    value = module.VMobject()
    if method == "append_points":
        keywords = {"new_points": points}
    elif method == "add_cubic_bezier_curve_to":
        value.start_new_path(points[0])
        keywords = {"handle1": points[1], "handle2": points[2], "anchor": points[3]}
    elif method == "add_quadratic_bezier_curve_to":
        value.start_new_path(points[0])
        keywords = {"handle": points[1], "anchor": points[3]}
    elif method == "append_vectorized_mobject":
        keywords = {"vectorized_mobject": module.VMobject().set_points(points)}
    elif method == "move_to":
        value = module.Square(1)
        keywords = {"point_or_mobject": np.array([3., 2., 0.])}
    else:
        raise ValueError(f"Unknown path keyword control: {method}")
    result = getattr(value, method)(**keywords)
    # These common 2D controls compare public coordinate values. They do not
    # qualify NumPy container shape, identity, writable views or raw aliases.
    return {"method": method, "keywords": list(keywords), "returnsSelf": result is value,
            "pointsXY": [[float(p[0]), float(p[1])] for p in value.get_all_points()],
            "centerXY": [float(value.get_center()[i]) for i in (0, 1)]}


QUADRATIC_PRECISION_SPECS = [
    {"id": "fractional-singleton", "prefix": [[.123456789012345, .234567890123456, 0.]],
     "control": [.123456789012345, .987654321098765, 0.],
     "anchor": [1.23456789012345, -.456789012345678, 0.]},
    {"id": "fractional-complete-prefix",
     "prefix": [[.123456789012345, .234567890123456, 0.],
                [.345678901234567, .456789012345678, 0.],
                [.567890123456789, .67890123456789, 0.],
                [.789012345678901, .890123456789012, 0.]],
     "control": [.123456789012345, .987654321098765, 0.],
     "anchor": [1.23456789012345, -.456789012345678, 0.]},
]


def observe_quadratic_precision(module, spec):
    import numpy as np

    value = module.VMobject().set_points(np.array(spec["prefix"]))
    result = value.add_quadratic_bezier_curve_to(
        handle=np.array(spec["control"]), anchor=np.array(spec["anchor"]))
    return {"spec": spec, "returnsSelf": result is value,
            "pointsXY": [[float(p[0]), float(p[1])] for p in value.get_all_points()]}
