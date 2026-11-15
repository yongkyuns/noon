#!/usr/bin/env python3
"""Prepare immutable narration audio before Noon scene execution.

Imports bounded 16-bit PCM WAV recordings, or optionally requests one ElevenLabs
speech response. Captions and manually timed cues are separate from the audio
cache. This external media boundary does not schedule scenes or align words.

See https://elevenlabs.io/docs/api-reference/text-to-speech/convert.md for the
optional provider contract. No provider request is made on a verified cache hit.
"""
from __future__ import annotations

import argparse
from array import array
import hashlib
import http.client
import io
import json
import math
import os
from pathlib import Path
import re
import struct
import sys
import tempfile
import urllib.error
import urllib.request
import wave


MAX_PCM_BYTES = 64 * 1024 * 1024
MAX_WAV_BYTES = MAX_PCM_BYTES + 1024 * 1024
MAX_METADATA_BYTES = 1024 * 1024
MAX_TEXT_BYTES = 64 * 1024
OUTPUT_FORMAT = "wav_24000"


def _json_bytes(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False,
                      allow_nan=False, separators=(",", ":")).encode("utf-8")


def _digest(data):
    return hashlib.sha256(data).hexdigest()


def _read(path, limit):
    with Path(path).open("rb") as source:
        data = source.read(limit + 1)
    if len(data) > limit:
        raise ValueError(f"file exceeds {limit} bytes: {path}")
    return data


def _read_json(path):
    def unique_pairs(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON field: {key}")
            result[key] = value
        return result
    return json.loads(_read(path, MAX_METADATA_BYTES), object_pairs_hook=unique_pairs)


def _text(value, field, *, empty=False):
    if (not isinstance(value, str) or "\0" in value
            or (not empty and not value.strip())
            or len(value.encode("utf-8")) > MAX_TEXT_BYTES):
        raise ValueError(f"{field} must be bounded UTF-8 text without NUL")
    return value


def _number(value, field):
    try:
        valid = type(value) in (int, float) and math.isfinite(value)
    except OverflowError:
        valid = False
    if not valid:
        raise ValueError(f"{field} must be a finite number")
    return value


def _speech_request(value):
    required = {"provider", "voice_id", "model_id", "text", "voice_settings"}
    optional = {"language_code", "previous_text", "next_text",
                "pronunciation_dictionary_locators"}
    if not isinstance(value, dict) or not required <= value.keys() or value.keys() - required - optional:
        raise ValueError("speech request has missing or unsupported fields")
    if value["provider"] != "elevenlabs":
        raise ValueError("the only supported synthesis provider is elevenlabs")
    request = dict(value)
    for key in ("voice_id", "model_id"):
        if not isinstance(request[key], str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", request[key]):
            raise ValueError(f"{key} must be an explicit provider identifier")
    _text(request["text"], "text")
    settings = request["voice_settings"]
    fields = {"stability", "similarity_boost", "style", "use_speaker_boost", "speed"}
    if not isinstance(settings, dict) or settings.keys() != fields:
        raise ValueError("voice_settings must explicitly specify all five supported settings")
    for key in ("stability", "similarity_boost", "style"):
        if not 0 <= _number(settings[key], key) <= 1:
            raise ValueError(f"{key} must be between zero and one")
    if type(settings["use_speaker_boost"]) is not bool:
        raise ValueError("use_speaker_boost must be a boolean")
    if _number(settings["speed"], "speed") <= 0:
        raise ValueError("speed must be positive; provider-specific limits still apply")
    if "language_code" in request:
        if not isinstance(request["language_code"], str) or not re.fullmatch(r"[a-z]{2}", request["language_code"]):
            raise ValueError("language_code must be a two-letter lowercase ISO 639-1 code")
        if request["model_id"] == "eleven_multilingual_v2":
            raise ValueError("eleven_multilingual_v2 does not support language_code")
    for key in ("previous_text", "next_text"):
        if key in request:
            _text(request[key], key, empty=True)
    if "pronunciation_dictionary_locators" in request:
        locators = request["pronunciation_dictionary_locators"]
        if not isinstance(locators, list) or not 1 <= len(locators) <= 3:
            raise ValueError("pronunciation_dictionary_locators requires one to three versioned dictionaries")
        for locator in locators:
            if not isinstance(locator, dict) or locator.keys() != {"pronunciation_dictionary_id", "version_id"}:
                raise ValueError("each pronunciation dictionary needs pronunciation_dictionary_id and version_id")
            for key, item in locator.items():
                if not isinstance(item, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", item):
                    raise ValueError(f"{key} must be an explicit provider identifier")
    request["output_format"] = OUTPUT_FORMAT
    return request


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def _elevenlabs_wav(request, *, opener=None):
    # Read a credential only when this explicit provider request misses cache.
    api_key = os.environ.get("ELEVENLABS_API_KEY")
    if not api_key:
        raise ValueError("ELEVENLABS_API_KEY is required for an uncached synthesis request")
    if len(api_key) > 4096 or any(not 33 <= ord(character) <= 126 for character in api_key):
        raise ValueError("ELEVENLABS_API_KEY must contain only printable ASCII without whitespace")
    url = f"https://api.elevenlabs.io/v1/text-to-speech/{request['voice_id']}?output_format={OUTPUT_FORMAT}"
    body = {key: value for key, value in request.items()
            if key not in {"provider", "voice_id", "output_format"}}
    outgoing = urllib.request.Request(url, _json_bytes(body), method="POST",
        headers={"xi-api-key": api_key, "Content-Type": "application/json", "Accept": "audio/wav"})
    opener = opener or urllib.request.build_opener(_NoRedirect())
    try:
        with opener.open(outgoing, timeout=60) as response:
            payload = response.read(MAX_WAV_BYTES + 1)
    except urllib.error.HTTPError as error:
        # Provider bodies may contain input text; credentials and bodies are not logged.
        error.close()
        raise ValueError(f"ElevenLabs synthesis failed with HTTP {error.code}") from None
    except (urllib.error.URLError, TimeoutError, OSError, http.client.HTTPException, ValueError):
        raise ValueError("ElevenLabs synthesis failed before a complete response") from None
    if len(payload) > MAX_WAV_BYTES:
        raise ValueError("provider WAV response exceeds the preparation limit")
    return payload


def _decode_wav(payload):
    if len(payload) > MAX_WAV_BYTES:
        raise ValueError("WAV exceeds the preparation limit")
    # wave tolerates an incomplete outer RIFF and partial frames. This import
    # accepts a complete file with one data chunk, not a streaming WAV header.
    if (len(payload) < 12 or payload[:4] != b"RIFF" or payload[8:12] != b"WAVE"
            or struct.unpack_from("<I", payload, 4)[0] + 8 != len(payload)):
        raise ValueError("invalid or truncated WAV container")
    offset, data_sizes = 12, []
    while offset < len(payload):
        if offset + 8 > len(payload):
            raise ValueError("truncated WAV chunk header")
        size = struct.unpack_from("<I", payload, offset + 4)[0]
        if payload[offset:offset + 4] == b"data":
            data_sizes.append(size)
        offset += 8 + size + (size % 2)
        if offset > len(payload):
            raise ValueError("truncated WAV chunk")
    if len(data_sizes) != 1:
        raise ValueError("WAV must contain one PCM data chunk")
    try:
        with wave.open(io.BytesIO(payload), "rb") as source:
            rate, channels, frames = source.getframerate(), source.getnchannels(), source.getnframes()
            if source.getcomptype() != "NONE" or source.getsampwidth() != 2:
                raise ValueError("only uncompressed 16-bit PCM WAV is supported")
            if not 8_000 <= rate <= 192_000 or channels not in (1, 2):
                raise ValueError("WAV requires 8–192 kHz mono or stereo audio")
            if frames <= 0 or frames * channels * 4 > MAX_PCM_BYTES:
                raise ValueError("decoded PCM must be nonempty and at most 64 MiB")
            if data_sizes[0] != frames * channels * 2:
                raise ValueError("WAV contains an incomplete PCM frame")
            raw = source.readframes(frames)
            if len(raw) != frames * channels * 2:
                raise ValueError("truncated WAV sample data")
    except (wave.Error, EOFError, struct.error):
        raise ValueError("invalid or unsupported WAV") from None
    samples = array("h")
    samples.frombytes(raw)
    if sys.byteorder != "little":
        samples.byteswap()
    pcm = array("f", (sample / 32768 for sample in samples))
    if sys.byteorder != "little":
        pcm.byteswap()
    return pcm.tobytes(), rate, channels


def _audio_metadata(pcm, rate, channels):
    if type(rate) is not int or not 8_000 <= rate <= 192_000 or type(channels) is not int or channels not in (1, 2):
        raise ValueError("invalid cached audio format")
    if not pcm or len(pcm) > MAX_PCM_BYTES or len(pcm) % (4 * channels):
        raise ValueError("invalid cached PCM length")
    frames = len(pcm) // (4 * channels)
    identity = hashlib.sha256(b"noon-narration-audio-v1\0" + struct.pack("<IH", rate, channels))
    identity.update(pcm)
    return {"format": "noon-narration-audio-v1", "encoding": "f32le",
            "sample_rate": rate, "channels": channels, "frames": frames,
            "duration_seconds": frames / rate, "sha256": _digest(pcm),
            "asset_id": identity.hexdigest()}


def _load_audio(cache, asset_id):
    if not isinstance(asset_id, str) or not re.fullmatch(r"[0-9a-f]{64}", asset_id):
        raise ValueError("invalid cached audio identity")
    directory = cache / "audio" / asset_id
    metadata = _read_json(directory / "audio.json")
    pcm = _read(directory / "audio.f32le", MAX_PCM_BYTES)
    if not isinstance(metadata, dict):
        raise ValueError("invalid cached audio metadata")
    expected = _audio_metadata(pcm, metadata.get("sample_rate"), metadata.get("channels"))
    if _json_bytes(metadata) != _json_bytes(expected) or expected["asset_id"] != asset_id:
        raise ValueError("cached audio failed integrity verification")
    return metadata


def _publish_json(path, value):
    """Publish a complete immutable file without replacing another writer's file."""
    payload = _json_bytes(value) + b"\n"
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".prepare-", delete=False) as temporary:
        temporary_path = Path(temporary.name)
        try:
            temporary.write(payload)
            temporary.flush()
            os.fsync(temporary.fileno())
        except BaseException:
            temporary_path.unlink(missing_ok=True)
            raise
    try:
        try:
            os.link(temporary_path, path)
        except FileExistsError:
            if _read(path, MAX_METADATA_BYTES) != payload:
                raise ValueError(f"existing immutable metadata is inconsistent: {path}") from None
    finally:
        temporary_path.unlink(missing_ok=True)


def _store_audio(cache, pcm, rate, channels):
    metadata = _audio_metadata(pcm, rate, channels)
    parent = cache / "audio"
    parent.mkdir(parents=True, exist_ok=True)
    destination = parent / metadata["asset_id"]
    if destination.exists():
        return _load_audio(cache, metadata["asset_id"])
    with tempfile.TemporaryDirectory(dir=parent, prefix=".prepare-") as temporary:
        stage = Path(temporary)
        for name, payload in (("audio.f32le", pcm), ("audio.json", _json_bytes(metadata) + b"\n")):
            with (stage / name).open("xb") as output:
                output.write(payload)
                output.flush()
                os.fsync(output.fileno())
        try:
            os.rename(stage, destination)
        except OSError:
            if not destination.exists():
                raise
            # A concurrent publisher must have left the same complete asset.
            return _load_audio(cache, metadata["asset_id"])
    return metadata


def _manual_cues(cues, duration=None):
    if not isinstance(cues, list) or len(cues) > 1000:
        raise ValueError("cues must be a list of at most 1000 manual intervals")
    names, previous = set(), -1
    for cue in cues:
        if not isinstance(cue, dict) or cue.keys() != {"name", "start_seconds", "end_seconds", "provenance"}:
            raise ValueError("cue fields must be name, start_seconds, end_seconds, provenance")
        name = _text(cue["name"], "cue name")
        if len(name.encode("utf-8")) > 128:
            raise ValueError("cue names must be at most 128 UTF-8 bytes")
        start = _number(cue["start_seconds"], "cue start")
        end = _number(cue["end_seconds"], "cue end")
        if name in names or cue["provenance"] != "manual":
            raise ValueError("cue names must be unique and timing provenance must be manual")
        if not 0 <= start < end or start < previous or (duration is not None and end > duration):
            raise ValueError("cue intervals must be ordered and within measured audio duration")
        names.add(name)
        previous = start
    return cues


def prepare_narration(cache_dir, *, wav_path=None, spoken_text=None, request=None,
                      caption_text=None, cues=None, opener=None):
    """Return a persisted manifest path; all cache writes are immutable.

    ``opener`` is an injectable HTTP boundary for offline provider tests, not an
    endpoint override. Imported transcripts and provider inputs are unverified
    against the audio; manual cue metadata does not claim forced alignment.
    """
    if (wav_path is None) == (request is None):
        raise ValueError("choose exactly one WAV import or synthesis request")
    if wav_path is not None:
        _text(spoken_text, "spoken_text")
        wav = _read(wav_path, MAX_WAV_BYTES)
        audio_request = {"provider": "import-wav", "source_sha256": _digest(wav)}
        transcript_status = "supplied-unverified"
    else:
        if spoken_text is not None:
            raise ValueError("synthesis spoken text belongs in the request")
        audio_request = _speech_request(request)
        spoken_text = audio_request["text"]
        transcript_status = "provider-input-unverified"
    caption_text = spoken_text if caption_text is None else _text(caption_text, "caption_text", empty=True)
    cues = _manual_cues([] if cues is None else cues)
    cache = Path(cache_dir).resolve()
    # Reject known-invalid destinations before a potentially paid request. A
    # dangling receipt is an occupied corrupt entry, never a synthesis cache miss.
    for name in ("audio", "requests", "narrations"):
        directory = cache / name
        if directory.is_symlink():
            raise ValueError(f"cache directory must not be a symlink: {name}")
        directory.mkdir(parents=True, exist_ok=True)
    request_id = _digest(_json_bytes(audio_request))
    receipt_path = cache / "requests" / f"{request_id}.json"
    if receipt_path.is_symlink():
        raise ValueError("cached request receipt must not be a symlink")
    if receipt_path.exists():
        receipt = _read_json(receipt_path)
        if (not isinstance(receipt, dict) or receipt.keys() != {"format", "request", "audio_id"}
                or receipt["format"] != "noon-narration-request-v1"
                or _json_bytes(receipt["request"]) != _json_bytes(audio_request)):
            raise ValueError("cached synthesis/import receipt is inconsistent")
        audio = _load_audio(cache, receipt["audio_id"])
    else:
        if request is not None:
            wav = _elevenlabs_wav(audio_request, opener=opener)
        pcm, rate, channels = _decode_wav(wav)
        if request is not None and rate != 24_000:
            raise ValueError("provider response does not match requested wav_24000 sample rate")
        audio = _store_audio(cache, pcm, rate, channels)
        _publish_json(receipt_path, {"format": "noon-narration-request-v1",
                                    "request": audio_request, "audio_id": audio["asset_id"]})
    _manual_cues(cues, audio["duration_seconds"])
    manifest = {"format": "noon-narration-v1", "audio": dict(audio,
                path=f"../audio/{audio['asset_id']}/audio.f32le"),
                "request_id": request_id, "spoken_text": spoken_text,
                "transcript_status": transcript_status, "caption_text": caption_text,
                "cues": cues}
    manifest_path = cache / "narrations" / f"{_digest(_json_bytes(manifest))}.json"
    _publish_json(manifest_path, manifest)
    return manifest_path


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--wav", type=Path, help="import an uncompressed 16-bit PCM WAV")
    source.add_argument("--request", type=Path, help="explicit ElevenLabs synthesis request JSON")
    parser.add_argument("--spoken-text-file", type=Path, help="required supplied transcript for WAV import")
    parser.add_argument("--caption-text-file", type=Path, help="displayed text; defaults to spoken text")
    parser.add_argument("--cues", type=Path, help="JSON list of manually timed named intervals")
    parser.add_argument("--cache-dir", type=Path, required=True)
    args = parser.parse_args(argv)
    if bool(args.wav) != bool(args.spoken_text_file):
        parser.error("--spoken-text-file is required with --wav and unavailable with --request")
    try:
        result = prepare_narration(args.cache_dir, wav_path=args.wav,
            spoken_text=_read(args.spoken_text_file, MAX_TEXT_BYTES).decode("utf-8") if args.spoken_text_file else None,
            request=_read_json(args.request) if args.request else None,
            caption_text=_read(args.caption_text_file, MAX_TEXT_BYTES).decode("utf-8") if args.caption_text_file else None,
            cues=_read_json(args.cues) if args.cues else None)
    except (OSError, ValueError, UnicodeError) as error:
        parser.exit(1, f"narration preparation failed: {error}\n")
    print(result)


if __name__ == "__main__":
    main()
