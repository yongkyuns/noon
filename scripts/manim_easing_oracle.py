"""Shared original-source probes for explicit user easing callables."""
from decimal import Decimal
from fractions import Fraction
import inspect
import math
from pathlib import Path
import runpy

import numpy as np

normalize = runpy.run_path(str(Path(__file__).with_name("manim-simple-functions.py")),
                          run_name="_easing_normalization")["normalize"]


def easing_observations(namespace, names):
    result = []
    for name in names:
        function = namespace[name]
        result.append([name, str(inspect.signature(function))])
        for value in [-math.inf, -1, *[index/256 for index in range(257)], 2, math.inf,
                      math.nan, Fraction(1, 4), Decimal("0.25"), True, np.float32(0.25),
                      np.float64(0.25), "0.5", np.array([0.25]), np.array([0.25, 0.5])]:
            try:
                observed = function(value)
                result.append([name, "value", normalize(observed), type(observed).__name__])
            except Exception as error:
                result.append([name, "error", type(error).__name__])
    return result
