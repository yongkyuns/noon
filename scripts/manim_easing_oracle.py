"""Shared original-source probes for explicit user easing callables."""
from decimal import Decimal
from fractions import Fraction
import inspect
import hashlib
import sys
from functools import wraps
import math
from pathlib import Path
import runpy

import numpy as np
from pinned_python_helpers import load_helpers

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


def qualify_easings(source_root, names, *, installed_manim=False, module_only=False):
    """Use one source/installation/public-identity protocol for every easing batch."""
    source = source_root / "manim/utils/rate_functions.py"
    original = load_helpers(source, set(names) | {"unit_interval"},
                            {"wraps": wraps, "np": np, "sqrt": math.sqrt})
    expected = easing_observations(original, names)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_rate_functions as rates
    import noon
    public = noon.rate_functions if module_only else noon
    errors = [] if easing_observations(vars(rates), names) == expected else ["easing observations differ"]
    for name in names:
        function = getattr(public, name)
        if function is not getattr(rates, name) or rates.easing_from_rate_func(function) != name:
            errors.append(f"public easing identity {name} differs")
    if installed_manim:
        from manim.utils import rate_functions
        if Path(rate_functions.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed complete rate module differs from pin")
        if easing_observations(vars(rate_functions), names) != expected:
            errors.append("installed easing observations differ")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "functions": len(names), "observations": len(expected),
            "installed_module_checked": installed_manim, "errors": errors}
