"""Python argument/handle routing only; Rust and browser tests own audio proof."""
import copy
import math
import sys
import types
import unittest
from array import array
from unittest.mock import Mock, patch

import noon
import _noon_audio as audio
from _manim_reactive import ValueTracker


class AudioFacadeTests(unittest.TestCase):
    def setUp(self):
        self.resource = types.SimpleNamespace(sampleRate=8_000, channels=1, frames=8_000)
        self.host = Mock()
        self.host.smallDiscovery.return_value = self.resource
        self.host.fromInterleaved.return_value = self.resource
        self.options = Mock()
        self.options_factory = Mock(return_value=self.options)
        self.clip_handle = types.SimpleNamespace(startTime=1.0, endTime=3.0, duration=2.0)
        self.context = Mock()
        self.context.audio.return_value = self.clip_handle
        self.scene = noon.Scene()
        self.scene._canonical_authoring_context = self.context
        js = types.ModuleType("js")
        js.noonAudioSource = self.host
        js.noonAudioClipOptions = self.options_factory
        js.noonResolveAnimationOptions = Mock()
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = lambda view: view
        self.enter(patch.dict(sys.modules, {"js": js, "pyodide.ffi": ffi}))

    def enter(self, context):
        result = context.__enter__()
        self.addCleanup(context.__exit__, None, None, None)
        return result

    def source(self):
        return noon.AudioSource._from_handle(self.resource)

    def test_bundled_source_is_one_rust_preparation_and_only_an_opaque_handle(self):
        source = noon.AudioSource.small_discovery()
        self.host.smallDiscovery.assert_called_once_with()
        self.assertEqual((source.sample_rate, source.channels, source.frames), (8_000, 1, 8_000))
        self.assertIs(copy.copy(source), source)
        self.assertIs(copy.deepcopy(source), source)
        self.assertFalse(hasattr(source, "samples"))
        self.assertFalse(hasattr(source, "__dict__"))
        with self.assertRaises(AttributeError):
            source.frames = 2

    def test_interleaved_pcm_crosses_as_one_float32_payload(self):
        samples = array("f", [0.25, -0.5, 1.25, -1.5])
        source = noon.AudioSource.from_interleaved(samples, sample_rate=8_000, channels=2)
        payload, rate, channels = self.host.fromInterleaved.call_args.args
        self.assertIs(payload.obj, samples)
        self.assertEqual(payload.format, "f")
        self.assertEqual(list(payload), list(samples))
        self.assertEqual((rate, channels), (8_000, 2))
        self.assertIs(source._handle, self.resource)
        self.assertFalse(hasattr(source, "_pcm"))

    def test_format_boundary_rejects_python_wraparound_and_boolean_arguments(self):
        for option, value, exception in [
            ("sample_rate", True, TypeError),
            ("sample_rate", 8_000.5, TypeError),
            ("sample_rate", -1, ValueError),
            ("sample_rate", 2**32, ValueError),
            ("channels", False, TypeError),
            ("channels", 2**16, ValueError),
        ]:
            options = {"sample_rate": 8_000, "channels": 1, option: value}
            with self.subTest(option=option, value=value), self.assertRaises(exception):
                noon.AudioSource.from_interleaved([0.0], **options)
        self.host.fromInterleaved.assert_not_called()

    def test_shared_pcm_rejection_is_preserved_without_frontend_normalization(self):
        failure = ValueError("shared nonfinite PCM rejection")
        self.host.fromInterleaved.side_effect = failure
        with self.assertRaises(ValueError) as caught:
            noon.AudioSource.from_interleaved([math.nan], sample_rate=8_000)
        self.assertIs(caught.exception, failure)
        self.assertTrue(math.isnan(self.host.fromInterleaved.call_args.args[0][0]))

    def test_audio_preserves_absolute_defaults_and_frees_only_inert_options(self):
        clip = self.scene.audio(self.source())
        self.options_factory.assert_called_once_with(0.0, 0, None, None, False, 1.0, 0.0, 0.0)
        self.context.audio.assert_called_once_with(self.resource, self.options)
        self.options.free.assert_called_once_with()
        self.assertIsInstance(clip, noon.AudioClip)
        self.assertEqual((clip.start_time, clip.end_time, clip.duration), (1.0, 3.0, 2.0))
        self.assertIs(copy.copy(clip), clip)
        self.assertIs(copy.deepcopy(clip), clip)
        self.assertFalse(hasattr(self.scene, "_audio"))
        self.assertFalse(hasattr(clip, "__dict__"))

    def test_loop_trim_and_fade_arguments_are_forwarded_without_a_python_schedule(self):
        source = self.source()
        self.scene.audio(source, start_time=0.5, source_start_frame=250,
                         source_end_frame=750, duration=3.0, loop=True,
                         gain=0.4, fade_in=0.25, fade_out=0.75)
        self.options_factory.assert_called_once_with(0.5, 250, 750, 3.0, True, 0.4, 0.25, 0.75)
        self.scene.audio(source, start_time=10.0)
        self.assertEqual([call.args[0] for call in self.context.audio.call_args_list],
                         [self.resource, self.resource])
        self.host.fromInterleaved.assert_not_called()

    def test_clip_options_invalid_python_types_fail_before_admission(self):
        for key, value, exception in [
            ("start_time", True, TypeError), ("gain", "1", TypeError),
            ("fade_in", False, TypeError), ("fade_out", [], TypeError),
            ("duration", "2", TypeError), ("loop", 1, TypeError),
            ("source_start_frame", 1.5, TypeError),
            ("source_end_frame", True, TypeError),
            ("source_start_frame", -1, ValueError),
            ("source_end_frame", 2**32, ValueError),
        ]:
            with self.subTest(key=key, value=value), self.assertRaises(exception):
                self.scene.audio(self.source(), **{key: value})
        with self.assertRaises(TypeError):
            self.scene.audio("music.mp3")
        self.context.audio.assert_not_called()
        self.options_factory.assert_not_called()

    def test_shared_option_rejection_releases_options_and_keeps_rust_error(self):
        failure = ValueError("shared fade exceeds duration")
        self.context.audio.side_effect = failure
        with self.assertRaises(ValueError) as caught:
            self.scene.audio(self.source(), fade_in=float("nan"))
        self.assertIs(caught.exception, failure)
        self.assertTrue(math.isnan(self.options_factory.call_args.args[-2]))
        self.options.free.assert_called_once_with()
        self.assertFalse(hasattr(self.scene, "_audio"))

    def test_clip_observations_read_the_rust_handle_and_preserve_stale_failure(self):
        class Handle:
            failure = None
            @property
            def startTime(self):
                if self.failure:
                    raise self.failure
                return 0.5
        handle = Handle()
        self.context.audio.return_value = handle
        clip = self.scene.audio(self.source())
        self.assertEqual(clip.start_time, 0.5)
        handle.failure = ReferenceError("retired Rust clip generation")
        with self.assertRaises(ReferenceError) as caught:
            clip.start_time
        self.assertIs(caught.exception, handle.failure)

    def test_scalar_declaration_reuses_scene_tracker_and_rust_cursor(self):
        tracker = ValueTracker._from_canonical(self.scene, self.context, object())
        self.assertIs(self.scene.declare_value(tracker, 4.0, run_time=2, rate_func="linear"), self.scene)
        self.context.declareValue.assert_called_once_with(tracker._canonical_handle, 4.0, 2.0, "linear")
        self.context.beginOrdinaryWait.assert_not_called()
        self.context.createExecutionPlayer.assert_not_called()
        self.assertFalse(hasattr(self.scene, "_time"))

    def test_scalar_foreign_or_detached_handles_fail_without_association_or_publication(self):
        for tracker in [
            ValueTracker._from_canonical(noon.Scene(), object(), object()),
            ValueTracker._from_canonical(None, None, object()),
        ]:
            with self.assertRaises(ValueError):
                self.scene.declare_value(tracker, 2.0)
        self.context.declareValue.assert_not_called()
        self.context.associateValueTracker.assert_not_called()

    def test_scene_playback_control_uses_ordinary_handle_and_typed_action(self):
        target = object.__new__(noon.Mobject)
        target._scene = self.scene
        target._semantic_handle = object()
        for action in noon.PlaybackControl:
            self.assertIs(self.scene.on_click_playback(target, action), self.scene)
            self.context.onClickPlayback.assert_called_with(target._semantic_handle, action.value)
        with self.assertRaises(TypeError):
            self.scene.on_click_playback(target, "restart")
        target._scene = noon.Scene()
        with self.assertRaises(ValueError):
            self.scene.on_click_playback(target, noon.PlaybackControl.RESTART)


if __name__ == "__main__":
    unittest.main()
