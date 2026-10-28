"""Strict, temporary #2296/#2328 test-harness reconciliation, not product code."""
from __future__ import annotations

SCALAR_START = b"  const scalarContinuationSource = await readFile("
SCALAR_END = b"\n  async function observeScalarDuring("
SPARSE_START = b"  // Sparse callback reads suspend only the active callback invocation."
SPARSE_END = b"  // Existing and provisional structural edits publish with effective writes."
HELPER_START = b"async function startSampledSource("
HELPER_END = b"async function stopSampledSource("


def section(source: bytes, start: bytes, end: bytes) -> bytes:
    if source.count(start) != 1 or source.count(end) != 1:
        raise ValueError("section boundary is missing or ambiguous")
    first, last = source.index(start), source.index(end)
    if first >= last:
        raise ValueError("section boundaries are reversed")
    return source[first:last]


def replace(source: bytes, start: bytes, end: bytes, replacement: bytes) -> bytes:
    old = section(source, start, end)
    return source.replace(old, replacement, 1)


def reconcile(base: bytes, pr: bytes, master: bytes) -> bytes:
    base_scalar = section(base, SCALAR_START, SCALAR_END)
    pr_scalar = section(pr, SCALAR_START, SCALAR_END)
    master_scalar = section(master, SCALAR_START, SCALAR_END)
    if base_scalar != master_scalar:
        raise ValueError("master also changed the scalar continuation; needs review")
    if b'createExecutionAttachment("scalar continuation")' not in pr_scalar:
        raise ValueError("PR scalar readiness safeguard is absent")
    if b"await attachment.ready;" not in pr_scalar:
        raise ValueError("PR no longer awaits scalar attachment")
    # Prove we account for *all* PR changes to this file, not just the conflict.
    projected_pr = replace(base, SCALAR_START, SCALAR_END, pr_scalar)
    projected_pr = replace(projected_pr, SPARSE_START, SPARSE_END,
                           section(pr, SPARSE_START, SPARSE_END))
    if projected_pr != pr:
        raise ValueError("unreviewed PR edits outside the two expected sections")
    master_sparse = section(master, SPARSE_START, SPARSE_END)
    for required in (b"sampleToAuthoredTime(0.75)", b"sampleToAuthoredTime(1.5)",
                     b"await stopSampledSource(page);", b"callbackSessionId"):
        if required not in master_sparse:
            raise ValueError("master deterministic callback contract is missing")
    result = replace(master, SCALAR_START, SCALAR_END, pr_scalar)
    if section(result, SPARSE_START, SPARSE_END) != master_sparse:
        raise ValueError("master sparse callback coverage changed")
    if section(result, HELPER_START, HELPER_END) != section(master, HELPER_START, HELPER_END):
        raise ValueError("master sampled-source attachment lifecycle changed")
    if section(result, SCALAR_START, SCALAR_END) != pr_scalar:
        raise ValueError("PR scalar readiness block changed")
    # The inverse proves the resolved script is *exactly* master plus that block.
    if replace(result, SCALAR_START, SCALAR_END, master_scalar) != master:
        raise ValueError("unrelated master source was changed")
    return result


def main() -> None:
    import hashlib
    import json
    from pathlib import Path
    import subprocess

    path = "scripts/shared-authoring-smoke.mjs"
    inputs = {
        "base": ("bc25e92bb741607f678f6adea528c9e894c6335c", "bbb9b67e1e24bb63fe55bc08b6e09c8b64384edd"),
        "pr": ("0cb1b7dabaed0df82567794c09fff17327256d05", "0ec62df0253d03d31949707d19c9224b5776a6e4"),
        "master": ("5b38eab85a86f4cc9019e8179e263c7b29771827", "8acec76bb76738b514e985cea1883994379d2df1"),
    }
    def git_blob(data: bytes) -> str:
        return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()

    sources = {}
    for name, (commit, expected_blob) in inputs.items():
        data = subprocess.check_output(["git", "show", f"{commit}:{path}"])
        if git_blob(data) != expected_blob:
            raise ValueError(f"{name} source does not match inspected Git blob")
        sources[name] = data
    result = reconcile(sources["base"], sources["pr"], sources["master"])
    Path(path).write_bytes(result)
    print(json.dumps({"path": path, "inputs": inputs, "result_blob": git_blob(result),
                      "result_sha256": hashlib.sha256(result).hexdigest(),
                      "master_sparse_and_helper_preserved": True,
                      "pr_scalar_block_preserved": True}, indent=2))


if __name__ == "__main__":
    main()
