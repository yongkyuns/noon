import argparse
import hashlib
import io
import json
import stat
import zipfile

RUN_ID = 42005151229
ARTIFACT_ID = 13404372868
ARTIFACT_SHA256 = "ed7462d273aa111d5a3ab5b48051150985fc8cbb9d5fdbd442c6d7a3faa24c85"
ARTIFACT_BYTES = 45999936
DIAGNOSTIC_HEAD = "0c252dfd3ca4eaf3cdf3d760e54ae1b489805e76"
BEZIER_SOURCE_SHA256 = "60e5708b4a235f68c8b0788fd57fb219fdc4a2949aa61573d1aa0cef267ff84b"
SOURCES = {
    "anchor": "58135c4069a6f8d700dcaaee677d7c8c7a48d6c5",
    "candidate": "be814def71f9d34d9d28d106631544ca5a8eb3f8",
}
FUNCTIONS_PER_LIST = 20
MAX_MEMBERS = 100
MAX_UNCOMPRESSED_BYTES = 160_000_000


def require(condition, message):
    if not condition:
        raise ValueError(message)


def safe_members(archive):
    members = archive.infolist()
    require(len(members) <= MAX_MEMBERS, "unexpectedly large artifact file list")
    total = 0
    by_name = {}
    for member in members:
        name = member.filename
        require(name not in by_name, f"duplicate artifact path: {name}")
        require("\\" not in name and not name.startswith("/") and ".." not in name.split("/"),
                f"unsafe artifact path: {name}")
        require(not stat.S_ISLNK(member.external_attr >> 16), f"symlink artifact entry: {name}")
        total += member.file_size
        by_name[name] = member
    require(total <= MAX_UNCOMPRESSED_BYTES, "artifact exceeds bounded expanded size")
    return by_name


def load_json(archive, files, name):
    require(name in files, f"missing artifact file: {name}")
    return json.loads(archive.read(files[name]))


def frame_key(call_frame):
    return (call_frame.get("functionName") or "<anonymous>",
            call_frame.get("url") or "", call_frame.get("lineNumber"), call_frame.get("columnNumber"))


def summarize_profile(raw, expected):
    profile = json.loads(raw)
    nodes = profile.get("nodes") or []
    samples = profile.get("samples") or []
    deltas = profile.get("timeDeltas") or []
    require(nodes and samples and len(samples) == len(deltas), "profile has no samples or mismatched timeDeltas")
    by_id = {node["id"]: node for node in nodes}
    require(len(by_id) == len(nodes), "CPU profile contains duplicate node IDs")
    parent = {}
    for node in nodes:
        for child in node.get("children") or []:
            require(child in by_id and child not in parent, "CPU profile has an invalid/multiply-parented node")
            parent[child] = node["id"]

    self_us, inclusive_us, leaf_samples = {}, {}, {}
    total_us = 0
    for node_id, delta in zip(samples, deltas):
        require(node_id in by_id and isinstance(delta, int) and delta >= 0,
                "CPU profile contains an invalid sample or time delta")
        total_us += delta
        leaf = by_id[node_id]
        key = frame_key(leaf.get("callFrame") or {})
        self_us[key] = self_us.get(key, 0) + delta
        leaf_samples[key] = leaf_samples.get(key, 0) + 1
        visited, visited_frames = set(), set()
        while node_id is not None:
            require(node_id not in visited, "CPU profile parent cycle")
            visited.add(node_id)
            node = by_id[node_id]
            key = frame_key(node.get("callFrame") or {})
            if key not in visited_frames:
                inclusive_us[key] = inclusive_us.get(key, 0) + delta
                visited_frames.add(key)
            node_id = parent.get(node_id)

    def rows(values):
        return [{"function": name, "url": url, "line": line, "column": column,
                 "milliseconds": round(duration / 1000, 3),
                 "percentOfSampled": round(100 * duration / total_us, 2) if total_us else 0,
                 "leafSamples": leaf_samples.get((name, url, line, column), 0)}
                for (name, url, line, column), duration in sorted(values.items(), key=lambda item: item[1], reverse=True)[:FUNCTIONS_PER_LIST]]

    def special_frame_time(function_name):
        return round(sum(duration for (name, _url, _line, _column), duration in self_us.items()
                         if name == function_name) / 1000, 3)

    return {
        "expectedFileSha256": expected["sha256"],
        "actualFileSha256": hashlib.sha256(raw).hexdigest(),
        "nodeCount": len(nodes), "sampleCount": len(samples), "sampledMilliseconds": round(total_us / 1000, 3),
        "profileStartMicroseconds": profile.get("startTime"),
        "profileEndMicroseconds": profile.get("endTime"),
        "selfTop": rows(self_us), "inclusiveTop": rows(inclusive_us),
        "chromeSpecialFrameSelfMilliseconds": {
            "idle": special_frame_time("(idle)"),
            "garbageCollector": special_frame_time("(garbage collector)"),
            "program": special_frame_time("(program)"),
        },
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("artifact_zip")
    parser.add_argument("output_json")
    args = parser.parse_args()
    raw_zip = open(args.artifact_zip, "rb").read()
    require(len(raw_zip) == ARTIFACT_BYTES, "uploaded diagnostic artifact byte size changed")
    require(hashlib.sha256(raw_zip).hexdigest() == ARTIFACT_SHA256, "uploaded diagnostic artifact digest changed")

    with zipfile.ZipFile(io.BytesIO(raw_zip)) as archive:
        files = safe_members(archive)
        require("profile-exit-status.txt" in files, "both-side exit status is missing")
        exit_status = archive.read(files["profile-exit-status.txt"]).decode("utf-8")
        require("anchor_exit=0" in exit_status and "candidate_exit=0" in exit_status,
                f"one or both profiling runs failed: {exit_status!r}")
        result = {"schemaVersion": 1, "diagnosticOnly": True, "runId": RUN_ID,
                  "artifactId": ARTIFACT_ID, "diagnosticHead": DIAGNOSTIC_HEAD,
                  "artifactSha256": ARTIFACT_SHA256, "artifactBytes": len(raw_zip),
                  "scope": "CPU sampling attribution only; not an FPS result, GPU timing, or readiness qualification",
                  "sides": {}}
        for side, source in SOURCES.items():
            report_path = f"profiles/{side}/report.json"
            report = load_json(archive, files, report_path)
            require(report.get("exampleId") == "showcase-bezier-paths", f"{side}: wrong profiled source")
            require(report.get("label") == ("baseline" if side == "anchor" else "candidate"),
                    f"{side}: report label mismatch")
            require(report.get("source", {}).get("path", "").endswith("showcase_bezier_paths.py"),
                    f"{side}: wrong source path")
            require(report.get("source", {}).get("sha256") == BEZIER_SOURCE_SHA256,
                    f"{side}: canonical Bezier source bytes changed")
            require(report.get("runtimeIdentity", {}).get("sourceRevision") == source,
                    f"{side}: runtime source revision mismatch")
            require(report.get("cpuProfileDiagnostic", {}).get("packageIdentity") == report.get("runtimeIdentity"),
                    f"{side}: package identity evidence disagrees")
            require(report.get("cpuProfileDiagnostic", {}).get("scope", "").startswith("diagnostic-only whole warm pass"),
                    f"{side}: report lacks whole-warm scope")
            worker_result = report.get("workerCpuProfile") or {}
            require(worker_result.get("status") == "captured", f"{side}: worker capture did not finish")
            boundary = worker_result.get("boundary") or {}
            start = boundary.get("start") or {}
            stop = boundary.get("stop") or {}
            require(start.get("sample", {}).get("runGeneration") < stop.get("sample", {}).get("runGeneration"),
                    f"{side}: profile boundary did not cover a new warm Run")
            require(start.get("sample", {}).get("exampleId") == "showcase-bezier-paths" and
                    stop.get("sample", {}).get("exampleId") == "showcase-bezier-paths",
                    f"{side}: profile boundaries do not match the Bezier run")
            workers = {}
            for owner in ("authoring", "render"):
                expected = worker_result.get("profiles", {}).get(owner)
                require(expected is not None, f"{side}: missing {owner} worker profile")
                profile_path = f"profiles/{side}/{expected.get('file')}"
                require(profile_path in files, f"{side}: missing {owner} profile bytes")
                worker_summary = summarize_profile(archive.read(files[profile_path]), expected)
                require(worker_summary["actualFileSha256"] == expected["sha256"],
                        f"{side}: {owner} profile digest mismatch")
                require(worker_summary["nodeCount"] == expected["nodeCount"] and
                        worker_summary["sampleCount"] == expected["sampleCount"],
                        f"{side}: {owner} profile counts disagree with capture report")
                workers[owner] = worker_summary
            result["sides"][side] = {
                "sourceRevision": source, "sourceSha256": report["source"].get("sha256"),
                "runtimeBuildId": report["runtimeIdentity"].get("buildId"),
                "warmRunLifecycle": report.get("warmPassLifecycle"),
                "profileStartBoundary": start, "profileStopBoundary": stop,
                "workers": workers,
            }

    with open(args.output_json, "w", encoding="utf-8") as output:
        json.dump(result, output, indent=2, sort_keys=True)
        output.write("\n")


if __name__ == "__main__":
    main()
