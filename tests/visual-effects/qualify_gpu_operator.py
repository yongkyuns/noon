#!/usr/bin/env python3
"""Run actual native glow readback with a real-shader negative control.

This is correctness qualification, not a performance benchmark or Scene test.
Compilation/setup failures cannot satisfy the negative control. Every attempt is
retained; no retry-until-green, ignored failure, or zero-test success is accepted.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import time
from pathlib import Path

TEST = "gpu::glow_filter::tests::pixels::native_gaussian_pixels_and_retained_updates"
PAINTER_TEST = "gpu::glow_filter::tests::analytic_scene::retained_painter_glow_pixels"
SEMANTIC_TEST = "gpu::glow_filter::tests::analytic_scene::semantic_static_glow_publication_pixels"
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
    return code == 0 and PASS.search(log) is not None and test in log


def sha256(content: bytes) -> str:
    return hashlib.sha256(content).hexdigest()


def run_stage(root: Path, output: Path, name: str, negative: bool, *, test: str = TEST) -> dict:
    command = [
        "cargo", "test", "-p", "noon-render-wgpu", "--lib",
        test, "--", "--ignored", "--exact", "--nocapture",
    ]
    started = time.monotonic()
    # Do not import a shell environment from report files or interpret log text.
    try:
        process = subprocess.run(
            command, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, encoding="utf-8", errors="replace", check=False, timeout=600,
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
        "scope": "native raster operator, painter and static semantic publication; not full Scene orchestration or physical performance",
        "source_shader_sha256": sha256(original),
        "mutant_shader_sha256": sha256(mutant),
        "stages": records,
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
        report["passed"] = all(record["accepted"] for record in records) and len(records) == 5
        if not report["passed"]:
            raise RuntimeError("glow GPU qualification or real-shader negative control failed")
    finally:
        report["source_restored"] = shader.read_bytes() == original
        (output / "qualification.json").write_text(json.dumps(report, indent=2) + "\n")
        if not report["source_restored"]:
            raise RuntimeError("qualification left modified shader bytes")


if __name__ == "__main__":
    main()
