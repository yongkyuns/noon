"""ManimCE v0.21 enum tokens for Python authoring and metadata.

Values pinned to ManimCommunity/manim@861cd4849b17db1db3515b531ffe80b297848f93.
"""
from enum import Enum, StrEnum


class RendererType(Enum):
    """Pinned Manim renderer-selection tokens."""
    CAIRO = "cairo"
    OPENGL = "opengl"


class LineJointType(Enum):
    """Pinned Manim line-joint tokens."""
    AUTO = 0
    ROUND = 1
    BEVEL = 2
    MITER = 3


class CapStyleType(Enum):
    """Pinned Manim line-cap tokens."""
    AUTO = 0
    ROUND = 1
    BUTT = 2
    SQUARE = 3


class DefaultSectionType(StrEnum):
    """Pinned default section metadata token."""
    NORMAL = "default.normal"


