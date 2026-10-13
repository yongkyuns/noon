"""Manim-compatible operations on user data, outside engine execution."""

from __future__ import annotations

import math
from collections.abc import Callable
from functools import lru_cache
from typing import Any, Protocol, TypeVar


class Comparable(Protocol):
    def __lt__(self, other: Any) -> bool: ...
    def __gt__(self, other: Any) -> bool: ...


ComparableT = TypeVar("ComparableT", bound=Comparable)


@lru_cache(maxsize=10)
def choose(n: int, k: int) -> int:
    """Return the binomial coefficient with the pinned bounded cache."""
    return math.comb(n, k)


def clip(a: ComparableT, min_a: ComparableT, max_a: ComparableT) -> ComparableT:
    """Preserve comparison order and the identity of the selected argument."""
    if a < min_a:
        return min_a
    elif a > max_a:
        return max_a
    return a


def sigmoid(x: float) -> float:
    """Evaluate the pinned NumPy expression without coercing user values."""
    import numpy as np
    return 1.0 / (1 + np.exp(-x))


def binary_search(
    function: Callable[[float], float],
    target: float,
    lower_bound: float,
    upper_bound: float,
    tolerance: float = 1e-4,
) -> float | None:
    """Search user callbacks with Manim's evaluation order and termination."""
    import numpy as np
    lh = lower_bound
    rh = upper_bound
    mh = np.mean(np.array([lh, rh]))
    while abs(rh - lh) > tolerance:
        mh = np.mean(np.array([lh, rh]))
        lx, mx, rx = (function(h) for h in (lh, mh, rh))
        if lx == target:
            return lh
        if rx == target:
            return rh
        if lx <= target <= rx:
            if mx > target:
                rh = mh
            else:
                lh = mh
        elif lx > target > rx:
            lh, rh = rh, lh
        else:
            return None
    return mh
