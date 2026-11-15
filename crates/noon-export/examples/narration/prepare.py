#!/usr/bin/env python3
"""Optionally reproduce the Double the input lesson's original speech fixtures.

Requires FFmpeg with its Flite filter and built-in slt voice. Creates a NEW output
directory. Phrase boundaries are manual editorial cues from concatenated PCM
frames, not word alignment or listening qualification. Existing Noon preparation
owns all import and cache behavior; this script does not implement a provider.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import wave


RATE = 16_000
PASSAGES = (
    ("positive", (
        "Choose an input of two.",
        "Multiplying by two moves the output to four.",
        "The output is twice the input.",
    ), "x = 2 → y = 4. The output is twice the input.", 0.4),
    ("negative", (
        "Next, try an input of negative two. Predict the output.",
        "Doubling negative two gives negative four.",
        "The sign stays negative, and the magnitude doubles.",
    ), "What output does y = 2x give when x = −2?", 2.0),
    ("zero", (
        "Finally, try an input of zero.",
        "Twice zero is still zero. Both values are zero.",
        "The same rule works for positive, negative, and zero inputs.",
    ), "x = 0 → y = 0. Doubling preserves zero.", 0.4),
)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def phrase(section, text):
    section.mkdir()
    (section / "phrase.txt").write_text(text + "\n", encoding="utf-8")
    command = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i",
               "flite=textfile=phrase.txt:voice=slt", "-c:a", "pcm_s16le", "-n", "phrase.wav"]
    subprocess.run(command, cwd=section, check=True)
    with wave.open(str(section / "phrase.wav"), "rb") as source:
        if (source.getframerate(), source.getnchannels(), source.getsampwidth(),
                source.getcomptype()) != (RATE, 1, 2, "NONE"):
            raise ValueError("Flite output must be 16 kHz mono PCM16 WAV")
        count = source.getnframes()
        pcm = source.readframes(count)
    if not count or len(pcm) != count * 2:
        raise ValueError("Flite output contains empty or incomplete PCM frames")
    return pcm, {"text": text, "frames": count, "command": command,
                 "wav_sha256": sha256(section / "phrase.wav")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_directory", type=Path, help="new directory for recordings and cache")
    args = parser.parse_args()
    root = args.output_directory.resolve()
    root.mkdir(parents=True, exist_ok=False)
    prepare = Path(__file__).resolve().parents[4] / "scripts" / "prepare_narration.py"
    version = subprocess.check_output(["ffmpeg", "-version"], text=True).splitlines()[0]
    summary = []
    for name, texts, caption, pause in PASSAGES:
        section = root / name
        section.mkdir()
        phrases, chunks, layout, cues, frames = [], [], [], [], 0

        def append(label, pcm, kind, cue=True):
            nonlocal frames
            start = frames
            frames += len(pcm) // 2
            chunks.append(pcm)
            layout.append({"name": label, "kind": kind,
                           "start_frame": start, "end_frame": frames})
            if cue:
                cues.append({"name": label, "start_seconds": start / RATE,
                             "end_seconds": frames / RATE, "provenance": "manual"})

        append("lead_in", b"\0\0" * round(0.35 * RATE), "silence", False)
        for key, text in zip(("intro", "move", "inspect"), texts):
            pcm, record = phrase(section / key, text)
            phrases.append(dict(record, name=key, cwd=f"{name}/{key}"))
            append(key, pcm, "generated_phrase")
            if key == "intro":
                append("prediction_hold" if name == "negative" else "before_move",
                       b"\0\0" * round(pause * RATE), "silence", name == "negative")
            elif key == "move":
                append("before_inspection", b"\0\0" * round(0.4 * RATE), "silence", False)
        append("final_hold", b"\0\0" * max(round(0.65 * RATE), 8 * RATE - frames), "silence")
        duration = frames / RATE
        if not 8 <= duration <= 14 or any(
                not 0 <= c["start_seconds"] < c["end_seconds"] <= duration for c in cues):
            raise ValueError("passage duration or manual cue lies outside the expected bounds")
        wav_path = section / f"{name}.wav"
        with wave.open(str(wav_path), "wb") as output:
            output.setparams((1, 2, RATE, 0, "NONE", "not compressed"))
            output.writeframes(b"".join(chunks))
        (section / "spoken.txt").write_text(" ".join(texts) + "\n", encoding="utf-8")
        (section / "captions.txt").write_text(caption + "\n", encoding="utf-8")
        write_json(section / "cues.json", cues)
        command = [sys.executable, "-B", str(prepare), "--wav", str(wav_path),
                   "--spoken-text-file", str(section / "spoken.txt"),
                   "--caption-text-file", str(section / "captions.txt"),
                   "--cues", str(section / "cues.json"), "--cache-dir", str(root / "cache")]
        manifest = Path(subprocess.check_output(command, text=True).strip())
        relative_manifest = str(manifest.relative_to(root))
        write_json(section / "provenance.json", {
            "original_text": True, "synthetic_speech": True, "voice": "Flite slt",
            "ffmpeg_version": version, "sample_rate": RATE, "channels": 1,
            "encoding": "PCM16 WAV", "frames": frames, "duration_seconds": duration,
            "phrases": phrases, "layout": layout, "cues": cues,
            "cue_method": "Manual phrase intervals from exact concatenated PCM frame boundaries",
            "word_alignment": False, "human_listening": False, "voice_quality_verified": False,
            "wav_sha256": sha256(wav_path), "generator_sha256": sha256(Path(__file__)),
            "preparation_command": command, "preparation_script_sha256": sha256(prepare),
            "manifest": relative_manifest, "manifest_sha256": sha256(manifest),
        })
        summary.append({"name": name, "manifest": relative_manifest,
                        "wav": f"{name}/{name}.wav", "duration_seconds": duration, "cues": cues})
        print(manifest)
    write_json(root / "summary.json", summary)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError, wave.Error) as error:
        sys.exit(f"Narration fixture preparation failed: {error}")
