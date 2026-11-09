#!/usr/bin/env python3
"""Collect exact redistribution evidence; this is not a license-clearance tool.

The default graph is a conservative workspace-default-feature, all-target
superset rooted at noon-python and noon-web. Build dependencies are included;
dev-only edges are not. Nothing is built, installed, or published. Cargo stays
locked/offline. --fetch-missing is an explicit, bounded network opt-in, limited
to GitHub revisions recorded inside checksum-verified registry archives.
"""

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from urllib.parse import quote, urlparse
from urllib.request import Request, urlopen
import zipfile


TEXT_LIMIT = 2 * 1024 * 1024
NOTICE_NAME = re.compile(r"^(?:licen[cs]e|copying|copyright|notice)(?:[._-].*)?$", re.I)
NOTICE_DIRS = {"license", "licenses", "licence", "licences", "legal", "notices"}
SOURCE_SUFFIXES = {".rs", ".c", ".h", ".cpp", ".py", ".js", ".ts", ".wasm"}
NATIVE_NAME = re.compile(r"\.(?:so(?:\..*)?|dylib|dll|pyd|a|lib)$", re.I)
README = """Noon release dependency evidence

inventory.json records a conservative Cargo graph, declared license expressions,
registry checksums, copied evidence, missing evidence, and optional wheel payloads.
SHA256SUMS covers every collected file. Registry evidence is copied unchanged from
the cached .crate archive only after its SHA256 matches Cargo.lock. Optional
upstream files come from the exact GitHub revision in that archive's VCS metadata;
their Git blob hashes are verified and their provenance is recorded individually.

The graph includes all conditional targets and build dependencies in Cargo's
workspace-resolved default-feature graph, so it can include code absent from a
particular binary. Dev-only edges are excluded. Features unified by other
workspace members can enlarge this superset. This is not a binary composition
report. No SPDX alternative is selected and no legal approval is asserted.

Finding license files does not establish that every applicable notice was found.
Pinned ancestor files require applicability review. Embedded assets and vendored
code can have separate terms; retain their dedicated notices too. Optional wheel
inspection inventories native bytes and included notice files after repair. It
does not resolve ELF/Mach-O/PE imports, map native libraries to licenses, or inspect
system libraries, the Rust toolchain, npm bundles, CDN dependencies or external
executables. These remain separate release qualification tasks.
"""


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_digest(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def safe_path(value):
    path = PurePosixPath(value)
    if (not value or path.is_absolute() or ".." in path.parts or
            any(ord(c) < 32 or c in "\\:\x7f" for c in value)):
        raise ValueError(f"unsafe evidence path: {value!r}")
    return path


def is_notice(value):
    path = PurePosixPath(value)
    return path.suffix.lower() not in SOURCE_SUFFIXES and (
        bool(NOTICE_NAME.fullmatch(path.name)) or
        bool(re.fullmatch(r"third[-_]party[-_](?:licenses|notices)(?:\..*)?", path.name, re.I)) or
        any(part.lower() in NOTICE_DIRS for part in path.parts[:-1]))


def put(output, relative, data):
    destination = output.joinpath(*safe_path(relative).parts)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("xb") as handle:
        handle.write(data)
    return {"path": relative, "sha256": digest(data), "bytes": len(data)}


def graph(metadata, roots):
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    members = set(metadata["workspace_members"])
    root_ids = []
    for name in roots:
        matches = [pid for pid in members if packages[pid]["name"] == name]
        if len(matches) != 1:
            raise ValueError(f"expected one workspace root named {name}")
        root_ids.extend(matches)
    reached = {}
    for root in root_ids:
        pending, seen = [root], set()
        while pending:
            current = pending.pop()
            if current in seen:
                continue
            seen.add(current)
            reached.setdefault(current, []).append(packages[root]["name"])
            for dependency in nodes[current]["deps"]:
                if any(kind["kind"] != "dev" for kind in dependency["dep_kinds"]):
                    pending.append(dependency["pkg"])
    return packages, nodes, members, reached


def archive_evidence(package, expected, output, prefix):
    """Read only bounded evidence files from a checksum-verified Cargo cache."""
    manifest = Path(package["manifest_path"])
    source = manifest.parent
    archive = source.parents[1] / "cache" / source.parent.name / f"{package['name']}-{package['version']}.crate"
    # Normal Cargo registry layout is registry/src/REGISTRY/NAME-VERSION.
    if source.parents[1].name == "src":
        archive = source.parents[2] / "cache" / source.parent.name / archive.name
    if not archive.is_file():
        raise ValueError("cached registry archive unavailable; run cargo fetch --locked before collecting")
    if file_digest(archive) != expected:
        raise ValueError("cached registry archive does not match Cargo.lock checksum")
    base = f"{package['name']}-{package['version']}"
    declared = package.get("license_file")
    if declared and Path(declared).is_absolute():
        declared = Path(declared).relative_to(source).as_posix()
    if declared:
        declared = str(safe_path(declared))
    evidence, notices, vcs, issues = [], [], None, []
    with tarfile.open(archive, "r:gz") as packed:
        for entry in packed:
            parts = safe_path(entry.name).parts
            if not parts or parts[0] != base:
                raise ValueError("registry archive has an unexpected package root")
            relative = "/".join(parts[1:])
            if not relative:
                continue
            notice = is_notice(relative) or relative == declared
            if not notice and relative not in {"Cargo.toml", ".cargo_vcs_info.json"}:
                continue
            if entry.isdir():
                continue
            if not entry.isfile() or entry.size > TEXT_LIMIT:
                issues.append(f"unsupported or oversized evidence: {relative}")
                continue
            data = packed.extractfile(entry).read(TEXT_LIMIT + 1)
            if relative == "Cargo.toml":
                declared_package = tomllib.loads(data.decode())["package"]
                for field in ("name", "version", "license", "repository"):
                    if declared_package.get(field) != package.get(field):
                        raise ValueError(f"Cargo metadata differs from checksum-verified manifest: {field}")
                if declared_package.get("license-file") != declared:
                    raise ValueError("Cargo metadata differs from checksum-verified manifest: license-file")
            item = put(output, f"{prefix}/registry/{relative}", data)
            item.update({"origin": "checksum-verified-registry-archive", "upstreamPath": relative})
            evidence.append(item)
            if notice:
                if data.strip():
                    notices.append(item["path"])
                else:
                    issues.append(f"empty license or notice text: {relative}")
            if relative == ".cargo_vcs_info.json":
                vcs = json.loads(data)
    if declared and not any(item["upstreamPath"] == declared for item in evidence):
        issues.append(f"declared license-file is absent: {declared}")
    return evidence, notices, vcs, issues


class PinnedGitHub:
    """Bounded primary-source lookup, cached by URL and repository + revision."""

    def __init__(self, cache):
        self.cache, self.trees, self.requests = cache, {}, 0

    def request(self, url, limit):
        self.cache.mkdir(parents=True, exist_ok=True)
        cached = self.cache / digest(url.encode())
        if cached.is_file():
            data = cached.read_bytes()
        else:
            self.requests += 1
            if self.requests > 256:
                raise ValueError("bounded upstream request limit exceeded")
            request = Request(url, headers={"User-Agent": "noon-release-license-evidence", "Accept": "application/vnd.github+json"})
            with urlopen(request, timeout=15) as response:
                if urlparse(response.url).hostname not in {"api.github.com", "raw.githubusercontent.com"}:
                    raise ValueError("upstream evidence redirected outside GitHub")
                data = response.read(limit + 1)
            if len(data) <= limit:
                cached.write_bytes(data)
        if len(data) > limit:
            raise ValueError("upstream response exceeds evidence size limit")
        return data

    def collect(self, package, vcs, output, prefix):
        repository = urlparse(package.get("repository") or "")
        match = re.fullmatch(r"/([\w.-]+)/([\w.-]+?)(?:\.git)?/?", repository.path)
        sha = (vcs or {}).get("git", {}).get("sha1", "")
        if (repository.scheme != "https" or repository.hostname != "github.com" or
                repository.query or repository.fragment or not match or not re.fullmatch(r"[0-9a-fA-F]{40}", sha) or
                (vcs or {}).get("git", {}).get("dirty")):
            raise ValueError("no HTTPS GitHub repository plus pinned 40-hex VCS revision")
        repo, sha = "/".join(match.groups()), sha.lower()
        relative = (vcs or {}).get("path_in_vcs", "")
        crate_path = safe_path(relative) if relative else PurePosixPath(".")
        ancestors = {str(crate_path), *(str(p) for p in crate_path.parents), "."}
        key = (repo, sha)
        if key not in self.trees:
            if len(self.trees) >= 64:
                raise ValueError("bounded upstream repository limit exceeded")
            try:
                tree = json.loads(self.request(f"https://api.github.com/repos/{repo}/git/trees/{sha}?recursive=1", 16 * TEXT_LIMIT))
                if tree.get("truncated") or not isinstance(tree.get("tree"), list):
                    raise ValueError("upstream tree is truncated or unavailable")
                self.trees[key] = tree
            except Exception as error:
                self.trees[key] = str(error)
        tree = self.trees[key]
        if isinstance(tree, str):
            raise ValueError(tree)
        result = []
        for entry in sorted(tree["tree"], key=lambda value: value["path"]):
            relative = str(safe_path(entry["path"]))
            candidate = PurePosixPath(relative)
            primary = str(candidate.parent) in ancestors or any(
                str(parent.parent) in ancestors and parent.name.lower() in NOTICE_DIRS
                for parent in candidate.parents)
            if entry["type"] != "blob" or not primary or not is_notice(relative):
                continue
            if entry.get("mode") not in {"100644", "100755"}:
                raise ValueError(f"upstream notice is not a regular file: {relative}")
            if len(result) >= 64:
                raise ValueError("bounded upstream file limit exceeded")
            url = f"https://raw.githubusercontent.com/{repo}/{sha}/{quote(relative, safe='/')}"
            data = self.request(url, TEXT_LIMIT)
            blob = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
            if blob != entry["sha"]:
                raise ValueError(f"upstream Git blob hash mismatch: {relative}")
            if not data.strip():
                raise ValueError(f"empty upstream license or notice text: {relative}")
            item = put(output, f"{prefix}/upstream/{relative}", data)
            item.update({"origin": "pinned-upstream-ancestor", "url": url, "repository": f"https://github.com/{repo}",
                         "revision": sha, "gitBlobSha1": blob, "upstreamPath": relative,
                         "cratePathInVcs": str(crate_path), "applicability": "requires review"})
            result.append(item)
        return result


def wheel_evidence(filename, output):
    wheel_hash = file_digest(filename)
    prefix = f"wheels/{filename.name}-{wheel_hash[:12]}"
    result = {"filename": filename.name, "sha256": wheel_hash, "nativePayloads": [], "noticeFiles": [],
              "scope": "archive payload inventory only; native imports and license mappings require separate review"}
    with zipfile.ZipFile(filename) as wheel:
        for entry in sorted(wheel.infolist(), key=lambda item: item.filename):
            name = str(safe_path(entry.filename))
            if entry.is_dir():
                continue
            if NATIVE_NAME.search(PurePosixPath(name).name):
                with wheel.open(entry) as handle:
                    checksum = hashlib.file_digest(handle, "sha256").hexdigest()
                result["nativePayloads"].append({"path": name, "sha256": checksum, "bytes": entry.file_size,
                                                "isNoonExtension": PurePosixPath(name).name.startswith("_noon_native.")})
            if is_notice(name):
                if entry.file_size > TEXT_LIMIT:
                    raise ValueError(f"oversized wheel notice: {name}")
                item = put(output, f"{prefix}/notices/{name}", wheel.read(entry))
                item.update({"origin": "wheel-payload", "upstreamPath": name})
                result["noticeFiles"].append(item)
    return result


def collect(root, output, metadata, roots, wheels=(), fetcher=None, metadata_selection="provided Cargo metadata"):
    root, output = root.resolve(), output.resolve()
    if output.exists():
        raise ValueError("output already exists; use a fresh evidence directory")
    if Path(metadata["workspace_root"]).resolve() != root:
        raise ValueError("metadata workspace root does not match --root")
    lock_bytes = (root / "Cargo.lock").read_bytes()
    locked = {(p["name"], p["version"], p.get("source")): p for p in tomllib.loads(lock_bytes.decode())["package"]}
    packages, nodes, members, reached = graph(metadata, roots)
    output.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix=".license-evidence-", dir=output.parent))
    try:
        report = {"schema": 1, "lockfileSha256": digest(lock_bytes), "scope": {
            "roots": sorted(set(roots)), "featureSelection": metadata_selection,
            "targets": "all conditional targets reported by Cargo; no platform filter",
            "dependencyKinds": ["normal", "build"], "classification": "conservative dependency superset",
            "legalClearance": "not assessed", "nativeLibraryLicensing": "not assessed"}, "packages": [], "wheels": []}
        key = lambda pid: (packages[pid]["name"], packages[pid]["version"], packages[pid].get("source") or "workspace")
        keys = {pid: f"{packages[pid]['name']}-{packages[pid]['version']}-{digest((packages[pid].get('source') or 'workspace').encode())[:12]}"
                for pid in reached}
        for pid in sorted(reached, key=key):
            package = packages[pid]
            source = package.get("source")
            lock = locked.get((package["name"], package["version"], source))
            if lock is None:
                raise ValueError(f"metadata package absent from Cargo.lock: {package['name']} {package['version']}")
            item = {"id": keys[pid], "name": package["name"], "version": package["version"], "source": source,
                    "workspaceMember": pid in members, "roots": sorted(reached[pid]),
                    "declaredLicense": package.get("license"), "declaredLicenseFile": package.get("license_file"),
                    "repository": package.get("repository"), "registryChecksum": lock.get("checksum"),
                    "features": sorted(nodes[pid]["features"]), "dependencies": [], "evidenceFiles": [], "noticeFiles": [], "issues": []}
            for dependency in nodes[pid]["deps"]:
                kinds = [kind for kind in dependency["dep_kinds"] if kind["kind"] != "dev"]
                if kinds:
                    item["dependencies"].append({"id": keys[dependency["pkg"]], "conditions": kinds})
            item["dependencies"].sort(key=lambda value: value["id"])
            if pid not in members:
                vcs = None
                try:
                    if not source or not source.startswith("registry+") or not lock.get("checksum"):
                        raise ValueError("non-registry dependency needs separate pinned source evidence")
                    evidence, notices, vcs, issues = archive_evidence(package, lock["checksum"], stage, f"crates/{keys[pid]}")
                    item["evidenceFiles"], item["noticeFiles"], item["issues"] = evidence, notices, issues
                    item["vcs"] = vcs
                except (OSError, ValueError, tarfile.TarError) as error:
                    item["issues"].append(str(error))
                if not item["noticeFiles"] and fetcher and vcs:
                    try:
                        evidence = fetcher.collect(package, vcs, stage, f"crates/{keys[pid]}")
                        item["evidenceFiles"].extend(evidence)
                        item["noticeFiles"].extend(entry["path"] for entry in evidence)
                    except Exception as error:
                        item["issues"].append(f"pinned upstream lookup: {error}")
                if not item["noticeFiles"]:
                    item["issues"].append("no license or notice text collected")
                if not package.get("license") and not package.get("license_file"):
                    item["issues"].append("no license declaration in Cargo metadata")
            report["packages"].append(item)
        report["wheels"] = [wheel_evidence(wheel.resolve(), stage) for wheel in sorted(wheels)]
        missing = [p for p in report["packages"] if p["issues"]]
        report["summary"] = {"packages": len(report["packages"]), "thirdPartyPackages": sum(not p["workspaceMember"] for p in report["packages"]),
            "packagesWithEvidenceGaps": len(missing), "nativePayloads": sum(len(w["nativePayloads"]) for w in report["wheels"]),
            "additionalNativeLibrariesRequiringReview": sum(not p["isNoonExtension"] for w in report["wheels"] for p in w["nativePayloads"]),
            "pinnedAncestorFilesRequiringReview": sum(e["origin"] == "pinned-upstream-ancestor" for p in report["packages"] for e in p["evidenceFiles"])}
        put(stage, "inventory.json", (json.dumps(report, sort_keys=True, indent=2) + "\n").encode())
        put(stage, "README.txt", README.encode())
        put(stage, "Cargo.lock", lock_bytes)
        lines = ["# Evidence gaps requiring review", "", "This report does not establish redistribution approval.", ""]
        lines += [f"- {p['name']} {p['version']}: {'; '.join(p['issues'])}" for p in missing]
        if not missing:
            lines.append("No missing Cargo notice evidence was detected by this bounded collector.")
        lines += ["", "Native library license mappings and pinned ancestor-file applicability remain separate review tasks.", ""]
        put(stage, "REVIEW.md", "\n".join(lines).encode())
        checksums = "".join(f"{file_digest(filename)}  {filename.relative_to(stage).as_posix()}\n"
                            for filename in sorted(stage.rglob("*")) if filename.is_file())
        put(stage, "SHA256SUMS", checksums.encode())
        stage.rename(output)
        return report
    finally:
        if stage.exists():
            shutil.rmtree(stage)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, help="pre-captured Cargo metadata; selection is labeled as caller-provided")
    parser.add_argument("--package", action="append", help="workspace graph root; repeatable, defaults to noon-python and noon-web")
    parser.add_argument("--wheel", action="append", type=Path, default=[], help="inventory an actual repaired wheel; repeatable")
    parser.add_argument("--fetch-missing", action="store_true", help="fetch primary notices only at checksum-verified package GitHub revisions")
    parser.add_argument("--cache", type=Path, help="pinned upstream cache, default target/release-license-cache")
    parser.add_argument("--report-only", action="store_true", help="write evidence and report gaps without returning a failing exit code")
    args = parser.parse_args()
    try:
        if args.metadata:
            metadata = json.loads(args.metadata.read_text())
            selection = "caller-provided Cargo metadata; feature selection is not inferred"
        else:
            command = ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"]
            metadata = json.loads(subprocess.check_output(command, cwd=args.root))
            selection = "workspace-resolved default features (cargo metadata --locked --offline)"
        fetcher = PinnedGitHub(args.cache or args.root / "target/release-license-cache") if args.fetch_missing else None
        report = collect(args.root, args.output, metadata, args.package or ["noon-python", "noon-web"], args.wheel, fetcher, selection)
        print(json.dumps({"output": str(args.output.resolve()), **report["summary"], "legalClearance": "not assessed"}, indent=2))
        return int(bool(report["summary"]["packagesWithEvidenceGaps"]) and not args.report_only)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        print(f"license evidence collection failed: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
