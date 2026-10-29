"""Typed values over Rust's finite painted pointwise WGSL extension.

No Python attachment state, interpolation, shader clock or frame updater.
"""
from collections.abc import Mapping
from numbers import Integral

from _noon_effects import _number, _update
from _noon_errors import engine_call


class _Immutable:
    __slots__ = ()

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        memo[id(self)] = self
        return self


def _integer(value):
    if isinstance(value, bool) or not isinstance(value, Integral):
        raise TypeError("unsigned shader values require an integer")
    return _number(value, "unsigned shader value")


def _vector(value):
    result = [_number(v, "shader vector component") for v in value]
    if len(result) != 2:
        raise ValueError("shader vectors require two components")
    return result


def _color(value):
    from _manim_compat import _as_color
    c = _as_color("shader color", value)
    return [c.red, c.green, c.blue, c.alpha]


class ShaderParameter(_Immutable):
    """Immutable typed metadata, checked with the complete program in Rust.

    Floating fields interpolate; boolean and unsigned fields are discrete.
    Units are unitless, normalized, scene, pixels, radians or seconds.
    """
    __slots__ = ("_parameter",)

    def __init__(self):
        raise TypeError("use a typed ShaderParameter factory")

    @classmethod
    def _make(cls, method, name, *args):
        if not isinstance(name, str):
            raise TypeError("shader parameter name must be a string")
        from js import noonShaderParameter
        value = object.__new__(cls)
        value._parameter = engine_call(getattr(noonShaderParameter, method), name, *args)
        return value

    @classmethod
    def scalar(cls, name, *, default, minimum, maximum, unit="unitless"):
        return cls._make("scalar", name, _number(default, "default"),
                         _number(minimum, "minimum"), _number(maximum, "maximum"), unit)

    @classmethod
    def vector2(cls, name, *, default, minimum, maximum, unit="unitless"):
        from pyodide.ffi import to_js
        return cls._make("vector2", name, to_js(_vector(default)),
                         to_js(_vector(minimum)), to_js(_vector(maximum)), unit)

    @classmethod
    def color(cls, name, *, default):
        from pyodide.ffi import to_js
        return cls._make("color", name, to_js(_color(default)))

    @classmethod
    def boolean(cls, name, *, default):
        if not isinstance(default, bool):
            raise TypeError("boolean shader defaults require a boolean")
        return cls._make("boolean", name, default)

    @classmethod
    def unsigned(cls, name, *, default, minimum=0, maximum=4294967295):
        return cls._make("unsigned", name, _integer(default), _integer(minimum), _integer(maximum))


class WgslEffectProgram(_Immutable):
    """Prepare trusted WGSL once; reuse its immutable source and checked schema.

    shade(NoonShaderInput, NoonShaderParameters) returns straight encoded RGB
    and a coverage multiplier. Coordinates: local (+Y up normalized bounds),
    scene (+Y up), or pixels (top-left origin). Optional time is authored seconds.
    Module globals NoonGpuUniform, noon_uniform, noon_source, noon_unit,
    vs_fullscreen and fs_sample are reserved by this ABI.
    Source point morphs are unsupported; use Rotate for rigid rotation rather
    than animate.rotate, which requests point correspondence.
    """
    __slots__ = ("_program",)

    def __init__(self, source, *, parameters=(), coordinates="local", authored_time=False, seed=False):
        if not isinstance(source, str) or not isinstance(coordinates, str):
            raise TypeError("WGSL source and coordinates must be strings")
        if not isinstance(authored_time, bool) or not isinstance(seed, bool):
            raise TypeError("shader dependencies require booleans")
        from js import noonShaderSchema
        schema = engine_call(noonShaderSchema, coordinates, authored_time, seed)
        try:
            for parameter in parameters:
                if not isinstance(parameter, ShaderParameter):
                    raise TypeError("parameters require typed ShaderParameter values")
                engine_call(schema.addParameter, parameter._parameter)
            self._program = engine_call(schema.prepare, source)
        finally:
            schema.free()

    def parameter(self, name):
        if not isinstance(name, str):
            raise TypeError("shader parameter name must be a string")
        result = object.__new__(_ShaderParameterKey)
        result._key = engine_call(self._program.parameter, name)
        return result


class _ShaderParameterKey(_Immutable):
    __slots__ = ("_key",)

    def __init__(self):
        raise TypeError("obtain a bound key with program.parameter(name)")


def _write_parameters(update, parameters):
    if not isinstance(parameters, Mapping):
        raise TypeError("parameters require a mapping from program-bound keys to typed values")
    from pyodide.ffi import to_js
    for key, value in parameters.items():
        if not isinstance(key, _ShaderParameterKey):
            raise TypeError("shader parameter keys come from program.parameter(name)")
        kind = str(key._key.kind)
        if kind == "scalar":
            engine_call(update.setShaderScalar, key._key, _number(value, "shader scalar"))
        elif kind == "vector2":
            engine_call(update.setShaderVector2, key._key, to_js(_vector(value)))
        elif kind == "color":
            engine_call(update.setShaderColor, key._key, to_js(_color(value)))
        elif kind == "boolean":
            if not isinstance(value, bool):
                raise TypeError("boolean shader values require a boolean")
            engine_call(update.setShaderBoolean, key._key, value)
        elif kind == "unsigned":
            engine_call(update.setShaderUnsigned, key._key, _integer(value))
        else:
            raise RuntimeError("unknown shared Rust shader parameter kind")


class ShaderEffect(_Immutable):
    """An inert checked definition. Attach and animate with ordinary effect APIs."""
    __slots__ = ("_definition",)

    def __init__(self, program, *, parameters=None, seed=None):
        if not isinstance(program, WgslEffectProgram):
            raise TypeError("ShaderEffect requires a prepared WgslEffectProgram")
        from js import noonShaderEffect
        with _update(parameters=parameters, seed=seed) as update:
            self._definition = engine_call(noonShaderEffect, program._program, update)

    @classmethod
    def _from_definition(cls, definition):
        result = object.__new__(cls)
        result._definition = definition
        return result

    @property
    def program(self):
        result = object.__new__(WgslEffectProgram)
        result._program = engine_call(self._definition.shaderProgram)
        return result

    @property
    def seed(self):
        return int(self._definition.seed)

    def value(self, key):
        if not isinstance(key, _ShaderParameterKey):
            raise TypeError("shader parameter keys come from program.parameter(name)")
        value = engine_call(self._definition.shaderValue, key._key)
        kind = str(key._key.kind)
        if kind in ("scalar", "vector2", "color"):
            value = value.to_py()
        if kind == "scalar":
            return float(value[0])
        if kind == "color":
            from noon import Color
            return Color(*(float(v) for v in value))
        if kind == "vector2":
            return tuple(float(v) for v in value)
        if kind == "boolean":
            return bool(value)
        return int(value) if kind == "unsigned" else float(value)
