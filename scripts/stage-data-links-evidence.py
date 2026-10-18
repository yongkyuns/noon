"""Staging-only admission check for a *real* Noon renderer artifact.

Does not create/modify image bytes or weaken source/presentation checks.
Permanent gallery publication imports these exact Git blobs only after review.
"""
from pathlib import Path
import hashlib
import json
import struct

proof = Path("stage-upload")
src = Path("web/python/examples/showcase_data_links.py").read_bytes()
image = (proof / "showcase-data-links.png").read_bytes()
evidence = json.loads((proof / "capture-evidence.json").read_text())
report = json.loads((proof / "report.json").read_text())

def sha(data):
    return hashlib.sha256(data).hexdigest()

assert evidence["schema"] == 1 and evidence["publication"] == "captured-preview"
assert evidence["backend"] == "webgpu" and report["backendRequested"] == "webgpu"
assert report["stagedPosterDecode"] == "pass"
assert evidence["captureRevision"] == report["checkoutRevision"]
assert evidence["runtimeBuildIdentity"] == report["servedBuildIdentity"]
assert report["checkoutRevision"] == report["servedBuildIdentity"]["sourceRevision"]
assert len(evidence["posters"]) == len(report["results"]) == 1
poster, result = evidence["posters"][0], report["results"][0]
assert poster["id"] == result["id"] == "showcase-data-links"
assert result["outcome"] == "pass" and result["pageErrors"] == []
assert poster["captureBuildId"] == report["servedBuildIdentity"]["buildId"]
assert poster["sourceSha256"] == result["sourceSha256"] == sha(src)
assert poster["thumbnailTime"] == 3.8
assert poster["image"]["pngSha256"] == sha(image)
assert image[:8] == bytes((137, 80, 78, 71, 13, 10, 26, 10))
assert image[12:16] == b"IHDR"
assert struct.unpack(">II", image[16:24]) == (960, 540)
assert poster["image"]["width"] == 960 and poster["image"]["height"] == 540
samples = result["samples"]
assert any(s["requestedTime"] == 3.8 and s["publishedTime"] == 3.8 and
           s["pngSha256"] == sha(image) for s in samples)
assert samples[-1]["sourceCompleted"] is True
assert samples[-1]["sourceState"] == "completed"
assert abs(samples[-1]["authoredDuration"] - 7.2) < 1e-9
assert abs(samples[-1]["publishedTime"] - 7.2) < 1e-9
print("Verified actual WebGPU poster, source hash, viewport, replay-ready endpoint and build provenance")
