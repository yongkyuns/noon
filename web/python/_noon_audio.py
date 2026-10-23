"""Finite audio syntax over shared Rust resources, clips, and authored time.

Only argument conversion and opaque handle references live here. Source PCM is
prepared once; Rust owns admission, trim/loop/fade interpretation, mixing, and
the execution clock. These declarations precede playback and do not change the
ordinary Scene.play/Scene.wait continuation contract.
"""
from __future__ import annotations

from array import array
from enum import Enum
from numbers import Real
from operator import index

from _noon_errors import engine_call


def _number(value, name):
    if isinstance(value, bool) or not isinstance(value, Real):
        raise TypeError(f"{name} must be a number, not a boolean or string")
    try:
        return float(value)
    except OverflowError as error:
        raise ValueError(f"{name} is outside the host numeric range") from error


def _integer(value, name, maximum):
    if isinstance(value, bool):
        raise TypeError(f"{name} must be an integer, not a boolean")
    value = index(value)
    # Prevent the WASM boundary's integer coercion from wrapping/truncating.
    # Resource format and clip range validation remain in shared Rust.
    if not 0 <= value <= maximum:
        raise ValueError(f"{name} is outside the WASM integer range")
    return value


def _host():
    try:
        from js import noonAudioSource
    except ImportError as error:
        raise RuntimeError("audio requires the shared Rust authoring host") from error
    return noonAudioSource


class AudioSource:
    """Prepared immutable mono/stereo PCM owned by Rust.

    Use small_discovery() for the bundled original CC0 music, or
    from_interleaved() for already-decoded PCM. A reused source crosses the
    Python/Rust boundary only once; clip attachment shares its Rust payload.
    """
    __slots__ = ("_handle",)

    def __init__(self):
        raise TypeError("use AudioSource.small_discovery() or AudioSource.from_interleaved()")

    @classmethod
    def _from_handle(cls, handle):
        source = object.__new__(cls)
        source._handle = handle
        return source

    @classmethod
    def small_discovery(cls):
        """Prepare the original 80-second stereo composition at 24 kHz."""
        return cls._from_handle(engine_call(_host().smallDiscovery, operation="AudioSource.small_discovery"))

    @classmethod
    def from_interleaved(cls, samples, *, sample_rate, channels=1):
        """Copy interleaved float PCM into an immutable shared Rust resource.

        sample_rate is in Hz; channels is 1 or 2. Stereo samples alternate
        left/right. Rust rejects nonfinite PCM and retains the original levels,
        with no automatic normalization or clipping. File decoding is separate.
        """
        rate = _integer(sample_rate, "sample_rate", 0xffffffff)
        count = _integer(channels, "channels", 0xffff)
        host = _host()
        from pyodide.ffi import to_js
        # A contiguous float32 payload, not a Python list of per-sample bridge calls.
        pcm = samples if isinstance(samples, array) and samples.typecode == "f" else array("f", samples)
        return cls._from_handle(engine_call(
            host.fromInterleaved, to_js(memoryview(pcm)), rate, count,
            operation="AudioSource.from_interleaved",
        ))

    @property
    def sample_rate(self):
        return int(self._handle.sampleRate)

    @property
    def channels(self):
        return int(self._handle.channels)

    @property
    def frames(self):
        return int(self._handle.frames)

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        memo[id(self)] = self
        return self


class AudioClip:
    """Opaque alias of one Rust-owned semantic clip; no Python clip state."""
    __slots__ = ("_handle",)

    def __init__(self):
        raise TypeError("obtain an AudioClip from Scene.audio()")

    @classmethod
    def _from_handle(cls, handle):
        clip = object.__new__(cls)
        clip._handle = handle
        return clip

    @property
    def start_time(self):
        return float(engine_call(lambda: self._handle.startTime, operation="AudioClip.start_time"))

    @property
    def end_time(self):
        return float(engine_call(lambda: self._handle.endTime, operation="AudioClip.end_time"))

    @property
    def duration(self):
        return float(engine_call(lambda: self._handle.duration, operation="AudioClip.duration"))

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        memo[id(self)] = self
        return self


class PlaybackControl(Enum):
    """Viewer requests emitted by the ordinary Rust click/input publication."""
    TOGGLE_PLAYBACK = "toggle_playback"
    RESTART = "restart"
    PREVIOUS_SECTION = "previous_section"
    NEXT_SECTION = "next_section"
    TOGGLE_AUDIO_MUTED = "toggle_audio_muted"


def audio(scene, source, *, start_time=0.0, source_start_frame=0,
          source_end_frame=None, duration=None, loop=False, gain=1.0,
          fade_in=0.0, fade_out=0.0):
    if not isinstance(source, AudioSource):
        raise TypeError("Scene.audio requires an AudioSource")
    if not isinstance(loop, bool):
        raise TypeError("loop must be a boolean")
    arguments = (
        _number(start_time, "start_time"),
        _integer(source_start_frame, "source_start_frame", 0xffffffff),
        None if source_end_frame is None else _integer(source_end_frame, "source_end_frame", 0xffffffff),
        None if duration is None else _number(duration, "duration"),
        loop,
        _number(gain, "gain"),
        _number(fade_in, "fade_in"),
        _number(fade_out, "fade_out"),
    )
    from js import noonAudioClipOptions
    from _manim_scene import _context
    options = engine_call(noonAudioClipOptions, *arguments, operation="Scene.audio")
    try:
        return AudioClip._from_handle(engine_call(
            _context(scene).audio, source._handle, options, operation="Scene.audio",
        ))
    finally:
        options.free()


def on_click_playback(scene, target, action):
    import noon
    from _manim_scene import _context
    if not isinstance(target, noon.Mobject) or target._scene is not scene:
        raise ValueError("playback control target must belong to this Scene")
    if not isinstance(action, PlaybackControl):
        raise TypeError("playback action must be a PlaybackControl")
    handle = getattr(target, "_semantic_handle", None)
    if handle is None:
        raise ValueError("playback control requires an ordinary shared Mobject handle")
    engine_call(_context(scene).onClickPlayback, handle, action.value, operation="Scene.on_click_playback")
    return scene
