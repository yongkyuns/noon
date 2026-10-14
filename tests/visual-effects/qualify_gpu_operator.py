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
import json
import os
import re
import subprocess
import time
from pathlib import Path

TEST = "gpu::glow_filter::tests::pixels::native_gaussian_pixels_and_retained_updates"
PAINTER_TEST = "gpu::glow_filter::tests::analytic_scene::retained_painter_glow_pixels"
SEMANTIC_TEST = "gpu::glow_filter::tests::analytic_scene::semantic_static_glow_publication_pixels"
ANIMATED_TEST = "gpu::glow_filter::tests::animated_scene::animated_glow_publication_pixels"
LIVE_TEST = "gpu::glow_filter::tests::animated_scene::live_glow_value_publication_pixels"
HOST_TEST = "gpu::glow_filter::tests::retained_host::automatic_retained_host_glow_pixels"
CACHE_TARGET = "rendered_content_cache_msaa"
CACHE_TEST = "rendered_content_cache_msaa_operator"
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
        # Integer code expectation is separate from both WGSL and Rust readback.
        codes = bytearray()
        for y in range(8):
            for x in range(256):
                for sample in range(4):
                    alpha = (x + sample * 37 + y * 13) % 256
                    codes.extend((alpha, x & alpha, sample * 17 % (alpha + 1), alpha))
        if (directory / "seed-samples-actual.rgba").read_bytes() != codes:
            return "seed sample codes disagree with independent byte reference"
    except (OSError, UnicodeError) as error:
        return f"missing or unreadable probe evidence: {error}"
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
    if test == CACHE_TEST:
        # A fresh attempt directory prevents stale success or readbacks from a
        # previous attempt being reported as evidence for this invocation.
        artifacts = output / f"cache-msaa-{time.time_ns()}"
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
        record["evidence_error"] = cache_evidence_error(artifacts)
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
        "passed": False,
    }
    try:
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
        report["passed"] = cache["accepted"] and cache["source_unchanged"]
        if not report["passed"]:
            raise RuntimeError("#2094 sample-storage operator probe failed; not a passing skip")
    finally:
        report["source_restored"] = shader.read_bytes() == original
        (output / "qualification.json").write_text(json.dumps(report, indent=2) + "\n")
        if not report["source_restored"]:
            raise RuntimeError("qualification left modified shader bytes")


if __name__ == "__main__":
    main()
