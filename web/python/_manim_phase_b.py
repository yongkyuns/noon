"""Phase-B glue for Manim source compatibility.

Kept separate from the compatibility surface while Phase B is under active development.
"""

from __future__ import annotations

import noon as _base
import _manim_compat as _compat


class _GenericAnimationBuilder(_compat._CompatAnimationBuilder, _base._AnimationBuilder):
    """Make the generic proxy recognizable by the existing Noon play lowerer."""


# The property installed by _manim_compat resolves this module global at call time,
# so replacing the class preserves the generic proxy while also satisfying the
# low-level Scene.play isinstance check for Noon's animation builder.
_compat._CompatAnimationBuilder = _GenericAnimationBuilder
