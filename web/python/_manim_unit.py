"""Configuration-independent values from ManimCE's unit module.

Pixels and Percent depend on frame configuration and remain outside this
bounded module subset. These values operate only on caller-owned numbers.
"""
from math import pi
import sys

Degrees = pi / 180
Munits = 1
unit = sys.modules[__name__]
__all__ = ["Degrees", "Munits"]
