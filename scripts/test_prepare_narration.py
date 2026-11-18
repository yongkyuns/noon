#!/usr/bin/env python3
"""Offline behavior tests for narration preparation; never contact a provider."""
from __future__ import annotations

import copy
import io
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import urllib.error
import wave

import prepare_narration as narration


def wav_bytes(samples=(-32768, -16384, 0, 16384, 32767), *, rate=24000, channels=1, width=2):
    output = io.BytesIO()
    with wave.open(output, "wb") as stream:
        stream.setnchannels(channels)
        stream.setsampwidth(width)
        stream.setframerate(rate)
        stream.writeframes(struct.pack(f"<{len(samples)}h", *samples))
    return output.getvalue()


def speech_request():
    return {"provider": "elevenlabs", "voice_id": "explicit_voice", "model_id": "eleven_flash_v2_5",
            "text": "Double the input.", "voice_settings": {"stability": 0.5,
            "similarity_boost": 0.75, "style": 0, "use_speaker_boost": True, "speed": 1}}


class FakeOpener:
    def __init__(self, response=None, error=None):
        self.response = wav_bytes() if response is None else response
        self.error = error
        self.calls = []

    def open(self, request, timeout):
        self.calls.append((request, timeout))
        if self.error is not None:
            raise self.error
        return io.BytesIO(self.response)


class NarrationPreparationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.wav = self.directory / "recording.wav"
        self.wav.write_bytes(wav_bytes())
        self.cache = self.directory / "cache"

    def prepare(self, **options):
        return narration.prepare_narration(self.cache, wav_path=self.wav,
                                          spoken_text="x squared", **options)

    @staticmethod
    def manifest(path):
        return json.loads(path.read_text(encoding="utf-8"))

    def snapshot(self):
        return {str(path.relative_to(self.cache)): path.read_bytes()
                for path in self.cache.rglob("*") if path.is_file()} if self.cache.exists() else {}

    def test_import_decodes_exact_samples_and_measures_duration(self):
        path = self.prepare(caption_text="x²")
        manifest = self.manifest(path)
        audio = manifest["audio"]
        pcm = (path.parent / audio["path"]).read_bytes()
        self.assertEqual(struct.unpack("<5f", pcm), (-1, -0.5, 0, 0.5, 32767 / 32768))
        self.assertEqual((audio["sample_rate"], audio["channels"], audio["frames"]), (24000, 1, 5))
        self.assertEqual(audio["duration_seconds"], 5 / 24000)
        self.assertEqual(manifest["spoken_text"], "x squared")
        self.assertEqual(manifest["caption_text"], "x²")
        self.assertEqual(manifest["transcript_status"], "supplied-unverified")

    def test_stereo_duration_counts_frames_not_samples(self):
        self.wav.write_bytes(wav_bytes((-32768, 32767, 0, 16384), channels=2, rate=8000))
        audio = self.manifest(self.prepare())["audio"]
        self.assertEqual((audio["frames"], audio["channels"], audio["duration_seconds"]), (2, 2, 2 / 8000))

    def test_identical_request_reuses_complete_immutable_files(self):
        first = self.prepare()
        before = self.snapshot()
        with patch.object(narration, "_decode_wav", side_effect=AssertionError("unexpected decoding")):
            self.assertEqual(self.prepare(), first)
        self.assertEqual(self.snapshot(), before)

    def test_caption_and_cue_edits_reuse_exact_audio(self):
        first_path = self.prepare()
        first = self.manifest(first_path)
        audio_path = first_path.parent / first["audio"]["path"]
        before = audio_path.read_bytes(), audio_path.stat().st_mtime_ns
        cue = {"name": "term", "start_seconds": 0, "end_seconds": 4 / 24000, "provenance": "manual"}
        second_path = self.prepare(caption_text="x²", cues=[cue])
        second = self.manifest(second_path)
        self.assertNotEqual(first_path, second_path)
        self.assertEqual(first["request_id"], second["request_id"])
        self.assertEqual(first["audio"], second["audio"])
        self.assertEqual((audio_path.read_bytes(), audio_path.stat().st_mtime_ns), before)
        self.assertEqual(second["cues"], [cue])
        self.assertTrue(first_path.is_file())

    def test_changed_recording_keeps_previous_accepted_asset(self):
        first = self.manifest(self.prepare())
        before = self.snapshot()
        self.wav.write_bytes(wav_bytes((123, 456)))
        second = self.manifest(self.prepare())
        self.assertNotEqual(first["request_id"], second["request_id"])
        self.assertNotEqual(first["audio"]["asset_id"], second["audio"]["asset_id"])
        after = self.snapshot()
        self.assertTrue(all(after[key] == value for key, value in before.items()))

    def test_same_samples_in_different_formats_have_distinct_assets(self):
        first = self.manifest(self.prepare())["audio"]["asset_id"]
        self.wav.write_bytes(wav_bytes(rate=8000))
        second = self.manifest(self.prepare())["audio"]["asset_id"]
        self.assertNotEqual(first, second)

    def test_invalid_wav_does_not_publish_a_request_or_replace_accepted_files(self):
        self.prepare()
        before = self.snapshot()
        invalid = (b"not a WAV", wav_bytes()[:-1], wav_bytes(width=1), wav_bytes(rate=7999),
                   wav_bytes(rate=192001), wav_bytes(channels=5), wav_bytes(()))
        for payload in invalid:
            with self.subTest(payload=payload[:12]):
                self.wav.write_bytes(payload)
                with self.assertRaises(ValueError):
                    self.prepare()
                self.assertEqual(self.snapshot(), before)

    def test_decoded_budget_is_checked_before_allocating_output(self):
        with patch.object(narration, "MAX_PCM_BYTES", 16):
            with self.assertRaisesRegex(ValueError, "decoded PCM"):
                self.prepare()
        self.assertEqual(self.snapshot(), {})

    def test_incomplete_pcm_frame_and_truncated_outer_chunk_are_rejected(self):
        partial = bytearray(wav_bytes())
        # A nine-byte PCM chunk plus padding has a valid RIFF length but not
        # a whole number of 16-bit samples. wave alone rounds its frame count.
        struct.pack_into("<I", partial, 40, 9)
        truncated = bytearray(wav_bytes())
        struct.pack_into("<I", truncated, 4, len(truncated) + 8)
        for payload in (partial, truncated):
            self.wav.write_bytes(payload)
            with self.assertRaises(ValueError):
                self.prepare()
        self.assertEqual(self.snapshot(), {})

    def test_corrupt_audio_is_rejected_and_preserved_for_inspection(self):
        path = self.prepare()
        audio_path = path.parent / self.manifest(path)["audio"]["path"]
        audio_path.write_bytes(b"\0" * 20)
        before = self.snapshot()
        with self.assertRaisesRegex(ValueError, "integrity"):
            self.prepare()
        self.assertEqual(self.snapshot(), before)

    def test_corrupt_measured_audio_metadata_is_rejected(self):
        path = self.prepare()
        audio_path = path.parent / self.manifest(path)["audio"]["path"]
        metadata_path = audio_path.parent / "audio.json"
        metadata = self.manifest(metadata_path)
        metadata["duration_seconds"] = 1000
        metadata_path.write_text(json.dumps(metadata), encoding="utf-8")
        before = self.snapshot()
        with self.assertRaisesRegex(ValueError, "integrity"):
            self.prepare()
        self.assertEqual(self.snapshot(), before)

    def test_corrupt_request_and_manifest_are_not_silently_replaced(self):
        path = self.prepare()
        data = self.manifest(path)
        receipt = self.cache / "requests" / f"{data['request_id']}.json"
        original = receipt.read_bytes()
        receipt.write_text("{}", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "receipt"):
            self.prepare()
        self.assertEqual(receipt.read_text(), "{}")
        receipt.write_bytes(original)
        path.write_text("{}", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "immutable metadata"):
            self.prepare()
        self.assertEqual(path.read_text(), "{}")

    def test_failed_audio_publication_keeps_accepted_cache_and_cleans_stage(self):
        self.prepare()
        before = self.snapshot()
        self.wav.write_bytes(wav_bytes((123, 456)))
        with patch.object(narration.os, "rename", side_effect=OSError("injected publication failure")):
            with self.assertRaises(OSError):
                self.prepare()
        self.assertEqual(self.snapshot(), before)
        self.assertFalse(list(self.cache.rglob(".prepare-*")))

    def test_failed_manifest_publication_preserves_accepted_revision(self):
        first = self.prepare()
        before = self.snapshot()
        with patch.object(narration.os, "link", side_effect=OSError("injected publication failure")):
            with self.assertRaises(OSError):
                self.prepare(caption_text="revised")
        self.assertTrue(first.is_file())
        self.assertEqual(self.snapshot(), before)
        self.assertFalse(list(self.cache.rglob(".prepare-*")))

    def test_manual_cues_validate_names_intervals_and_provenance(self):
        cue = {"name": "term", "start_seconds": 0, "end_seconds": 4 / 24000, "provenance": "manual"}
        invalid = [[dict(cue, provenance="aligned")], [dict(cue, start_seconds=-1)],
                   [dict(cue, end_seconds=1)], [dict(cue, end_seconds=float("nan"))],
                   [dict(cue, start_seconds=True)], [cue, cue],
                   [dict(cue, name="later", start_seconds=1 / 24000), cue]]
        for cues in invalid:
            with self.subTest(cues=cues):
                with self.assertRaises(ValueError):
                    self.prepare(cues=cues)

    def synthesize(self, request=None, opener=None, **options):
        return narration.prepare_narration(self.cache, request=speech_request() if request is None else request,
                                          opener=opener, **options)

    def test_provider_request_is_explicit_and_cached_without_future_credentials(self):
        opener = FakeOpener()
        with patch.dict(os.environ, {"ELEVENLABS_API_KEY": "test-secret"}):
            first = self.synthesize(opener=opener)
        outgoing, timeout = opener.calls[0]
        self.assertEqual(outgoing.full_url,
            "https://api.elevenlabs.io/v1/text-to-speech/explicit_voice?output_format=wav_24000")
        self.assertEqual(outgoing.get_method(), "POST")
        self.assertEqual(outgoing.get_header("Xi-api-key"), "test-secret")
        self.assertEqual(timeout, 60)
        expected = speech_request()
        del expected["provider"], expected["voice_id"]
        self.assertEqual(json.loads(outgoing.data), expected)
        with patch.dict(os.environ, {}, clear=True):
            second = self.synthesize(opener=opener, caption_text="Different displayed text")
        self.assertEqual(len(opener.calls), 1)
        self.assertEqual(self.manifest(first)["audio"], self.manifest(second)["audio"])
        self.assertEqual(self.manifest(first)["transcript_status"], "provider-input-unverified")
        self.assertNotIn(b"test-secret", b"".join(self.snapshot().values()))

    def test_every_admitted_speech_input_changes_request_identity(self):
        base = speech_request()
        variants = []
        for key, value in (("text", "Another sentence"), ("voice_id", "another_voice"),
                           ("model_id", "another_model"), ("language_code", "en"),
                           ("previous_text", "First sentence."), ("next_text", "Following sentence."),
                           ("pronunciation_dictionary_locators", [{"pronunciation_dictionary_id": "dict", "version_id": "v1"}])):
            variants.append(dict(base, **{key: value}))
        for key, value in (("stability", 0.1), ("similarity_boost", 0.1), ("style", 0.1),
                           ("use_speaker_boost", False), ("speed", 1.1)):
            variant = copy.deepcopy(base)
            variant["voice_settings"][key] = value
            variants.append(variant)
        opener = FakeOpener()
        with patch.dict(os.environ, {"ELEVENLABS_API_KEY": "test-secret"}):
            identities = {self.manifest(self.synthesize(request, opener))["request_id"] for request in [base, *variants]}
        self.assertEqual(len(identities), len(variants) + 1)
        self.assertEqual(len(opener.calls), len(variants) + 1)
        # The provider double returned identical PCM; content storage can share it.
        self.assertEqual(len(list((self.cache / "audio").iterdir())), 1)

    def test_invalid_provider_settings_fail_before_network_or_credential_lookup(self):
        invalid = [dict(speech_request(), endpoint="https://invalid.example"),
                   dict(speech_request(), voice_id="../other"), dict(speech_request(), voice_settings={}),
                   dict(speech_request(), model_id="eleven_multilingual_v2", language_code="en"),
                   dict(speech_request(), pronunciation_dictionary_locators=[{"pronunciation_dictionary_id": "dict"}])]
        opener = FakeOpener()
        with patch.object(narration.os.environ, "get", side_effect=AssertionError("credential lookup")):
            for request in invalid:
                with self.subTest(request=request):
                    with self.assertRaises(ValueError):
                        self.synthesize(request, opener)
        self.assertEqual(opener.calls, [])

    def test_uncached_provider_requires_credential_and_never_calls_network_without_it(self):
        opener = FakeOpener()
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(ValueError, "ELEVENLABS_API_KEY"):
                self.synthesize(opener=opener)
        self.assertEqual(opener.calls, [])
        self.assertEqual(self.snapshot(), {})

    def test_malformed_credential_is_never_echoed_by_header_validation(self):
        opener = FakeOpener()
        with patch.dict(os.environ, {"ELEVENLABS_API_KEY": "test-secret\n"}):
            with self.assertRaises(ValueError) as caught:
                self.synthesize(opener=opener)
        self.assertNotIn("test-secret", str(caught.exception))
        self.assertEqual(opener.calls, [])

    def test_dangling_receipt_is_not_a_synthesis_cache_miss(self):
        opener = FakeOpener()
        with patch.dict(os.environ, {"ELEVENLABS_API_KEY": "test-secret"}):
            path = self.synthesize(opener=opener)
        manifest = self.manifest(path)
        receipt = self.cache / "requests" / f"{manifest['request_id']}.json"
        receipt.unlink()
        receipt.symlink_to(self.directory / "missing-receipt.json")
        with patch.object(narration.os.environ, "get", side_effect=AssertionError("credential lookup")):
            with self.assertRaisesRegex(ValueError, "receipt.*symlink"):
                self.synthesize(opener=opener)
        self.assertEqual(len(opener.calls), 1)
        self.assertTrue(receipt.is_symlink())
        self.assertEqual(len(list((self.cache / "audio").iterdir())), 1)
        self.assertTrue(path.is_file())

    def test_invalid_cache_destinations_fail_before_synthesis(self):
        for name in ("root", "audio", "requests", "narrations"):
            with self.subTest(name=name):
                cache = self.directory / f"invalid-{name}"
                if name == "root":
                    cache.write_bytes(b"preserve")
                else:
                    cache.mkdir()
                    (cache / name).write_bytes(b"preserve")
                opener = FakeOpener()
                with patch.object(narration.os.environ, "get", side_effect=AssertionError("credential lookup")):
                    with self.assertRaises(OSError):
                        narration.prepare_narration(cache, request=speech_request(), opener=opener)
                self.assertEqual(opener.calls, [])
                self.assertEqual((cache if name == "root" else cache / name).read_bytes(), b"preserve")

    def test_provider_failure_does_not_retry_publish_or_echo_response_body(self):
        self.prepare()
        before = self.snapshot()
        error = urllib.error.HTTPError("https://api.elevenlabs.io", 401, "sensitive detail", {}, io.BytesIO(b"test-secret"))
        opener = FakeOpener(error=error)
        with patch.dict(os.environ, {"ELEVENLABS_API_KEY": "test-secret"}):
            with self.assertRaises(ValueError) as caught:
                self.synthesize(opener=opener)
        self.assertEqual(str(caught.exception), "ElevenLabs synthesis failed with HTTP 401")
        self.assertEqual(len(opener.calls), 1)
        self.assertEqual(self.snapshot(), before)

    def test_redirect_handler_refuses_credential_forwarding(self):
        self.assertIsNone(narration._NoRedirect().redirect_request(None, None, 307, "redirect", {}, "https://elsewhere.invalid"))

    def test_invalid_or_oversized_provider_response_is_not_cached(self):
        for payload in (b"not a WAV", b"x" * 129, wav_bytes(rate=8000)):
            with self.subTest(payload=payload[:12]), patch.dict(os.environ, {"ELEVENLABS_API_KEY": "test-secret"}), \
                    patch.object(narration, "MAX_WAV_BYTES", 128):
                with self.assertRaises(ValueError):
                    self.synthesize(opener=FakeOpener(payload))
        self.assertEqual(self.snapshot(), {})

    def test_duplicate_json_fields_are_rejected(self):
        path = self.directory / "duplicate.json"
        path.write_text('{"name":"first","name":"second"}', encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "duplicate"):
            narration._read_json(path)

    def test_cli_import_creates_reusable_manifest_without_provider_access(self):
        transcript = self.directory / "spoken.txt"
        transcript.write_text("x squared", encoding="utf-8")
        result = subprocess.run([sys.executable, str(Path(narration.__file__)), "--wav", str(self.wav),
            "--spoken-text-file", str(transcript), "--cache-dir", str(self.cache)],
            check=True, capture_output=True, text=True)
        self.assertTrue(Path(result.stdout.strip()).is_file())
        self.assertEqual(self.manifest(Path(result.stdout.strip()))["spoken_text"], "x squared")


if __name__ == "__main__":
    unittest.main()
