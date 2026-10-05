"""Thin Matrix facades over retained Rust Matrix families."""
from _noon_errors import engine_call
import noon as _base
import _manim_compat as _compat
from _manim_semantic_handles import (
    _attach_shared_family,
    _family_wrapper_key,
    _handle_for,
    _live_constructor_context,
)

try:
    from _noon_host import noonCreateAuthoringMatrixHandle as _create_matrix
    from _noon_host import noonCreateAuthoringIntegerMatrixHandle as _create_integer
    from _noon_host import noonCreateAuthoringDecimalMatrixHandle as _create_decimal
    from _noon_host import noonCreateAuthoringMobjectMatrixHandle as _create_mobject_matrix
    from _noon_host import noonMatrixFromFamily as _matrix_from_family
except ImportError:
    _create_matrix = _create_integer = _create_decimal = _create_mobject_matrix = _matrix_from_family = None


_OPTION_NAMES = {"v_buff", "h_buff", "bracket_h_buff", "bracket_v_buff", "stretch_brackets"}


def _matrix_options(kwargs):
    unknown = set(kwargs) - _OPTION_NAMES
    if unknown:
        raise NotImplementedError("unsupported Matrix option(s): " + ", ".join(sorted(unknown)))
    return (
        kwargs.get("v_buff", .8), kwargs.get("h_buff", 1.3),
        kwargs.get("bracket_h_buff", .25), kwargs.get("bracket_v_buff", .25),
        kwargs.get("stretch_brackets", True),
    )


def _matrix_rows(matrix, convert):
    return [[convert(value) for value in row] for row in matrix]


def _handle_key(handle):
    return f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}"


class Matrix(_compat.VGroup):
    def __init__(self, matrix, **kwargs):
        if _create_matrix is None:
            raise RuntimeError("Matrix requires Noon's shared Rust authoring runtime")
        context = _live_constructor_context("Matrix", allow_unstarted=True)
        self._initialize_matrix(
            engine_call(_create_matrix, _matrix_rows(matrix, str), *_matrix_options(kwargs), context), context
        )

    def _initialize_matrix(self, handle, context):
        self._matrix_handle = handle
        _attach_shared_family(self, engine_call(handle.family), context)

    def _rehydrate_semantic_family_handle(self):
        if _matrix_from_family is None:
            raise RuntimeError("Matrix requires Noon's shared Rust authoring runtime")
        self._initialize_matrix(
            engine_call(_matrix_from_family, self._semantic_family_handle),
            getattr(self, "_canonical_live_target_context", None),
        )

    def get_entries(self):
        """Return a flat VGroup of the Matrix's entry Mobjects."""
        handles = list(engine_call(self._matrix_handle.entries))
        known = {
            _family_wrapper_key(entry): entry
            for row in self._entry_family().submobjects
            for entry in row.submobjects
        }
        # The retained entry family is intentionally nested by rows.  This
        # public accessor follows Manim's flat entry order without changing it.
        return _compat.VGroup(*[known[_handle_key(entry)] for entry in handles])

    def _entry_family(self):
        return self.submobjects[0]

    def get_rows(self):
        return _compat.VGroup(*self._entry_family().submobjects)

    def get_columns(self):
        known = {
            _family_wrapper_key(entry): entry
            for row in self._entry_family().submobjects
            for entry in row.submobjects
        }
        return _compat.VGroup(*[
            _compat.VGroup(*[known[_handle_key(entry)] for entry in column])
            for column in engine_call(self._matrix_handle.columns)
        ])

    def get_brackets(self):
        return _compat.VGroup(*self.submobjects[1:])


class IntegerMatrix(Matrix):
    def __init__(self, matrix, **kwargs):
        if _create_integer is None:
            raise RuntimeError("IntegerMatrix requires Noon's shared Rust authoring runtime")
        context = _live_constructor_context("IntegerMatrix", allow_unstarted=True)
        self._initialize_matrix(
            engine_call(_create_integer, _matrix_rows(matrix, float), *_matrix_options(kwargs), context), context
        )


class DecimalMatrix(Matrix):
    def __init__(self, matrix, **kwargs):
        if _create_decimal is None:
            raise RuntimeError("DecimalMatrix requires Noon's shared Rust authoring runtime")
        context = _live_constructor_context("DecimalMatrix", allow_unstarted=True)
        self._initialize_matrix(
            engine_call(_create_decimal, _matrix_rows(matrix, float), *_matrix_options(kwargs), context), context
        )


class MobjectMatrix(Matrix):
    def __init__(self, matrix, **kwargs):
        if _create_mobject_matrix is None:
            raise RuntimeError("MobjectMatrix requires Noon's shared Rust authoring runtime")
        source_rows = [list(row) for row in matrix]
        supplied = _matrix_rows(source_rows, self._mobject_handle)
        context = _live_constructor_context("MobjectMatrix", allow_unstarted=True)
        self._initialize_matrix(
            engine_call(_create_mobject_matrix, supplied, *_matrix_options(kwargs), context), context
        )
        for row_group, row in zip(self._entry_family().submobjects, source_rows):
            row_group._semantic_member_wrappers = {
                _family_wrapper_key(value): value for value in row
            }

    @staticmethod
    def _mobject_handle(value):
        if not isinstance(value, _base.Mobject):
            raise TypeError("MobjectMatrix entries must be shared Mobjects or Groups")
        family = getattr(value, "_semantic_family_handle", None)
        if family is not None:
            return family
        handle = _handle_for(value)
        if handle is None:
            raise TypeError("MobjectMatrix entries require current shared semantic identities")
        return handle
