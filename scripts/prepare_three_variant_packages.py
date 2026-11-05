#!/usr/bin/env python3
"""Fetch the three pinned release artifacts and safely overlay their web payloads."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import tempfile
import zipfile

ARTIFACTS = {
    "anchor": {
        "run": 41981888283,
        "id": 13395155425,
        "name": "product-anchor-1",
        "bytes": 21347498,
        "sha256": "094a17d4793bf64c71038bfc0ab41cfe157b0ce91a48092c3174198b351a0bbe",
        "workflowHead": "8ca4f6afd3bc1a7a41de254a0750dcbe392d6a20",
        "source": "58135c4069a6f8d700dcaaee677d7c8c7a48d6c5",
        "role": "baseline",
        "checkout": "anchor-source",
    },
    "d7": {
        "run": 41971877484,
        "id": 13391235889,
        "name": "product-candidate-1",
        "bytes": 24438234,
        "sha256": "77a11940c4e3bcc5ca9a98d01e469103d639a0f3c7f99d32d28fb4dd2cc8dd79",
        "workflowHead": "d7ca32fc984202efb63f92e2a356c80c28dd6314",
        "source": "f3def9bc39afe95f5ba2207d906714bae15d60f7",
        "role": "candidate",
        "checkout": "d7-source",
    },
    "current": {
        "run": 41981888283,
        "id": 13394652896,
        "name": "product-candidate-1",
        "bytes": 24438283,
        "sha256": "524d17616961c8b8fa9ade48432d51e1e987bb37c1f812dc1eb09ffca0d9da06",
        "workflowHead": "8ca4f6afd3bc1a7a41de254a0750dcbe392d6a20",
        "source": "be814def71f9d34d9d28d106631544ca5a8eb3f8",
        "role": "candidate",
        "checkout": "current-source",
    },
}


def lstat_path(path, kind):
    info = path.lstat()
    assert not stat.S_ISLNK(info.st_mode), f"symlink not allowed: {path}"
    if kind == "directory":
        assert stat.S_ISDIR(info.st_mode), f"not a directory: {path}"
    else:
        assert stat.S_ISREG(info.st_mode), f"not a regular file: {path}"
    return info


def assert_no_symlink_components(path):
    path = path.absolute()
    for component in reversed((path, *path.parents)):
        try:
            info = component.lstat()
        except FileNotFoundError:
            continue
        assert not stat.S_ISLNK(info.st_mode), f"symlink path component: {component}"


def mkdir_real(path):
    assert_no_symlink_components(path)
    try:
        info = path.lstat()
    except FileNotFoundError:
        path.mkdir(parents=True)
    else:
        assert not stat.S_ISLNK(info.st_mode), f"symlink not allowed: {path}"
    lstat_path(path, "directory")


def curl(url, token, output=None):
    command = ["curl", "--fail", "--silent", "--show-error", "--location",
               "-H", f"Authorization: Bearer {token}",
               "-H", "Accept: application/vnd.github+json"]
    if output is None:
        result = subprocess.run(command + [url], check=False, capture_output=True)
        assert result.returncode == 0, f"GitHub artifact API request failed with curl status {result.returncode}"
        return result.stdout
    result = subprocess.run(command + [url, "-o", str(output)], check=False,
                            capture_output=True, text=True)
    assert result.returncode == 0, f"GitHub artifact download failed with curl status {result.returncode}"
    return result


def file_sha(path):
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
            size += len(chunk)
    return digest.hexdigest(), size


def package_relative(name):
    path = PurePosixPath(name)
    assert not path.is_absolute() and ".." not in path.parts and "\\" not in name, name
    parts = path.parts
    if parts and parts[0] == "web":
        parts = parts[1:]
    assert parts, name
    rel = PurePosixPath(*parts)
    allowed = (rel.parts[0] == "pkg" or rel.as_posix() in {
        "python-worker.js", "runtime-build-identity.json", "ci-artifact.json", "ci-Cargo.lock",
    } or (len(rel.parts) == 2 and rel.parts[0] == "python" and
          rel.name.startswith("compat-bundle.") and rel.name.endswith(".json")))
    assert allowed, f"unexpected package artifact path: {name}"
    return rel


def write_member(root, rel, payload):
    target = root / Path(*rel.parts)
    parent = root
    for part in rel.parts[:-1]:
        parent = parent / part
        try:
            info = parent.lstat()
        except FileNotFoundError:
            parent.mkdir()
        else:
            assert not stat.S_ISLNK(info.st_mode), f"symlink not allowed: {parent}"
        lstat_path(parent, "directory")
    try:
        target.lstat()
    except FileNotFoundError:
        pass
    else:
        lstat_path(target, "regular")
    with tempfile.NamedTemporaryFile(dir=parent, prefix=".package-", delete=False) as stream:
        temp = Path(stream.name)
        stream.write(payload)
    os.replace(temp, target)
    lstat_path(target, "regular")


def extract_archive(archive, web_root):
    lstat_path(archive, "regular")
    lstat_path(web_root, "directory")
    seen = set()
    extracted = []
    with zipfile.ZipFile(archive) as bundle:
        for member in bundle.infolist():
            mode = member.external_attr >> 16
            assert not stat.S_ISLNK(mode), f"symlink in package ZIP: {member.filename}"
            raw_path = PurePosixPath(member.filename)
            assert not raw_path.is_absolute() and ".." not in raw_path.parts and "\\" not in member.filename, member.filename
            if member.is_dir():
                continue
            rel = package_relative(member.filename)
            assert rel not in seen, f"duplicate package path: {rel}"
            seen.add(rel)
            payload = bundle.read(member)
            write_member(web_root, rel, payload)
            extracted.append({"path": f"web/{rel.as_posix()}", "bytes": len(payload),
                              "sha256": hashlib.sha256(payload).hexdigest()})
    assert any(row["path"] == "web/pkg/noon_web_bg.wasm" for row in extracted)
    assert any(row["path"] == "web/ci-artifact.json" for row in extracted)
    return sorted(extracted, key=lambda row: row["path"])


def main():
    token = os.environ["GH_TOKEN"]
    workspace = Path(os.environ["GITHUB_WORKSPACE"]).absolute()
    study_root = Path(os.environ["STUDY_ROOT"]).absolute()
    runner_temp = Path(os.environ["RUNNER_TEMP"]).absolute()
    expected_root = runner_temp / f"latin-bezier-{os.environ['GITHUB_RUN_ID']}-{os.environ['GITHUB_RUN_ATTEMPT']}"
    assert study_root == expected_root, "study output path must be the unique runner-temp path"
    archives = runner_temp / "three-variant-package-zips"
    assert_no_symlink_components(workspace)
    assert_no_symlink_components(study_root)
    try:
        study_root.lstat()
    except FileNotFoundError:
        pass
    else:
        raise AssertionError(f"study output already exists: {study_root}")
    mkdir_real(study_root)
    mkdir_real(archives)
    listings = {}
    prepared = {"schemaVersion": 1, "diagnosticOnly": True, "variants": {}}
    for variant, pin in ARTIFACTS.items():
        root = workspace / pin["checkout"]
        lstat_path(root, "directory")
        web_root = root / "web"
        lstat_path(web_root, "directory")
        if pin["run"] not in listings:
            url = (f"https://api.github.com/repos/{os.environ['GITHUB_REPOSITORY']}"
                   f"/actions/runs/{pin['run']}/artifacts?per_page=100")
            listings[pin["run"]] = json.loads(curl(url, token))
        artifact = next((row for row in listings[pin["run"]]["artifacts"]
                         if row["id"] == pin["id"]), None)
        assert artifact is not None and not artifact["expired"], variant
        assert artifact["name"] == pin["name"]
        assert artifact["size_in_bytes"] == pin["bytes"]
        assert artifact["digest"] == "sha256:" + pin["sha256"]
        run = artifact["workflow_run"]
        assert run["id"] == pin["run"] and run["head_sha"] == pin["workflowHead"]

        archive = archives / f"{variant}.zip"
        try:
            archive.lstat()
        except FileNotFoundError:
            pass
        else:
            raise AssertionError(f"archive already exists: {archive}")
        url = (f"https://api.github.com/repos/{os.environ['GITHUB_REPOSITORY']}"
               f"/actions/artifacts/{pin['id']}/zip")
        curl(url, token, archive)
        digest, size = file_sha(archive)
        assert digest == pin["sha256"] and size == pin["bytes"], variant
        files = extract_archive(archive, web_root)
        prepared["variants"][variant] = {
            "runId": pin["run"], "artifactId": pin["id"], "artifactName": pin["name"],
            "artifactBytes": size, "artifactSha256": digest, "workflowHead": pin["workflowHead"],
            "sourceRevision": pin["source"], "checkout": pin["checkout"],
            "role": pin["role"], "extractedFiles": files,
        }
        print(f"Prepared pinned {variant} package: {pin['id']} {digest} ({size} bytes), files={len(files)}")
    path = study_root / "prepared-packages.json"
    with path.open("x", encoding="utf-8") as stream:
        stream.write(json.dumps(prepared, sort_keys=True, indent=2) + "\n")
    lstat_path(path, "regular")


if __name__ == "__main__":
    main()
