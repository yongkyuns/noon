"""Common public point edits retain caller coordinates and prior raw prefixes."""
import numpy as np

SCALAR_PATH_PRECISION_METHODS = (
    'set_points', 'append_points', 'start_new_path', 'add_line_to',
    'add_quadratic_bezier_curve_to', 'add_cubic_bezier_curve_to',
    'add_cubic_bezier_curve', 'add_points_as_corners', 'set_points_as_corners', 'close_path',
)
PREFIX = np.array([[.123456789012345, .234567890123456, 0.],
                   [.345678901234567, .456789012345678, 0.],
                   [.567890123456789, .67890123456789, 0.],
                   [.789012345678901, .890123456789012, 0.]])
POINT = np.array([1.23456789012345, -.456789012345678, 0.])
HANDLE1 = np.array([.123456789012345, .987654321098765, 0.])
HANDLE2 = np.array([.876543210987654, .567890123456789, 0.])


def observe_scalar_path_precision(module, method):
    value = module.VMobject().set_points(PREFIX.copy())
    arguments = {
        'set_points': (np.array([HANDLE1, POINT]),),
        'append_points': (np.array([HANDLE1, POINT]),),
        'start_new_path': (POINT,), 'add_line_to': (POINT,),
        'add_quadratic_bezier_curve_to': (HANDLE1, POINT),
        'add_cubic_bezier_curve_to': (HANDLE1, HANDLE2, POINT),
        'add_cubic_bezier_curve': (PREFIX[0], HANDLE1, HANDLE2, POINT),
        'add_points_as_corners': (np.array([HANDLE1, POINT]),),
        'set_points_as_corners': (np.array([PREFIX[0], HANDLE1, POINT]),),
        'close_path': (),
    }
    result = getattr(value, method)(*arguments[method])
    return {'method': method, 'returnsSelf': result is value,
            'pointsXY': [[float(p[0]), float(p[1])] for p in value.get_all_points()]}
