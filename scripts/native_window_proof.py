#!/usr/bin/env python3
"""Observe the unchanged README application through its owned Linux/X11 window.

This is a presentation/lifecycle proof, not a frame-rate benchmark. Xlib is
imported only by qualify_window; pixel and lifecycle controls need no display.
"""
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
import importlib.metadata
import json
import math
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import threading
import time


WIDTH, HEIGHT = 320, 180
WINDOW_WIDTH, WINDOW_HEIGHT = 960, 540
INITIAL_X = {"square": 141.5, "circle": 177.5}
TRAVEL = 33.75  # 1.5 world units at 180 / 8 pixels per unit.
POSITION_TOLERANCE = 1.5
FINAL_HOLD_SECONDS = 1.0
TIMEOUTS = {"total": 45.0, "startup": 20.0, "observation": 12.0,
            "close": 8.0, "teardown": 2.0}


class NativeWindowProofError(RuntimeError):
    """The observed README application did not satisfy the native contract."""


def require(condition, message):
    if not condition:
        raise NativeWindowProofError(message)


def decode_x11_rgb(data, width, height, layout):
    """Decode only the explicitly checked TrueColor Xvfb image layout."""
    require(layout.get("depth") in (24, 32) and layout.get("bits_per_pixel") == 32
            and layout.get("scanline_pad") == 32 and layout.get("image_byte_order") == 0
            and layout.get("visual_class") == 4
            and (layout.get("red_mask"), layout.get("green_mask"), layout.get("blue_mask"))
            == (0xFF0000, 0xFF00, 0xFF), "unsupported X11 TrueColor image layout")
    require(width > 0 and height > 0 and len(data) == width * height * 4,
            "X11 image byte count differs from its geometry")
    rgb = bytearray(width * height * 3)
    rgb[0::3], rgb[1::3], rgb[2::3] = data[2::4], data[1::4], data[0::4]
    return bytes(rgb)


def normalize_rgb(rgb, width, height):
    require((width, height) == (WINDOW_WIDTH, WINDOW_HEIGHT),
            "README window must retain its default 960x540 client size")
    require(len(rgb) == width * height * 3, "RGB image byte count differs")
    # Default native rendering is exactly 3x the established lesson probe.
    # Center samples avoid another dependency or an interpolation color change.
    normalized = bytearray(WIDTH * HEIGHT * 3)
    for y in range(HEIGHT):
        for x in range(WIDTH):
            source = ((y * 3 + 1) * width + x * 3 + 1) * 3
            target = (y * WIDTH + x) * 3
            normalized[target:target + 3] = rgb[source:source + 3]
    return bytes(normalized)


def inspect_lesson_frame(rgb, seconds):
    require(len(rgb) == WIDTH * HEIGHT * 3, "normalized lesson byte count differs")
    colors = {name: {"pixels": 0, "sum_x": 0, "sum_y": 0} for name in INITIAL_X}
    title = equation = caption = 0
    for offset in range(0, len(rgb), 3):
        red, green, blue = rgb[offset:offset + 3]
        x, y = (offset // 3) % WIDTH, (offset // 3) // WIDTH
        name = ("square" if red < 140 and green > 120 and blue > 170 else
                "circle" if red > 190 and 70 < green < 180 and blue < 100 else None)
        if name:
            colors[name]["pixels"] += 1
            colors[name]["sum_x"] += x
            colors[name]["sum_y"] += y
        if min(red, green, blue) > 160:
            title += y < HEIGHT * 0.235
            equation += HEIGHT * 0.235 <= y < HEIGHT * 0.4
            caption += y >= HEIGHT * 0.62
    frame = {"seconds": seconds, "title_white_pixels": title,
             "equation_white_pixels": equation, "caption_white_pixels": caption}
    for name, color in colors.items():
        count = color["pixels"]
        frame[name] = {"pixels": count,
                       "center_x": color["sum_x"] / count if count else -1,
                       "center_y": color["sum_y"] / count if count else -1}
    return frame


def _validate_content(frame):
    require(frame["title_white_pixels"] >= 20, "README title is missing")
    require(frame["equation_white_pixels"] >= 15, "README equation is missing")
    for name in INITIAL_X:
        shape = frame[name]
        require(shape["pixels"] >= 50, f"README {name} is missing")
        require(math.isfinite(shape["center_x"]) and math.isfinite(shape["center_y"])
                and abs(shape["center_y"] - 98.5) <= POSITION_TOLERANCE,
                f"README {name} vertical position differs")


def _sequence_progress(observations, *, complete):
    require(observations, "no native README observations")
    intermediate = final = None
    previous = None
    for index, frame in enumerate(observations):
        stamp = frame["seconds"]
        require(isinstance(stamp, (int, float)) and not isinstance(stamp, bool)
                and math.isfinite(stamp) and stamp >= 0
                and (previous is None or stamp > previous["seconds"]),
                "native observation times are not finite and strictly ordered")
        _validate_content(frame)
        shifts = {name: frame[name]["center_x"] - x for name, x in INITIAL_X.items()}
        require(all(-POSITION_TOLERANCE <= shift <= TRAVEL + POSITION_TOLERANCE
                    for shift in shifts.values()), "README shape left its expected horizontal path")
        require(abs(shifts["square"] - shifts["circle"]) <= POSITION_TOLERANCE,
                "README shapes did not move together")
        at_final = all(abs(shift - TRAVEL) <= POSITION_TOLERANCE for shift in shifts.values())
        if index == 0:
            require(all(abs(shift) <= POSITION_TOLERANCE for shift in shifts.values()),
                    "initial README shape positions were not observed")
            require(frame["caption_white_pixels"] == 0, "README caption appeared before motion")
        if previous is not None:
            for name in INITIAL_X:
                require(frame[name]["center_x"] + 0.75 >= previous[name]["center_x"],
                        f"README {name} motion is out of order")
        if all(POSITION_TOLERANCE < shift < TRAVEL - POSITION_TOLERANCE
               for shift in shifts.values()):
            intermediate = stamp if intermediate is None else intermediate
        require(frame["caption_white_pixels"] == 0 or at_final,
                "README caption appeared before the final shape positions")
        if final is not None:
            require(at_final and frame["caption_white_pixels"] >= 15,
                    "native final state did not remain visible")
        elif at_final and frame["caption_white_pixels"] >= 15:
            require(intermediate is not None, "no actual intermediate native motion was observed")
            final = stamp
        previous = frame
    hold = observations[-1]["seconds"] - final if final is not None else 0
    finished = final is not None and hold >= FINAL_HOLD_SECONDS
    if complete:
        require(intermediate is not None, "no actual intermediate native motion was observed")
        require(final is not None, "final README shapes and late caption were not observed")
        require(finished, "native final state was not retained for at least one second")
    return {"initial_seconds": observations[0]["seconds"],
            "intermediate_seconds": intermediate, "final_seconds": final,
            "last_observed_seconds": observations[-1]["seconds"],
            "final_alive_seconds": hold, "complete": finished}


def validate_observations(observations):
    """Require real initial/intermediate/final pixels, without a cadence budget."""
    return _sequence_progress(observations, complete=True)


def validate_lifecycle(lifecycle):
    require(lifecycle.get("close_protocol") == "WM_DELETE_WINDOW"
            and lifecycle.get("close_sent") is True
            and lifecycle.get("alive_before_close") is True,
            "native application did not receive an owned-window protocol close")
    require(lifecycle.get("forced_teardown") is False,
            "forced native process teardown cannot qualify as clean shutdown")
    require(type(lifecycle.get("returncode")) is int and lifecycle["returncode"] == 0,
            "native application did not exit cleanly with status 0")
    names = ("final_seconds", "last_observed_seconds", "close_seconds", "exit_seconds")
    require(all(isinstance(lifecycle.get(name), (int, float)) and not isinstance(lifecycle[name], bool)
                and math.isfinite(lifecycle[name]) for name in names),
            "native lifecycle times are missing or invalid")
    require(0 <= lifecycle["final_seconds"]
            and lifecycle["last_observed_seconds"] - lifecycle["final_seconds"] >= FINAL_HOLD_SECONDS
            and lifecycle["close_seconds"] >= lifecycle["last_observed_seconds"]
            and lifecycle["exit_seconds"] >= lifecycle["close_seconds"],
            "native close/exit did not follow the retained final state")


@contextmanager
def _bounded_x11():
    # Xlib's synchronous select() has no timeout. A socket timeout alone does
    # not bound it; SIGALRM also bounds connection setup and every reply wait.
    require(threading.current_thread() is threading.main_thread(),
            "bounded X11 qualification must run on the main thread")
    require(signal.getitimer(signal.ITIMER_REAL) == (0.0, 0.0),
            "an existing process alarm prevents bounded X11 qualification")
    previous = signal.getsignal(signal.SIGALRM)

    def expired(_signal, _frame):
        raise NativeWindowProofError("native X11 qualification exceeded its total timeout")

    signal.signal(signal.SIGALRM, expired)
    signal.setitimer(signal.ITIMER_REAL, TIMEOUTS["total"])
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)


def _window_identity(window, atoms, X, Xatom):
    if window.get_attributes().map_state != X.IsViewable:
        return None
    pid = window.get_full_property(atoms["pid"], Xatom.CARDINAL)
    if pid is None or pid.format != 32 or len(pid.value) != 1:
        return None
    return int(pid.value[0])


def _window_title(window, atoms):
    title = window.get_full_property(atoms["name"], atoms["utf8"])
    value = title.value if title is not None and title.format == 8 else window.get_wm_name()
    return value.decode("utf8", errors="replace") if isinstance(value, bytes) else value


def _find_owned_window(connection, pid, atoms, X, Xatom, error):
    pending = list(connection.screen().root.query_tree().children)
    seen, found = set(), []
    while pending:
        window = pending.pop()
        if window.id in seen:
            continue
        seen.add(window.id)
        require(len(seen) <= 1024, "X11 window discovery exceeded its bounded tree size")
        try:
            if _window_identity(window, atoms, X, Xatom) == pid:
                if _window_title(window, atoms) == "Noon":
                    found.append(window)
            pending.extend(window.query_tree().children)
        except error.BadWindow:
            continue  # A window may disappear while the tree is being read.
    require(len(found) <= 1, "more than one mapped Noon window belongs to the README process")
    return found[0] if found else None


def _owned_geometry(window, pid, atoms, X, Xatom):
    require(_window_identity(window, atoms, X, Xatom) == pid
            and _window_title(window, atoms) == "Noon", "owned Noon window is no longer mapped")
    geometry = window.get_geometry()
    require((geometry.width, geometry.height) == (WINDOW_WIDTH, WINDOW_HEIGHT),
            "README window must retain its default 960x540 client size")
    return geometry


def _image_layout(connection, window, geometry):
    visual_id = window.get_attributes().visual
    visuals = [visual for depth in connection.screen().allowed_depths
               for visual in depth.visuals if visual.visual_id == visual_id]
    formats = [item for item in connection.display.info.pixmap_formats if item.depth == geometry.depth]
    require(len(visuals) == len(formats) == 1, "X11 window visual/pixmap format is ambiguous")
    visual, image_format = visuals[0], formats[0]
    layout = {"width": geometry.width, "height": geometry.height, "depth": geometry.depth,
              "visual_id": visual_id, "visual_class": visual.visual_class,
              "red_mask": visual.red_mask, "green_mask": visual.green_mask,
              "blue_mask": visual.blue_mask, "bits_per_pixel": image_format.bits_per_pixel,
              "scanline_pad": image_format.scanline_pad,
              "image_byte_order": connection.display.info.image_byte_order}
    decode_x11_rgb(bytes(4), 1, 1, layout)  # Validate the layout before any capture.
    return layout


def _save_ppm(path, rgb):
    path.write_bytes(f"P6\n{WINDOW_WIDTH} {WINDOW_HEIGHT}\n255\n".encode("ascii") + rgb)
    return {"file": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "width": WINDOW_WIDTH, "height": WINDOW_HEIGHT}


def qualify_window(binary, evidence_dir, environment, *, cwd=None):
    """Launch one external consumer, observe its pixels, and close it normally.

    Requires an already active local X11 display, normally CI's Xvfb. It never
    creates or retries a display and never changes the README application.
    """
    evidence = Path(evidence_dir)
    evidence.mkdir(parents=True, exist_ok=True)
    created = time.monotonic()
    environment = dict(environment)
    report = {"schema": 1, "status": "failed", "proof": "readme-native-x11",
              "started_utc": datetime.now(timezone.utc).isoformat(),
              "timeouts_seconds": TIMEOUTS.copy(),
              "environment": {name: environment[name] for name in
                              ("DISPLAY", "WAYLAND_DISPLAY", "WINIT_UNIX_BACKEND",
                               "WGPU_BACKEND", "LIBGL_ALWAYS_SOFTWARE") if name in environment},
              "command": [str(binary)], "cwd": str(cwd) if cwd is not None else None,
              "normalized_size": [WIDTH, HEIGHT], "screenshots": {},
              "stdout": "stdout.log", "stderr": "stderr.log",
              "startup_observations": [], "observations": [],
              "lifecycle": {"close_sent": False, "forced_teardown": False},
              "scope": "Visible lesson content/motion and native close; no frame-rate assertion."}
    process = connection = last_rgb = None
    failure = None
    try:
        require(sys.platform.startswith("linux"), "native window qualification requires Linux/X11")
        display_name = environment.get("DISPLAY", "")
        require(re.fullmatch(r":\d+(?:\.\d+)?", display_name) is not None,
                "native window qualification requires an existing local DISPLAY")
        require(not environment.get("WAYLAND_DISPLAY"), "native window qualification requires X11 selection")
        binary = Path(binary).resolve(strict=True)
        working = Path(cwd).resolve(strict=True) if cwd is not None else binary.parent
        require(binary.is_file() and os.access(binary, os.X_OK), "README binary is not executable")
        require(working.is_dir(), "README consumer working directory is missing")
        report.update(command=[str(binary)], cwd=str(working),
                      executable={"path": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()})
        with _bounded_x11():
            version = importlib.metadata.version("python-xlib")
            require(version == "0.33", "native window qualification requires python-xlib==0.33")
            from Xlib import X, Xatom, display, error, protocol

            report["python_xlib"] = version
            connection = display.Display(display_name)
            report["display"] = {"name": display_name, "vendor": connection.display.info.vendor,
                                 "release_number": connection.display.info.release_number,
                                 "root_depth": connection.screen().root_depth}
            atoms = {name: connection.intern_atom(value) for name, value in
                     {"pid": "_NET_WM_PID", "name": "_NET_WM_NAME", "utf8": "UTF8_STRING",
                      "protocols": "WM_PROTOCOLS", "delete": "WM_DELETE_WINDOW"}.items()}
            with (evidence / "stdout.log").open("wb") as stdout, (evidence / "stderr.log").open("wb") as stderr:
                launched = time.monotonic()
                process = subprocess.Popen([str(binary)], cwd=working, env=environment,
                                           stdout=stdout, stderr=stderr, start_new_session=True)
                report["pid"] = process.pid
                startup_deadline = launched + TIMEOUTS["startup"]
                window = None
                while window is None:
                    require(process.poll() is None, f"README process exited before its window (exit {process.returncode})")
                    require(time.monotonic() < startup_deadline, "owned Noon window did not appear before timeout")
                    window = _find_owned_window(connection, process.pid, atoms, X, Xatom, error)
                    if window is None:
                        time.sleep(0.03)
                report["window_id"] = window.id
                report["window_title"] = "Noon"
                geometry = _owned_geometry(window, process.pid, atoms, X, Xatom)
                layout = _image_layout(connection, window, geometry)
                report["display"]["image_layout"] = layout

                def capture():
                    require(process.poll() is None, f"README process exited before WM_DELETE_WINDOW (exit {process.returncode})")
                    geometry = _owned_geometry(window, process.pid, atoms, X, Xatom)
                    require(geometry.depth == layout["depth"], "owned window image depth changed")
                    reply = window.get_image(0, 0, geometry.width, geometry.height, X.ZPixmap, 0xFFFFFFFF)
                    require(reply is not None and reply.depth == layout["depth"]
                            and reply.visual == layout["visual_id"], "X11 image reply differs from the owned visual")
                    rgb = decode_x11_rgb(reply.data, geometry.width, geometry.height, layout)
                    frame = inspect_lesson_frame(normalize_rgb(rgb, geometry.width, geometry.height),
                                                 time.monotonic() - launched)
                    require(process.poll() is None, "README process exited while its pixels were observed")
                    return rgb, frame

                while True:
                    require(time.monotonic() < startup_deadline, "initial readable README pixels did not appear before timeout")
                    rgb, frame = capture()
                    last_rgb = rgb
                    report["startup_observations"].append(frame)
                    if "first_mapped" not in report["screenshots"]:
                        report["screenshots"]["first_mapped"] = _save_ppm(evidence / "first-mapped.ppm", rgb)
                    try:
                        _validate_content(frame)
                    except NativeWindowProofError:
                        time.sleep(0.03)
                        continue
                    _sequence_progress([frame], complete=False)
                    report["observations"].append(frame)
                    report["screenshots"]["initial"] = _save_ppm(evidence / "initial.ppm", rgb)
                    break
                observation_deadline = time.monotonic() + TIMEOUTS["observation"]
                while True:
                    require(time.monotonic() < observation_deadline,
                            "native motion, late caption, and retained final state were not observed before timeout")
                    time.sleep(0.03)
                    rgb, frame = capture()
                    last_rgb = rgb
                    report["observations"].append(frame)
                    progress = _sequence_progress(report["observations"], complete=False)
                    if progress["intermediate_seconds"] is not None and "intermediate" not in report["screenshots"]:
                        report["screenshots"]["intermediate"] = _save_ppm(evidence / "intermediate.ppm", rgb)
                    if progress["final_seconds"] is not None and "final" not in report["screenshots"]:
                        report["screenshots"]["final"] = _save_ppm(evidence / "final.ppm", rgb)
                    if progress["complete"]:
                        report["screenshots"]["held_final"] = _save_ppm(evidence / "held-final.ppm", rgb)
                        report["lifecycle"].update(validate_observations(report["observations"]))
                        break
                _owned_geometry(window, process.pid, atoms, X, Xatom)
                require(process.poll() is None, "README process exited before protocol close")
                require(atoms["delete"] in window.get_wm_protocols(), "owned Noon window does not support WM_DELETE_WINDOW")
                report["lifecycle"].update(alive_before_close=True, close_protocol="WM_DELETE_WINDOW")
                event = protocol.event.ClientMessage(window=window.id, client_type=atoms["protocols"],
                                                     data=(32, [atoms["delete"], X.CurrentTime, 0, 0, 0]))
                close_errors = []
                window.send_event(event, event_mask=0, propagate=False,
                                  onerror=lambda exc, _request: close_errors.append(str(exc)))
                connection.sync()
                require(not close_errors, f"WM_DELETE_WINDOW protocol request failed: {close_errors}")
                report["lifecycle"].update(close_sent=True, close_seconds=time.monotonic() - launched)
                process.wait(timeout=TIMEOUTS["close"])
                report["lifecycle"].update(returncode=process.returncode, exit_seconds=time.monotonic() - launched)
                validate_lifecycle(report["lifecycle"])
                report["executable"]["sha256_after"] = hashlib.sha256(binary.read_bytes()).hexdigest()
                require(report["executable"]["sha256_after"] == report["executable"]["sha256"],
                        "README executable changed during qualification")
                report["status"] = "passed"
    except Exception as exc:
        failure = exc
        report["error"] = {"type": type(exc).__name__, "message": str(exc)}
    finally:
        try:
            if process is not None:
                if process.poll() is None:
                    report["status"] = "failed"
                    report["lifecycle"]["forced_teardown"] = True
                    for action in (signal.SIGTERM, signal.SIGKILL):
                        try:
                            os.killpg(process.pid, action)
                            process.wait(timeout=TIMEOUTS["teardown"])
                            break
                        except ProcessLookupError:
                            break
                        except subprocess.TimeoutExpired:
                            continue
                report["lifecycle"]["returncode"] = process.poll()
            if connection is not None:
                # Display.close() flushes pending requests and can wait indefinitely.
                # This closes solely the observer's socket after evidence/teardown.
                connection.display.socket.close()
            if report["status"] != "passed" and last_rgb is not None:
                report["screenshots"]["failure_last"] = _save_ppm(evidence / "failure-last.ppm", last_rgb)
        except Exception as exc:
            report["status"] = "failed"
            report["cleanup_error"] = {"type": type(exc).__name__, "message": str(exc)}
            failure = failure or exc
        report["duration_seconds"] = time.monotonic() - created
        (evidence / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf8")
    if failure is not None:
        raise NativeWindowProofError(f"{failure}; evidence: {evidence / 'report.json'}") from failure
    require(report["status"] == "passed", f"native qualification failed; evidence: {evidence / 'report.json'}")
    return report
