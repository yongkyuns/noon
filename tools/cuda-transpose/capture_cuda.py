#!/usr/bin/env python3
"""Create/verify a source-bound CUDA evidence bundle; never fall back to CPU.

The recorder also runs original, uninstrumented kernels. Sanitizer exit codes
are mandatory. A checked capture is reproducible evidence, not signed hardware
attestation and not an instruction timeline or performance measurement.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CUDA_FILES = ("tools/cuda-transpose/record_cuda.cu", "tools/cuda-transpose/original_kernels.cuh")
FLAGS = ["-std=c++17", "-O2", "-lineinfo"]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def source_fingerprints():
    return {name: digest((ROOT / name).read_bytes()) for name in CUDA_FILES}


def need(ok, message):
    if not ok:
        raise ValueError(message)


def validate_run(run, column):
    need(isinstance(run, dict), "run must be an object")
    stride, base = run.get("stride"), run.get("base_word")
    need(type(stride) is int and stride in (32, 33) and type(base) is int and base >= 0,
         "invalid stride or shared-memory base")
    need(run.get("loads") == [256]*4 and run.get("verified_output_words") == 1024,
         "incomplete load/output coverage")
    # Independent row-major -> transposed index formula, not the recorder loop.
    loads = [[r, c, base + r*stride+c, r*32+c, r*32+c] for r in range(32) for c in range(32)]
    reads = [[r, c, base + r*stride+c, c*32+r, r*32+c] for c in range(32) for r in range(32)]
    output = [(index % 32)*32 + index//32 for index in range(1024)]
    need(run.get("load_accesses") == loads, "CUDA load address/value mismatch")
    need(run.get("read_accesses") == reads, "CUDA shared read/store mismatch")
    need(run.get("reads") == reads[column*32:(column+1)*32], "selected-column mismatch")
    need(run.get("output") == output, "instrumented CUDA output mismatch")
    need(run.get("original_output") == output, "original uninstrumented CUDA output mismatch")


def validate_bundle(bundle, check_sources=True):
    need(isinstance(bundle, dict) and bundle.get("schema") == 1, "unsupported capture schema")
    p = bundle.get("provenance", {})
    need(p.get("origin") == "cuda-instrumented-and-original", "not a CUDA capture")
    need(p.get("configuration") == {"rows":32, "cols":32, "grid":[1,1], "block":[32,8],
                                    "input":"input[r,c]=32*r+c", "word_bytes":4}, "unsupported fixture")
    need(p.get("flags") == FLAGS and bool(p.get("nvcc_version")), "missing compiler provenance")
    need(isinstance(p.get("executable_sha256"), str) and len(p["executable_sha256"]) == 64,
         "missing binary identity")
    need(isinstance(p.get("device"), dict) and bool(p["device"].get("device")), "missing GPU identity")
    if check_sources:
        need(p.get("sources") == source_fingerprints(), "capture is stale: CUDA source fingerprints changed")
    sanitizer = p.get("sanitizers", {})
    need(set(sanitizer) == {"memcheck", "racecheck", "synccheck"}, "missing sanitizer qualification")
    for name, result in sanitizer.items():
        need(result.get("returncode") == 0 and isinstance(result.get("log"), str),
             name + " did not complete cleanly")
        need(result.get("sha256") == digest(result["log"].encode()), name + " log checksum mismatch")
        # Require actual Compute Sanitizer summary, not just a process returning 0.
        need("ERROR SUMMARY: 0 errors" in result["log"] if name != "racecheck"
             else "RACECHECK SUMMARY: 0 hazards" in result["log"], name + " clean summary is missing")
    columns = bundle.get("columns", {})
    need(set(columns) == {str(i) for i in range(32)}, "capture must cover all 32 columns")
    for col in range(32):
        entry = columns[str(col)]
        need(entry.get("column") == col, "column key differs from its trace")
        need([r.get("stride") for r in entry.get("runs", [])] == [32,33], "both layouts are required")
        for run in entry["runs"]:
            validate_run(run, col)
    return bundle


def run_checked(command, cwd, timeout=300):
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError("command failed (%d): %r\n%s\n%s" %
                           (result.returncode, command, result.stdout, result.stderr))
    return result


def capture(destination):
    nvcc, sanitizer = shutil.which("nvcc"), shutil.which("compute-sanitizer")
    if nvcc is None or sanitizer is None:
        raise RuntimeError("nvcc and compute-sanitizer are required; no CPU or synthetic fallback is permitted")
    before = source_fingerprints()
    columns, sanitizer_results = {}, {}
    with tempfile.TemporaryDirectory(prefix="noon-cuda-capture-") as tmp:
        binary = Path(tmp) / "record_cuda"
        version = run_checked([nvcc, "--version"], ROOT).stdout
        run_checked([nvcc, *FLAGS, str(HERE / "record_cuda.cu"), "-o", str(binary)], ROOT)
        binary_hash = digest(binary.read_bytes())
        device = None
        for column in range(32):
            result = run_checked([str(binary), str(column), "--json"], ROOT)
            raw = json.loads(result.stdout)
            identity = {k: raw[k] for k in ("device", "compute_capability", "runtime_version", "driver_version", "nvcc_version")}
            if device is None:
                device = identity
            need(identity == device, "GPU/toolchain identity changed during capture")
            columns[str(column)] = {"column":raw["column"], "runs":raw["runs"]}
            for r in raw["runs"]:
                validate_run(r, column)
        # One launch executes the same complete tile for every column selector;
        # the selector only chooses which validated records are printed.
        for tool in ("memcheck", "racecheck", "synccheck"):
            result = run_checked([sanitizer, "--tool", tool, "--error-exitcode", "1", str(binary), "3"], ROOT, timeout=600)
            log = result.stdout + result.stderr
            sanitizer_results[tool] = {"returncode":result.returncode, "log":log, "sha256":digest(log.encode())}
    need(source_fingerprints() == before, "CUDA source changed while capturing")
    bundle = {"schema":1, "provenance":{
        "origin":"cuda-instrumented-and-original", "captured_at":datetime.now(timezone.utc).isoformat(),
        "sources":before, "flags":FLAGS, "nvcc_version":version, "executable_sha256":binary_hash,
        "device":device, "sanitizers":sanitizer_results,
        "configuration":{"rows":32, "cols":32, "grid":[1,1], "block":[32,8],
                         "input":"input[r,c]=32*r+c", "word_bytes":4}}, "columns":columns}
    validate_bundle(bundle)
    destination.parent.mkdir(parents=True, exist_ok=True)
    tmp = destination.with_name(destination.name + ".tmp")
    tmp.write_text(json.dumps(bundle, separators=(",", ":")) + "\n", encoding="utf-8")
    tmp.replace(destination)
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    choice = parser.add_mutually_exclusive_group(required=True)
    choice.add_argument("--output", type=Path, help="compile, capture and sanitize CUDA, then write this file")
    choice.add_argument("--verify", type=Path, help="verify an existing source-bound capture")
    args = parser.parse_args()
    try:
        if args.verify:
            validate_bundle(json.loads(args.verify.read_text(encoding="utf-8")))
            print("PASS: full CUDA evidence, original outputs, sanitizer logs and current source fingerprints")
        else:
            print("Saved verified CUDA capture:", capture(args.output.resolve()))
        return 0
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print("CUDA CAPTURE FAILED:", error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
