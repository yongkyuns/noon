"""Fail closed when Pango renders an explicit profile with different font bytes."""
import ctypes
import hashlib
import importlib
from pathlib import Path


def _fontconfig_face(family):
    # Some macOS wheels statically link separate Fontconfig instances into the
    # registration and rendering extensions. The font list can therefore report
    # a registered face that the renderer cannot use. Query the rendering copy.
    renderer = importlib.import_module("manimpango.cmanimpango")
    library = ctypes.CDLL(renderer.__file__)
    declarations = {
        "FcConfigGetCurrent": ([], ctypes.c_void_p),
        "FcPatternCreate": ([], ctypes.c_void_p),
        "FcPatternAddString": ([ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p], ctypes.c_int),
        "FcConfigSubstitute": ([ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int], ctypes.c_int),
        "FcDefaultSubstitute": ([ctypes.c_void_p], None),
        "FcFontMatch": ([ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_int)], ctypes.c_void_p),
        "FcPatternGetString": ([ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int, ctypes.POINTER(ctypes.c_char_p)], ctypes.c_int),
        "FcPatternDestroy": ([ctypes.c_void_p], None),
    }
    for name, (arguments, result) in declarations.items():
        function = getattr(library, name)
        function.argtypes, function.restype = arguments, result
    pattern = library.FcPatternCreate()
    if not pattern:
        raise RuntimeError("Fontconfig could not create the reference pattern")
    match = None
    try:
        if not library.FcPatternAddString(pattern, b"family", family.encode("utf-8")):
            raise RuntimeError("Fontconfig could not set the reference family")
        config = library.FcConfigGetCurrent()
        if not library.FcConfigSubstitute(config, pattern, 0):
            raise RuntimeError("Fontconfig reference substitution failed")
        library.FcDefaultSubstitute(pattern)
        status = ctypes.c_int()
        match = library.FcFontMatch(config, pattern, ctypes.byref(status))
        if not match or status.value != 0:
            raise RuntimeError("Fontconfig found no reference face")
        resolved = {}
        for field in ("family", "style", "file"):
            value = ctypes.c_char_p()
            if library.FcPatternGetString(match, field.encode(), 0, ctypes.byref(value)) != 0:
                raise RuntimeError("Fontconfig omitted reference " + field)
            resolved[field] = value.value.decode("utf-8")
        return resolved
    finally:
        if match:
            library.FcPatternDestroy(match)
        library.FcPatternDestroy(pattern)


def verify_fontconfig_reference(family, fonts):
    """Check the default regular face against explicitly supplied font files."""
    allowed = {hashlib.sha256(Path(font).read_bytes()).hexdigest() for font in fonts}
    try:
        resolved = _fontconfig_face(family)
        digest = hashlib.sha256(Path(resolved["file"]).read_bytes()).hexdigest()
    except (AttributeError, ImportError, OSError, RuntimeError, ValueError) as error:
        raise RuntimeError("could not verify the Pango rendering font: " + str(error)) from error
    if resolved["family"].casefold() != family.casefold() or digest not in allowed:
        raise RuntimeError(
            f"Pango rendering font mismatch: requested {family!r}; "
            f"resolved {resolved['family']!r} from {resolved['file']!r} ({digest})"
        )
    return {**resolved, "sha256": digest}
