"""Read-only Linux task sampling around a child. Diagnostic, never qualification."""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def parse_stat(text: str) -> dict:
    left, right = text.index("("), text.rindex(")")
    fields = text[right + 1:].split()
    if len(fields) < 39:
        raise ValueError("truncated proc stat")
    return {"pid": int(text[:left]), "name": text[left+1:right],
            "state": fields[0], "ppid": int(fields[1]), "pgrp": int(fields[2]),
            "utime": int(fields[11]), "stime": int(fields[12]),
            "start_ticks": int(fields[19]), "last_cpu": int(fields[36])}


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="strict")


def descendants(table: dict, root: int, known: set) -> set:
    # Keep a previously seen identity after reparenting; never trust a reused PID.
    selected = {pid for pid, stat in table.items()
                if pid == root or (pid, stat["start_ticks"]) in known}
    while True:
        more = {pid for pid, stat in table.items() if stat["ppid"] in selected}
        if more <= selected:
            return selected
        selected |= more


def collect(root: int, known: set, proc: Path = Path("/proc")) -> dict:
    wall_ns, started, cpu_started = time.time_ns(), time.monotonic_ns(), time.process_time_ns()
    table, errors = {}, []
    for directory in proc.iterdir():
        if not directory.name.isdecimal():
            continue
        try:
            stat = parse_stat(read_text(directory / "stat"))
            table[stat["pid"]] = stat
        except (FileNotFoundError, ProcessLookupError):
            continue  # A normal exit during the scan; not a zero-valued sample.
        except (PermissionError, ValueError, UnicodeError) as error:
            errors.append({"path": str(directory), "error": type(error).__name__})
    selected = descendants(table, root, known)
    tasks = []
    for pid in sorted(selected):
        known.add((pid, table[pid]["start_ticks"]))
        try:
            directories = list((proc / str(pid) / "task").iterdir())
        except (FileNotFoundError, ProcessLookupError):
            continue
        for directory in directories:
            try:
                stat = parse_stat(read_text(directory / "stat"))
                sched = [int(x) for x in read_text(directory / "schedstat").split()]
                if len(sched) != 3 or any(x < 0 for x in sched):
                    raise ValueError("invalid schedstat")
                tasks.append({**stat, "tgid": pid,
                              "process_start_ticks": table[pid]["start_ticks"],
                              "runtime_ns": sched[0], "runqueue_ns": sched[1],
                              "timeslices": sched[2]})
            except (FileNotFoundError, ProcessLookupError):
                errors.append({"tgid": pid, "tid": directory.name, "error": "exited-during-scan"})
            except (PermissionError, ValueError, UnicodeError) as error:
                errors.append({"tgid": pid, "tid": directory.name, "error": type(error).__name__})
    system = {}
    for name in ("stat", "pressure/cpu", "pressure/memory", "loadavg", "sys/kernel/sched_schedstats"):
        try:
            system[name] = read_text(proc / name)
        except OSError as error:
            system[name] = {"unavailable": type(error).__name__}
    return {"kind": "sample", "wall_ns": wall_ns, "monotonic_ns": started, "observer_process_cpu_ns": cpu_started,
            "scan_wall_ns": time.monotonic_ns() - started,
            "observer_cpu_ns": time.process_time_ns() - cpu_started,
            "tasks": tasks, "system": system, "errors": errors}


def stop_known(child, known: set, grace: float) -> None:
    # Limit cleanup to the child/session plus exact descendant start identities.
    for sig in (signal.SIGTERM, signal.SIGKILL):
        for pid, start in list(known):
            try:
                if parse_stat(read_text(Path(f"/proc/{pid}/stat")))["start_ticks"] == start:
                    os.kill(pid, sig)
            except (FileNotFoundError, ProcessLookupError):
                pass
        if child.poll() is None:
            try:
                os.killpg(child.pid, sig)
            except ProcessLookupError:
                pass
        if sig == signal.SIGTERM:
            time.sleep(grace)


def observe(command: list[str], output: Path, interval: float, deadline: float) -> int:
    if sys.platform != "linux" or not command:
        raise ValueError("Linux and a child command are required")
    if not 0.02 <= interval <= 1 or not 0 < deadline <= 600:
        raise ValueError("unsupported fixed observer budget")
    known, child, timed_out, result = set(), None, False, None
    samples, active_schedstats = 0, False
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8") as stream:
        def emit(record):
            stream.write(json.dumps(record, separators=(",", ":")) + "\n")
            stream.flush()
        emit({"kind": "start", "qualification": False, "mergeApproval": False,
              "interval_seconds": interval, "deadline_seconds": deadline,
              "clock_ticks": os.sysconf("SC_CLK_TCK"), "observer_pid": os.getpid(),
              "wall_ns": time.time_ns(), "observer_process_cpu_ns": time.process_time_ns(), "command": command})
        try:
            child = subprocess.Popen(command, start_new_session=True)
            started, next_tick = time.monotonic(), time.monotonic()
            while True:
                record = collect(child.pid, known)
                active_schedstats |= any(t["runtime_ns"] > 0 for t in record["tasks"])
                emit(record)
                samples += 1
                result = child.poll()
                if result is not None:
                    break
                if time.monotonic() - started >= deadline:
                    timed_out = True
                    break
                # No burst of catch-up polling when a sample is slow.
                next_tick = max(next_tick + interval, time.monotonic())
                time.sleep(max(0, next_tick - time.monotonic()))
        except BaseException as error:
            emit({"kind": "error", "type": type(error).__name__, "message": str(error)})
            raise
        finally:
            if child is not None:
                stop_known(child, known, 0.25)
                result = child.wait(timeout=5)
            emit({"kind": "end", "returncode": result, "timed_out": timed_out,
                  "samples": samples, "active_schedstats": active_schedstats,
                  "wall_ns": time.time_ns(), "observer_process_cpu_ns": time.process_time_ns(), "qualification": False})
    if timed_out:
        return 124
    if result != 0:
        return result if result is not None and result > 0 else 1
    return 0 if active_schedstats else 2


if __name__ == "__main__":
    def interrupted(signum, _frame):
        raise KeyboardInterrupt(f"signal {signum}")
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--interval-ms", type=int, default=100)
    parser.add_argument("--deadline-seconds", type=float, default=420)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    sys.exit(observe(command, args.output, args.interval_ms / 1000, args.deadline_seconds))
