#!/usr/bin/env python3
"""Run actual native glow readback with a real-shader negative control.

This is correctness qualification through ordinary runtime tracks and publication,
not a performance benchmark or full public Scene orchestration test.
Compilation/setup failures cannot satisfy the negative control. Every attempt is
retained; no retry-until-green, ignored failure, or zero-test success is accepted.
A separate #2094 synthetic sample-storage probe reuses this runner and artifact
bundle. Its success is not production-renderer or cache-lifecycle qualification.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import subprocess
import time
import unittest
from pathlib import Path

from mixed_image_evidence import MIXED_IMAGE_TEST, MIXED_IMAGE_MARKER, mixed_image_evidence_error
from mixed_cache_evidence import MIXED_TEXT_TEST, MIXED_TEXT_MARKER, evidence_error as mixed_text_evidence_error

TEST = "gpu::glow_filter::tests::pixels::native_gaussian_pixels_and_retained_updates"
PAINTER_TEST = "gpu::glow_filter::tests::analytic_scene::retained_painter_glow_pixels"
SEMANTIC_TEST = "gpu::glow_filter::tests::analytic_scene::semantic_static_glow_publication_pixels"
ANIMATED_TEST = "gpu::glow_filter::tests::animated_scene::animated_glow_publication_pixels"
LIVE_TEST = "gpu::glow_filter::tests::animated_scene::live_glow_value_publication_pixels"
HOST_TEST = "gpu::glow_filter::tests::retained_host::automatic_retained_host_glow_pixels"
CACHE_TARGET = "rendered_content_cache_msaa"
CACHE_TEST = "rendered_content_cache_msaa_operator"
GEOMETRY_TEST = "gpu::presentation::prefix_restore_tests::ordered_prefix_sample_restore"
PASS = re.compile(r"test result: ok\. 1 passed; 0 failed; 0 ignored;")
FAIL = re.compile(r"test result: FAILED\. 0 passed; 1 failed; 0 ignored;")
ORIGINAL = "let bits = u32(round(clamp(value, 0.0, 1.0) * 16777215.0));"
MUTATION = "let bits = u32(round(clamp(value, 0.0, 1.0) * 255.0)) * 65793u;"


def qualified(code: int, log: str, *, negative: bool = False, test: str = TEST) -> bool:
    """Reject empty filters, compile/setup errors, and unrelated test failures."""
    if negative:
        return (
            code == 101
            and FAIL.search(log) is not None
            and test in log
            and "Gaussian mask exceeds frozen tolerance:" in log
        )
    accepted = code == 0 and PASS.search(log) is not None and test in log
    if test == CACHE_TEST:
        accepted = accepted and re.search(r"(?m)^CACHE_MSAA_OPERATOR_PASS\r?$", log) is not None
    if test == GEOMETRY_TEST:
        accepted = accepted and re.search(r"(?m)^CACHE_GEOMETRY_PROBE_PASS\r?$", log) is not None
    if test == MIXED_IMAGE_TEST:
        accepted = accepted and re.search(r"(?m)^" + MIXED_IMAGE_MARKER + r"\r?$", log) is not None
    if test == MIXED_TEXT_TEST:
        accepted = accepted and re.search(r"(?m)^" + MIXED_TEXT_MARKER + r"\r?$", log) is not None
    return accepted


def sha256(content: bytes) -> str:
    return hashlib.sha256(content).hexdigest()


def cache_evidence_cases():
    """Fixed inventory, not names supplied by a candidate report."""
    samples, resolved = 256 * 8 * 4 * 4, 256 * 8 * 4
    cases = [("seed-samples", samples, True)]
    cases += [(f"round-{i}-samples", samples, True) for i in range(8)]
    for backdrop in range(4):
        for step in range(4):
            cases += [(f"b{backdrop}-s{step}-resolved", resolved, True),
                      (f"b{backdrop}-s{step}-samples", samples, True)]
        cases.append((f"b{backdrop}-abandoned-samples", samples, True))
        cases += [(f"b{backdrop}-{defect}", resolved, False)
                  for defect in ("permuted", "averaged", "accumulated")]
    return cases


def cache_evidence_error(directory: Path) -> str | None:
    """Recheck every byte/mask independently of Rust assertions and pass logs."""
    try:
        if not (directory / "PASS.txt").read_text().startswith("CACHE_MSAA_OPERATOR_PASS:"):
            return "missing operator completion marker"
        if "samples=4" not in (directory / "profile.txt").read_text():
            return "missing four-sample profile"
        readbacks = {}
        for name, size, equal in cache_evidence_cases():
            expected = (directory / f"{name}-expected.rgba").read_bytes()
            actual = (directory / f"{name}-actual.rgba").read_bytes()
            mask = (directory / f"{name}-diff.mask").read_bytes()
            if len(expected) != size or len(actual) != size or len(mask) != size // 4:
                return f"{name}: incomplete readback or mask"
            if (actual == expected) != equal:
                return f"{name}: exact comparison or negative control failed"
            recomputed = bytes(0 if expected[i:i + 4] == actual[i:i + 4] else 255
                               for i in range(0, size, 4))
            if mask != recomputed:
                return f"{name}: incorrect differing-pixel mask"
            readbacks[name] = (expected, actual)
        # Integer code expectation is separate from both WGSL and Rust readback.
        codes = bytearray()
        for y in range(8):
            for x in range(256):
                for sample in range(4):
                    alpha = (x + sample * 37 + y * 13) % 256
                    codes.extend((alpha, x & alpha, sample * 17 % (alpha + 1), alpha))
        if readbacks["seed-samples"][1] != codes:
            return "seed sample codes disagree with independent byte reference"
        # Pairwise equality alone lets two identically wrong files pass. Bind
        # every restoration round to the original independently computed bytes.
        for round_index in range(8):
            name = f"round-{round_index}-samples"
            if readbacks[name][1] != codes:
                return f"{name}: sample restoration differs from independent byte reference"
        for backdrop in range(4):
            for representation in ("samples", "resolved"):
                original = readbacks[f"b{backdrop}-s0-{representation}"][1]
                for step in (2, 3):
                    name = f"b{backdrop}-s{step}-{representation}"
                    if readbacks[name][1] != original:
                        return f"{name}: repeated suffix changed with history"
            ordinary = readbacks[f"b{backdrop}-s3-resolved"][1]
            for defect in ("permuted", "averaged", "accumulated"):
                name = f"b{backdrop}-{defect}"
                if readbacks[name][0] != ordinary:
                    return f"{name}: negative control uses a different ordinary reference"
            prefix = readbacks[f"b{backdrop}-abandoned-samples"][1]
            for step in range(4):
                name = f"b{backdrop}-s{step}-samples"
                samples = readbacks[name][1]
                # Row 3 has exactly opaque red/blue sources. Endpoint codes
                # need no assumption about intermediate UNORM rounding, and
                # catch blank output or lost/permuted coverage independently.
                if step != 1:
                    for x in range(256):
                        for sample in range(4):
                            offset = (3 * 256 + x) * 16 + sample * 4
                            if (x >> 4) & (1 << sample):
                                color = bytes((0, 0, 255, 255))
                            elif x & (1 << sample):
                                color = bytes((255, 0, 0, 255))
                            else:
                                continue
                            if samples[offset:offset + 4] != color:
                                return f"{name}: opaque sample witness failed at x={x} sample={sample}"
                # The suffix cannot affect samples outside its explicit mask;
                # compare those bytes to the separately retained prefix. This
                # also covers the complementary suffix and every alpha row.
                for pixel in range(256 * 8):
                    mask = (pixel % 256) >> 4
                    if step == 1:
                        mask ^= 15
                    for sample in range(4):
                        offset = pixel * 16 + sample * 4
                        if not mask & (1 << sample):
                            if samples[offset:offset + 4] != prefix[offset:offset + 4]:
                                return f"{name}: uncovered sample changed at pixel={pixel} sample={sample}"
    except (OSError, UnicodeError) as error:
        return f"missing or unreadable probe evidence: {error}"
    return None


def geometry_evidence_cases():
    """Fixed readback inventory for the real-geometry prefix experiment."""
    for profile in range(4):
        width, height = (96, 64) if profile < 2 else (127, 81)
        pixels = width * height * 4
        for step in range(4):
            for representation in ("ordinary", "fresh", "resolved", "samples"):
                yield (f"p{profile}-s{step}-{representation}",
                       pixels * (4 if representation == "samples" else 1), True)
        for representation in ("resolved", "samples"):
            yield (f"p{profile}-rebuilt-{representation}",
                   pixels * (4 if representation == "samples" else 1), True)
        for defect in ("accumulated", "reordered", "stale-prefix"):
            yield f"p{profile}-{defect}", pixels, False


def geometry_evidence_error(directory: Path) -> str | None:
    """Check evidence integrity/history, not an independent geometry rasterizer."""
    try:
        if not (directory / "PASS.txt").read_text().startswith("CACHE_GEOMETRY_PROBE_PASS:"):
            return "missing geometry completion marker"
        if "samples=4 transfer=Direct" not in (directory / "profile.txt").read_text():
            return "missing geometry sample/transfer profile"
        readbacks = {}
        for name, size, equal in geometry_evidence_cases():
            expected = (directory / f"{name}-expected.rgba").read_bytes()
            actual = (directory / f"{name}-actual.rgba").read_bytes()
            mask = (directory / f"{name}-diff.mask").read_bytes()
            if len(expected) != size or len(actual) != size or len(mask) != size // 4:
                return f"{name}: incomplete geometry readback or mask"
            if (expected == actual) != equal:
                return f"{name}: geometry comparison or negative control failed"
            calculated = bytes(0 if expected[i:i + 4] == actual[i:i + 4] else 255
                               for i in range(0, size, 4))
            if mask != calculated:
                return f"{name}: incorrect geometry difference mask"
            readbacks[name] = expected, actual
        for profile in range(4):
            for step in range(4):
                ordinary = readbacks[f"p{profile}-s{step}-ordinary"][0]
                if not any(ordinary[i:i + 4] != ordinary[:4]
                           for i in range(4, len(ordinary), 4)):
                    return f"p{profile}-s{step}: blank ordinary geometry"
                for representation in ("fresh", "resolved"):
                    name = f"p{profile}-s{step}-{representation}"
                    if readbacks[name][0] != ordinary:
                        return f"{name}: different ordinary geometry reference"
                samples = readbacks[f"p{profile}-s{step}-samples"][0]
                if not any(samples[i:i + 4] != samples[i + s:i + s + 4]
                           for i in range(0, len(samples), 16) for s in (4, 8, 12)):
                    return f"p{profile}-s{step}: no multisample coverage witness"
            for representation in ("resolved", "samples"):
                original = readbacks[f"p{profile}-s0-{representation}"][1]
                for step in (2, 3):
                    name = f"p{profile}-s{step}-{representation}"
                    if readbacks[name][1] != original:
                        return f"{name}: geometry history drift"
            ordinary = readbacks[f"p{profile}-s3-ordinary"][0]
            for defect in ("accumulated", "reordered"):
                name = f"p{profile}-{defect}"
                if readbacks[name][0] != ordinary:
                    return f"{name}: different negative-control reference"
            rebuilt = readbacks[f"p{profile}-rebuilt-resolved"][0]
            if readbacks[f"p{profile}-stale-prefix"][0] != rebuilt or rebuilt == ordinary:
                return f"p{profile}: missing changed-prefix reference"
    except (OSError, UnicodeError) as error:
        return f"missing or unreadable geometry evidence: {error}"
    return None


def run_stage(root: Path, output: Path, name: str, negative: bool, *, test: str = TEST,
              integration: str | None = None) -> dict:
    target = ["--lib"] if integration is None else ["--test", integration]
    command = [
        "cargo", "test", "-p", "noon-render-wgpu", *target,
        test, "--", "--ignored", "--exact", "--nocapture",
    ]
    env = os.environ.copy()
    artifacts = None
    if test in (CACHE_TEST, GEOMETRY_TEST, MIXED_IMAGE_TEST, MIXED_TEXT_TEST):
        # A fresh attempt directory prevents stale success or readbacks from a
        # previous attempt being reported as evidence for this invocation.
        prefix = {CACHE_TEST: "cache-msaa", GEOMETRY_TEST: "cache-geometry",
                  MIXED_IMAGE_TEST: "cache-mixed-image", MIXED_TEXT_TEST: "cache-mixed-text"}[test]
        artifacts = output / f"{prefix}-{time.time_ns()}"
        artifacts.mkdir()
        env["NOON_CACHE_PROBE_ARTIFACTS"] = str(artifacts)
        env["NOON_CACHE_PROBE_BACKEND"] = env.get(
            "NOON_CACHE_PROBE_BACKEND", env.get("NOON_GLOW_BACKEND", "vulkan"))
    started = time.monotonic()
    # Do not import a shell environment from report files or interpret log text.
    try:
        process = subprocess.run(
            command, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, encoding="utf-8", errors="replace", check=False, timeout=600,
            env=env,
        )
        code, log = process.returncode, process.stdout
    except subprocess.TimeoutExpired as error:
        partial = error.stdout or b""
        log = partial.decode("utf-8", "replace") if isinstance(partial, bytes) else partial
        code, log = -1, log + "\nqualification process timed out\n"
    except OSError as error:
        code, log = -1, f"qualification could not launch cargo: {error}\n"
    (output / f"{name}.log").write_text(log, encoding="utf-8")
    print(log, end="", flush=True)
    record = {
        "name": name,
        "command": command,
        "return_code": code,
        "elapsed_seconds": time.monotonic() - started,
        "negative_control": negative,
        "accepted": qualified(code, log, negative=negative, test=test),
        "log_sha256": sha256(log.encode()),
    }
    if artifacts is not None:
        record["artifact_directory"] = artifacts.name
        record["artifact_sha256"] = {
            str(path.relative_to(artifacts)): sha256(path.read_bytes())
            for path in sorted(artifacts.rglob("*")) if path.is_file()
        }
        validator = {CACHE_TEST: cache_evidence_error, GEOMETRY_TEST: geometry_evidence_error,
                     MIXED_IMAGE_TEST: mixed_image_evidence_error,
                     MIXED_TEXT_TEST: mixed_text_evidence_error}[test]
        record["evidence_error"] = validator(artifacts)
        record["accepted"] = record["accepted"] and record["evidence_error"] is None
    (output / f"{name}.json").write_text(json.dumps(record, indent=2) + "\n")
    return record


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report-dir", type=Path, required=True)
    args = parser.parse_args()
    output = args.report_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parents[2]
    shader = root / "crates/noon-render-wgpu/src/gpu/glow_filter.wgsl"
    original = shader.read_bytes()
    text = original.decode("utf-8")
    if text.count(ORIGINAL) != 1:
        raise SystemExit("negative-control shader anchor must match exactly once")
    mutant = text.replace(ORIGINAL, MUTATION).encode()
    records = []
    report = {
        "schema": 1,
        "scope": "native raster operator, painter, static, animated, live value and automatic host preparation; not full Scene orchestration or physical performance",
        "source_shader_sha256": sha256(original),
        "mutant_shader_sha256": sha256(mutant),
        "stages": records,
        "cache_probe": {"scope": "#2094 synthetic 4x sample-storage operator only", "accepted": False},
        "geometry_probe": {"scope": "#2094 production geometry draw experiment only", "accepted": False},
        "mixed_image_probe": {"scope": "#2094 image/vector experiment only", "accepted": False},
        "mixed_text_probe": {"scope": "#2094 image/glyph/vector experiment only", "accepted": False},
        "passed": False,
    }
    try:
        checker_log = io.StringIO()
        suite = unittest.defaultTestLoader.discover(
            str(root / "tests/visual-effects"), pattern="test_mixed_image_evidence.py")
        checked = unittest.TextTestRunner(stream=checker_log, verbosity=2).run(suite)
        (output / "mixed-image-checker-tests.log").write_text(checker_log.getvalue())
        report["mixed_image_checker_tests"] = {"count": checked.testsRun,
                                              "accepted": checked.testsRun == 8 and checked.wasSuccessful()}
        if not report["mixed_image_checker_tests"]["accepted"]:
            raise RuntimeError("mixed-image checker regressions did not execute and pass")
        text_checker_log = io.StringIO()
        text_suite = unittest.defaultTestLoader.discover(
            str(root / "tests/visual-effects"), pattern="test_mixed_cache_evidence.py")
        text_checked = unittest.TextTestRunner(stream=text_checker_log, verbosity=2).run(text_suite)
        (output / "mixed-text-checker-tests.log").write_text(text_checker_log.getvalue())
        report["mixed_text_checker_tests"] = {
            "count": text_checked.testsRun,
            "accepted": text_checked.testsRun == 13 and text_checked.wasSuccessful(),
        }
        if not report["mixed_text_checker_tests"]["accepted"]:
            raise RuntimeError("mixed-text checker regressions did not execute and pass")
        baseline = run_stage(root, output, "published-source", False)
        records.append(baseline)
        if not baseline["accepted"]:
            raise RuntimeError("published-source GPU test did not pass exactly one test")
        try:
            shader.write_bytes(mutant)
            negative = run_stage(root, output, "rgb8-negative-control", True)
            records.append(negative)
        finally:
            # Restore byte-for-byte even when build, driver, or timeout fails.
            shader.write_bytes(original)
        restored = run_stage(root, output, "restored-source", False)
        records.append(restored)
        if restored["accepted"]:
            painter = run_stage(root, output, "retained-painter", False, test=PAINTER_TEST)
            records.append(painter)
            if painter["accepted"]:
                semantic = run_stage(root, output, "static-semantic-publication", False, test=SEMANTIC_TEST)
                records.append(semantic)
                if semantic["accepted"]:
                    animated = run_stage(root, output, "animated-runtime-publication", False, test=ANIMATED_TEST)
                    records.append(animated)
                    if animated["accepted"]:
                        live = run_stage(root, output, "live-value-publication", False, test=LIVE_TEST)
                        records.append(live)
                        if live["accepted"]:
                            host = run_stage(root, output, "automatic-retained-host", False, test=HOST_TEST)
                            records.append(host)
        report["glow_passed"] = all(record["accepted"] for record in records) and len(records) == 8
        if not report["glow_passed"]:
            raise RuntimeError("glow GPU qualification or real-shader negative control failed")
        source_hashes = {}
        for extension in ("rs", "wgsl"):
            path = root / f"crates/noon-render-wgpu/tests/{CACHE_TARGET}.{extension}"
            content = path.read_bytes()
            (output / path.name).write_bytes(content)
            source_hashes[str(path.relative_to(root))] = sha256(content)
        cache = run_stage(root, output, "cache-msaa-operator", False,
                          test=CACHE_TEST, integration=CACHE_TARGET)
        cache["scope"] = "#2094 synthetic 4x sample-storage operator only"
        cache["source_sha256"] = source_hashes
        cache["source_unchanged"] = all(sha256((root / path).read_bytes()) == digest
                                        for path, digest in source_hashes.items())
        lock = root / "Cargo.lock"
        cache["cargo_lock_sha256"] = sha256(lock.read_bytes()) if lock.is_file() else None
        if lock.is_file():
            (output / "cache-probe-Cargo.lock").write_bytes(lock.read_bytes())
        report["cache_probe"] = cache
        if not (cache["accepted"] and cache["source_unchanged"]):
            raise RuntimeError("#2094 sample-storage operator probe failed; not a passing skip")
        geometry_sources = (
            "crates/noon-render-wgpu/src/gpu/presentation/prefix_restore_tests.rs",
            "crates/noon-render-wgpu/src/gpu/presentation.rs",
            "tests/visual-effects/qualify_gpu_operator.py",
        )
        geometry_hashes = dict(source_hashes)
        for source in geometry_sources:
            content = (root / source).read_bytes()
            (output / Path(source).name).write_bytes(content)
            geometry_hashes[source] = sha256(content)
        geometry = run_stage(root, output, "cache-production-geometry", False, test=GEOMETRY_TEST)
        geometry["scope"] = "#2094 production geometry draw experiment only; no automatic caching"
        geometry["source_sha256"] = geometry_hashes
        geometry["source_unchanged"] = all(sha256((root / path).read_bytes()) == digest
                                           for path, digest in geometry_hashes.items())
        geometry["cargo_lock_sha256"] = sha256(lock.read_bytes()) if lock.is_file() else None
        if lock.is_file():
            (output / "cache-geometry-Cargo.lock").write_bytes(lock.read_bytes())
        report["geometry_probe"] = geometry
        report["geometry_passed"] = geometry["accepted"] and geometry["source_unchanged"]
        if not report["geometry_passed"]:
            raise RuntimeError("#2094 production geometry prefix probe failed; not a passing skip")
        image_sources = (
            "crates/noon-render-wgpu/src/gpu/presentation/prefix_restore_tests/mixed_images.rs",
            "crates/noon-render-wgpu/src/gpu/retained_text.rs",
            "crates/noon-render-wgpu/src/gpu/raster_image_gpu.rs",
            "crates/noon-render-wgpu/src/gpu/raster_image_prepare.rs",
            "tests/visual-effects/mixed_image_evidence.py",
            "tests/visual-effects/test_mixed_image_evidence.py",
        )
        image_hashes = dict(geometry_hashes)
        for source in image_sources:
            content = (root / source).read_bytes()
            (output / Path(source).name).write_bytes(content)
            image_hashes[source] = sha256(content)
        mixed = run_stage(root, output, "cache-mixed-image", False, test=MIXED_IMAGE_TEST)
        mixed["scope"] = "#2094 image/vector draw experiment; not glyph/text or automatic caching"
        mixed["source_sha256"] = image_hashes
        mixed["source_unchanged"] = all(sha256((root / path).read_bytes()) == digest
                                        for path, digest in image_hashes.items())
        mixed["cargo_lock_sha256"] = sha256(lock.read_bytes()) if lock.is_file() else None
        if lock.is_file():
            (output / "cache-mixed-image-Cargo.lock").write_bytes(lock.read_bytes())
        report["mixed_image_probe"] = mixed
        report["mixed_image_passed"] = mixed["accepted"] and mixed["source_unchanged"]
        if not report["mixed_image_passed"]:
            raise RuntimeError("#2094 mixed image prefix probe failed; not a passing skip")
        text_sources = (
            "crates/noon-render-wgpu/src/gpu/presentation/prefix_restore_tests/mixed.rs",
            "crates/noon-typst/src/lib.rs",
            "tests/visual-effects/mixed_cache_evidence.py",
            "tests/visual-effects/test_mixed_cache_evidence.py",
        )
        text_hashes = dict(image_hashes)
        for source in text_sources:
            content = (root / source).read_bytes()
            # Keep source paths: multiple crates have a lib.rs.
            destination = output / "mixed-text-sources" / source
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(content)
            text_hashes[source] = sha256(content)
        mixed_text = run_stage(root, output, "cache-mixed-text", False, test=MIXED_TEXT_TEST)
        mixed_text["scope"] = "#2094 image/glyph/vector draw experiment; no automatic caching"
        mixed_text["source_sha256"] = text_hashes
        mixed_text["source_unchanged"] = all(sha256((root / path).read_bytes()) == digest
                                             for path, digest in text_hashes.items())
        mixed_text["cargo_lock_sha256"] = sha256(lock.read_bytes()) if lock.is_file() else None
        if lock.is_file():
            (output / "cache-mixed-text-Cargo.lock").write_bytes(lock.read_bytes())
        report["mixed_text_probe"] = mixed_text
        report["passed"] = mixed_text["accepted"] and mixed_text["source_unchanged"]
        if not report["passed"]:
            raise RuntimeError("#2094 mixed text prefix probe failed; not a passing skip")
    finally:
        report["source_restored"] = shader.read_bytes() == original
        (output / "qualification.json").write_text(json.dumps(report, indent=2) + "\n")
        if not report["source_restored"]:
            raise RuntimeError("qualification left modified shader bytes")


if __name__ == "__main__":
    main()
