"""Reconstruct one hash-pinned candidate; never publish validation scaffolding."""
from pathlib import Path
import base64
import gzip
import hashlib
import subprocess

BASE = "0b9411aaec4c9dab6ccaecaa1dff8675a9709a8f"
EXPECTED = "2e12ff7caa02f0eb44b2f6eb67e918ac7d624628e951f8d82bfaae71ed0022ed"
subprocess.run(["git", "merge-base", "--is-ancestor", BASE, "HEAD"], check=True)
s = Path("scripts/validation/c5-direct-inspection.patch.gz.b64").read_text().strip()
# Repair transport transcription only; gzip integrity and the full source digest
# below must both succeed before any source file is changed.
for old, new in [
    ("Hc3meryElax", "Hc3mKo6meryElax"),
    ("giMMSpHldV78", "giMMSpE9ldV78"),
    ("InsFsNI0mG2fAa", "InsFsNI0m0sgG2fAa"),
    ("SNUVZZkxfwKaha", "SNUVZkxfwKaha"),
]:
    assert s.count(old) == 1, old
    s = s.replace(old, new)
patch = gzip.decompress(base64.b64decode(s, validate=True))
actual = hashlib.sha256(patch).hexdigest()
assert actual == EXPECTED, (actual, EXPECTED)
proof = Path("/tmp/c5-direct-proof")
proof.mkdir(exist_ok=True)
p = proof / "unformatted-source.patch"
p.write_bytes(patch)
(proof / "input-sha256.txt").write_text(actual + "\n")
subprocess.run(["git", "apply", "--check", str(p)], check=True)
subprocess.run(["git", "apply", str(p)], check=True)
print("Applied exact browser inspection source:", actual)
