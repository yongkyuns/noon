"""Thin Matrix facade over the shared Rust matrix family."""
from _noon_errors import engine_call
import noon as _base
import _manim_compat as _compat
from _manim_semantic_handles import _attach_shared_family, _live_constructor_context

try:
    from js import noonCreateAuthoringMatrixHandle as _create_matrix
    from js import noonCreateAuthoringIntegerMatrixHandle as _create_integer
    from js import noonCreateAuthoringDecimalMatrixHandle as _create_decimal
except ImportError:
    _create_matrix = _create_integer = _create_decimal = None

class Matrix(_compat.VGroup):
    def __init__(self, matrix, **kwargs):
        if _create_matrix is None: raise RuntimeError("Matrix requires Noon's shared Rust authoring runtime")
        allowed = {"v_buff", "h_buff", "bracket_h_buff", "bracket_v_buff", "stretch_brackets"}
        unknown = set(kwargs) - allowed
        if unknown: raise NotImplementedError("unsupported Matrix option(s): " + ", ".join(sorted(unknown)))
        context = _live_constructor_context("Matrix")
        rows = [[str(value) for value in row] for row in matrix]
        self._initialize_matrix(engine_call(_create_matrix, rows, kwargs.get("v_buff", .8), kwargs.get("h_buff", 1.3), kwargs.get("bracket_h_buff", .25), kwargs.get("bracket_v_buff", .25), kwargs.get("stretch_brackets", True), context), context)
    def _initialize_matrix(self, handle, context):
        self._matrix_handle = handle
        _attach_shared_family(self, engine_call(handle.family), context)
    def get_entries(self):
        return _attach_shared_family(object.__new__(_compat.VGroup), engine_call(self._matrix_handle.entryFamily), getattr(self, "_canonical_live_target_context", None))
    def get_rows(self):
        return list(self.get_entries().submobjects)
    def get_columns(self):
        return [_attach_shared_family(object.__new__(_compat.VGroup), family, getattr(self, "_canonical_live_target_context", None)) for family in engine_call(self._matrix_handle.columnFamilies)]
    def get_brackets(self):
        return _compat.VGroup(*self.submobjects[1:])

class IntegerMatrix(Matrix):
    def __init__(self, matrix, **kwargs):
        if _create_integer is None: raise RuntimeError("IntegerMatrix requires Noon's shared Rust authoring runtime")
        context = _live_constructor_context("IntegerMatrix")
        self._initialize_matrix(engine_call(_create_integer, [[float(v) for v in row] for row in matrix], kwargs.get("v_buff", .8), kwargs.get("h_buff", 1.3), kwargs.get("bracket_h_buff", .25), kwargs.get("bracket_v_buff", .25), kwargs.get("stretch_brackets", True), context), context)

class DecimalMatrix(Matrix):
    def __init__(self, matrix, **kwargs):
        if _create_decimal is None: raise RuntimeError("DecimalMatrix requires Noon's shared Rust authoring runtime")
        context = _live_constructor_context("DecimalMatrix")
        self._initialize_matrix(engine_call(_create_decimal, [[float(v) for v in row] for row in matrix], kwargs.get("v_buff", .8), kwargs.get("h_buff", 1.3), kwargs.get("bracket_h_buff", .25), kwargs.get("bracket_v_buff", .25), kwargs.get("stretch_brackets", True), context), context)
