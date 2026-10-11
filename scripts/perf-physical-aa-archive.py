"""One-shot, source-pinned physical A/A release archive intake. No compiling or resampling."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import sys
import urllib.request
import zipfile

REPOSITORY = "yongkyuns/noon"
STUDY = "1933-physical-aa-20261010-01"
FILES = {"pkg/noon_web_bg.wasm", "pkg/noon_web.js", "python-worker.js",
         "runtime-build-identity.json", "ci-artifact.json", "ci-Cargo.lock"}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def github_json(endpoint, token):
    req = urllib.request.Request(
        f"https://api.github.com/repos/{REPOSITORY}/{endpoint}",
        headers={"Authorization": f"Bearer {token}",
                 "Accept": "application/vnd.github+json",
                 "X-GitHub-Api-Version": "2022-11-28",
                 "User-Agent": "noon-physical-aa"},
    )
    with urllib.request.urlopen(req, timeout=30) as response:
        return json.loads(response.read())


def github_zip(endpoint, token, limit=100 * 1024 * 1024):
    req = urllib.request.Request(
        f"https://api.github.com/repos/{REPOSITORY}/{endpoint}",
        headers={"Authorization": f"Bearer {token}",
                 "Accept": "application/vnd.github+json",
                 "X-GitHub-Api-Version": "2022-11-28",
                 "User-Agent": "noon-physical-aa"},
    )
    with urllib.request.urlopen(req, timeout=180) as response:
        body = response.read(limit + 1)
    if not body or len(body) > limit:
        raise ValueError("invalid or oversized archived release ZIP")
    return body


def require_unique_manual_dispatch(runs, current_id):
    """No second workflow-dispatch run may consume this one-shot A/A study."""
    if not isinstance(current_id, str) or not current_id.isdecimal():
        raise ValueError("missing GitHub Actions run identity")
    if runs.get("total_count") != 1:
        raise ValueError("physical A/A study already dispatched or run listing unavailable")
    observed = runs.get("workflow_runs")
    if not isinstance(observed, list) or len(observed) != 1:
        raise ValueError("single registered physical A/A run not established")
    entry = observed[0]
    if (entry.get("id") != int(current_id)
            or entry.get("run_attempt") != 1
            or entry.get("event") != "workflow_dispatch"
            or entry.get("head_branch") != "master"):
        raise ValueError("physical A/A run identity or first-attempt provenance differs")
    return {"runId": int(current_id), "attempt": 1,
            "diagnosticOnly": True, "qualification": False}


def verified_archive(raw, expected_source):
    """Reject traversal, symlinks, missing files, duplicate paths, altered source."""
    from io import BytesIO
    z = zipfile.ZipFile(BytesIO(raw))
    names = set()
    total = 0
    for info in z.infolist():
        p = PurePosixPath(info.filename)
        if p.is_absolute() or ".." in p.parts or "\\" in info.filename or info.is_dir():
            raise ValueError("unsafe archived path")
        if info.filename in names:
            raise ValueError("duplicate archived asset path")
        names.add(info.filename)
        if stat.S_ISLNK(info.external_attr >> 16):
            raise ValueError("symlink in release archive")
        total += info.file_size
        if info.file_size > 120 * 1024 * 1024 or total > 260 * 1024 * 1024:
            raise ValueError("decompressed archive limit exceeded")
    if not FILES.issubset(names) or len(names) > 15:
        raise ValueError("missing or unexpected release asset inventory")
    manifest = json.loads(z.read("ci-artifact.json"))
    if manifest.get("source") != expected_source:
        raise ValueError("release manifest source differs from pinned Git checkout")
    runtime = json.loads(z.read("runtime-build-identity.json"))
    return z, runtime


def exclusive_write(target, raw):
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open("xb") as stream:
        stream.write(raw)


def extract(z, root):
    web = root / "web"
    for entry in z.infolist():
        destination = web.joinpath(*PurePosixPath(entry.filename).parts)
        if not destination.resolve().is_relative_to(web.resolve()):
            raise ValueError("release extraction escaped checkout")
        exclusive_write(destination, z.read(entry.filename))


def main(plan_file, workspace, evidence):
    plan_path = Path(plan_file).resolve()
    root = Path(workspace).resolve()
    record = Path(evidence).resolve()
    plan_bytes = plan_path.read_bytes()
    plan = json.loads(plan_bytes)
    if plan.get("studyId") != STUDY or plan.get("retries") != 0:
        raise ValueError("unregistered physical A/A plan")
    if os.environ.get("GITHUB_RUN_ATTEMPT") != "1" or os.environ.get("GITHUB_EVENT_NAME") != "workflow_dispatch":
        raise ValueError("only one manual physical A/A study attempt is allowed")
    token = os.environ.get("GH_TOKEN")
    if not token:
        raise ValueError("missing read-only GitHub Actions artifact token")
    # A new distinct dispatch is not a rerun of the original registration.
    registry = github_json(
        "actions/workflows/perf-physical-aa.yml/runs"
        "?event=workflow_dispatch&per_page=2", token)
    admission = require_unique_manual_dispatch(registry, os.environ.get("GITHUB_RUN_ID"))
    exclusive_write(record / "registered-plan.json", plan_bytes)
    exclusive_write(record / "single-manual-dispatch.json",
                    (json.dumps(admission, indent=2) + "\n").encode())
    completed = []
    for build in plan["builds"]:
        artifact = build["artifactId"]
        meta = github_json(f"actions/artifacts/{artifact}", token)
        if meta.get("id") != artifact or meta.get("expired"):
            raise ValueError(f"pinned release {artifact} unavailable or expired")
        if meta.get("workflow_run", {}).get("id") != plan["producerRun"]:
            raise ValueError("artifact belongs to a different producer")
        if meta.get("digest") != "sha256:" + build["zipSha256"]:
            raise ValueError("archive metadata disagrees with registered digest")
        raw = github_zip(f"actions/artifacts/{artifact}/zip", token)
        if sha256(raw) != build["zipSha256"]:
            raise ValueError("downloaded archive bytes differ from predeclared SHA256")
        z, runtime = verified_archive(raw, build["source"])
        if runtime.get("buildId") != build["buildId"]:
            raise ValueError("release runtime buildId differs from registered source")
        exclusive_write(record / f"{build['name']}-{artifact}-original.zip", raw)
        for dest in ([build["name"], "baseline-peer"] if build["name"] == "baseline"
                     else ["candidate"]):
            checkout = root / dest
            if not checkout.is_dir() or not (checkout / ".git").exists():
                raise ValueError("source checkout missing for " + dest)
            extract(z, checkout)
        completed.append({"name": build["name"], "artifact": artifact,
                          "sha256": sha256(raw), "source": build["source"],
                          "buildId": build["buildId"], "bytes": len(raw)})
    exclusive_write(record / "original-artifact-attestation.json",
                    (json.dumps({"schema": 1, "studyId": STUDY,
                                 "diagnosticOnly": True, "qualification": False,
                                 "performanceAcceptance": False, "mergeApproval": False,
                                 "originalArtifacts": completed}, indent=2) + "\n").encode())
    print(json.dumps({"originalPinnedReleaseArchives": len(completed),
                      "noBuildsPerformed": True, "qualification": False}))


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: perf-physical-aa-archive.py PLAN.json WORKSPACE EVIDENCE_DIR")
    main(*sys.argv[1:])
