#!/usr/bin/env python3
"""One-shot, provenance-pinned feasibility probe for SwiftShader.ini worker count.

Read-only monitoring: original product harness and packages are never rewritten.
Each mode is run exactly once in a distinct working directory. No performance
acceptance metric or select-the-best trial logic is implemented here.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time

WORKER = re.compile(r"^Thread<\d\d>$")


def validate_plan(plan: dict) -> None:
    assert plan["schema"] == 1 and plan["diagnosticOnly"] is True
    assert plan["qualification"] is False and plan["mergeApproval"] is False
    assert plan["studyId"] == "1933-swiftshader-worker-cap-20261008-01"
    assert plan["originalProducerRun"] == 37710961499
    assert plan["harnessSha"] == "f5ec15c7a1a9a70c368e141abfc740f5081efc4e"
    assert plan["baseline"] == {
        "source": "4d8d61646cabb1c8c4e47c72bd3d7e77802d6c0f",
        "artifactId": 11523896652,
        "zipSha256": "23eb7282da981e6272c18b2ba59a9ff8863d45406cd00efae73fc87657d54071",
        "buildId": "810ea10b60bc3ea023a9c20ff21bc918a791494ba3e7c595bdc657dbea95fc15",
    }
    assert plan["modes"] == [
        {"name": "default", "threadCount": None},
        {"name": "two-workers", "threadCount": 2},
    ]
    assert plan["sourceExample"] == "parity-square-and-circle" and plan["pairIndex"] == 1
    assert plan["expectedBrowser"] == "151.0.7922.34"
    assert plan["expectedGpuMode"] == "software-WebGL"
    assert plan["expectedBackend"] == "WebGL2"
    assert plan["pollIntervalSeconds"] == 0.5
    assert plan["pairTimeoutSeconds"] == 180
    assert plan["execution"] == {"event": "pull_request.opened", "attempt": 1, "retry": False}


def parse_ppid(stat: str) -> int:
    # /proc/$pid/stat comm may contain spaces or literal closing parens.
    suffix = stat.rsplit(") ", 1)[1].split()
    return int(suffix[1])


def family_of(root_pid: int, processes: dict[int, int]) -> set[int]:
    children = {root_pid}
    while True:
        extra = {pid for pid, parent in processes.items() if parent in children}
        if extra <= children:
            break
        children |= extra
    return children


def census(root_pid: int, proc: Path = Path("/proc")) -> dict:
    processes = {}
    for item in proc.iterdir():
        if not item.name.isdecimal():
            continue
        try:
            processes[int(item.name)] = parse_ppid((item / "stat").read_text())
        except (OSError, ValueError, IndexError):
            pass
    descendants = family_of(root_pid, processes)
    groups = {}
    for pid in sorted(descendants):
        path = proc / str(pid)
        try:
            leader = (path / "comm").read_text().strip()
            tids = sorted((path / "task").iterdir(), key=lambda x: int(x.name))
        except (OSError, ValueError):
            continue
        matching = []
        for t in tids:
            try:
                name = (t / "comm").read_text().strip()
            except OSError:
                continue
            if WORKER.fullmatch(name):
                matching.append([int(t.name), name])
        if matching:
            groups[str(pid)] = {"leader": leader, "workers": matching}
    return {"atNs": time.time_ns(), "workerCount": sum(len(v["workers"]) for v in groups.values()),
            "groups": groups}


def run_pair(mode: dict, plan: dict, harness: Path, baseline: Path, output: Path) -> dict:
    name = mode["name"]
    home = output / "working" / name
    home.mkdir(parents=True)  # existing/colliding output fails closed
    if mode["threadCount"] is not None:
        (home / "SwiftShader.ini").write_text(
            f'[Processor]\nThreadCount={mode["threadCount"]}\n', encoding="ascii"
        )
    work = output / "trials" / name
    work.mkdir(parents=True)
    log_path = work / "pair.log"
    env = {k: v for k, v in os.environ.items() if not k.startswith("NOON_PRODUCT_")}
    env.update({
        "NOON_PRODUCT_REFERENCE_ROOT": str(baseline),
        "NOON_PRODUCT_CANDIDATE_ROOT": str(baseline),
        "NOON_PRODUCT_REFERENCE_ARTIFACT_ROLE": "baseline",
        "NOON_PRODUCT_EVIDENCE_ROOT": str(work),
        "NOON_PRODUCT_EXAMPLE": plan["sourceExample"],
        "NOON_PRODUCT_PAIR_INDEX": str(plan["pairIndex"]),
        "NOON_PRODUCT_PORT": "4205",
    })
    command = [os.environ.get("NOON_NODE_BIN", "node"),
               str(harness / "scripts/playground-product-pair.mjs")]
    started = time.monotonic()
    samples = []
    with log_path.open("xb") as log:
        proc = subprocess.Popen(command, cwd=home, env=env, stdin=subprocess.DEVNULL,
                                stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        observer_start = time.process_time_ns()
        try:
            while proc.poll() is None:
                if time.monotonic() - started > plan["pairTimeoutSeconds"]:
                    raise TimeoutError(f"frozen pair timed out in mode {name}")
                tick = time.process_time_ns()
                sample = census(proc.pid)
                sample["observerCpuNs"] = time.process_time_ns() - tick
                samples.append(sample)
                time.sleep(plan["pollIntervalSeconds"])
            if proc.returncode != 0:
                raise RuntimeError(f"frozen pair failed mode {name}: exit {proc.returncode}")
        finally:
            if proc.poll() is None:
                try:
                    os.killpg(proc.pid, signal.SIGTERM)
                    proc.wait(timeout=5)
                except (ProcessLookupError, subprocess.TimeoutExpired):
                    try:
                        os.killpg(proc.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    proc.wait()
            observer_cpu = time.process_time_ns() - observer_start
        (work / "thread-census.json").write_text(json.dumps(samples, indent=2) + "\n")
    reports = {}
    for label in ("baseline", "candidate"):
        report_path = work / label / "trial-1" / "report.json"
        report = json.loads(report_path.read_text())
        assert report["exampleId"] == plan["sourceExample"]
        assert report["runtimeIdentity"]["buildId"] == plan["baseline"]["buildId"]
        assert report["runtimeIdentity"]["sourceRevision"] == plan["baseline"]["source"]
        assert report["runtime"]["browserVersion"] == plan["expectedBrowser"]
        assert report["runtime"]["backend"] == plan["expectedBackend"]
        assert report["runtime"]["gpuMode"] == plan["expectedGpuMode"]
        frame = report["fps"]
        start = frame["clockOriginMs"] + frame["startRendererAt"]
        end = frame["clockOriginMs"] + frame["endRendererAt"]
        in_window = [s for s in samples if start <= s["atNs"] / 1e6 <= end]
        reports[label] = {"scoreStartMs": start, "scoreEndMs": end,
                          "observedSamples": len(in_window),
                          "workersDuringScore": [s["workerCount"] for s in in_window],
                          "workerMax": max([s["workerCount"] for s in in_window], default=None)}
    return {"name": name, "threadCountSetting": mode["threadCount"],
            "wallMs": round((time.monotonic() - started) * 1000),
            "observerCpuMs": observer_cpu / 1e6, "allWorkerMax": max(s["workerCount"] for s in samples),
            "reports": reports}


def run(plan_file: Path, harness: Path, baseline: Path, output: Path) -> dict:
    plan_bytes = plan_file.read_bytes()
    plan = json.loads(plan_bytes)
    validate_plan(plan)
    assert os.environ.get("GITHUB_RUN_ATTEMPT") == "1"
    assert os.environ.get("GITHUB_EVENT_NAME") == "pull_request"
    assert json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text()).get("action") == "opened"
    assert subprocess.check_output(["git", "-C", str(harness), "rev-parse", "HEAD"], text=True).strip() == plan["harnessSha"]
    assert subprocess.check_output(["git", "-C", str(baseline), "rev-parse", "HEAD"], text=True).strip() == plan["baseline"]["source"]
    assert not output.exists(), "study cannot overwrite or resume previous output"
    output.mkdir(parents=True)
    summary = {"studyId": plan["studyId"], "qualification": False, "mergeApproval": False,
               "planSha256": hashlib.sha256(plan_bytes).hexdigest(), "acquisitionComplete": False,
               "completedModes": 0, "modes": [], "errors": []}
    try:
        for mode in plan["modes"]:
            result = run_pair(mode, plan, harness, baseline, output)
            summary["modes"].append(result)
            summary["completedModes"] += 1
        d, c = summary["modes"]
        dmax = [v["workerMax"] for v in d["reports"].values()]
        cmax = [v["workerMax"] for v in c["reports"].values()]
        summary["controlObserved"] = min(dmax) >= 3 and 0 < max(cmax) <= 2
        summary["status"] = "control-honored" if summary["controlObserved"] else "not-confirmed"
        summary["acquisitionComplete"] = True
    except BaseException as exc:
        summary["errors"].append(f"{type(exc).__name__}: {exc}")
        summary["status"] = "incomplete"
        raise
    finally:
        (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


if __name__ == "__main__":
    assert len(sys.argv) == 5, "usage: study.py PLAN HARNESS BASELINE OUTPUT"
    result = run(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve(),
                 Path(sys.argv[3]).resolve(), Path(sys.argv[4]).resolve())
    print(json.dumps({"studyId": result["studyId"], "status": result["status"],
                      "completedModes": result["completedModes"]}))
