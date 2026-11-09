#!/usr/bin/env python3
"""Build and qualify crates as an external consumer of the actual archives.

No registry publication occurs. Cargo packages the complete publishable workspace;
the consumer patches crates.io only to archives extracted outside the checkout.
Registry versions and checksums must remain within the reviewed workspace lock.
Use --allow-dirty explicitly for a working-tree candidate and --offline explicitly
when all package dependencies are cached. --wasm also checks the browser host.
--render runs and independently decodes the packaged ordinary Rust first lesson;
it requires a native graphics adapter, FFmpeg and ffprobe.
"""
from __future__ import annotations

import argparse
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
FIXTURE_TESTS = ("public_facade.rs", "authoring_errors.rs", "object_authoring_errors.rs")
LESSON_EXAMPLE = "examples/first_lesson.rs"
LESSON_WIDTH, LESSON_HEIGHT, LESSON_FPS, LESSON_FRAMES = 320, 180, 30, 60


class PackageError(ValueError):
    """A package or its installed dependency graph violates the release contract."""


def require(condition, message):
    if not condition:
        raise PackageError(message)


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def workspace_packages(root=ROOT):
    workspace = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf8"))
    inherited = workspace["workspace"].get("package", {})
    packages = {}
    for member in workspace["workspace"]["members"]:
        manifest = tomllib.loads((root / member / "Cargo.toml").read_text(encoding="utf8"))
        package = manifest["package"]
        name = package["name"]
        version = package["version"]
        if isinstance(version, dict):
            require(version == {"workspace": True}, f"unsupported version declaration in {member}")
            version = inherited["version"]
        require(name not in packages, f"duplicate workspace package: {name}")
        packages[name] = {"name": name, "version": version, "member": member,
                          "publish": package.get("publish") is not False and name != "noon-python"}
    return packages


def production_dependencies(manifest):
    """Include optional and target-specific edges; dev-only paths are irrelevant."""
    tables = [("root", manifest)]
    tables += [(f"target.{target}", table) for target, table in manifest.get("target", {}).items()]
    for location, table in tables:
        for kind in ("dependencies", "build-dependencies"):
            for alias, value in table.get(kind, {}).items():
                declaration = {"version": value} if isinstance(value, str) else value
                yield f"{location}.{kind}.{alias}", declaration.get("package", alias), declaration


def read_archive(path):
    """Read regular files only, rejecting path escapes, duplicate members and links."""
    with tarfile.open(path, "r:gz") as archive:
        members = archive.getmembers()
        names = [member.name for member in members]
        require(len(names) == len(set(names)), f"duplicate archive member in {path.name}")
        roots = set()
        for member in members:
            name = PurePosixPath(member.name)
            require(name.parts and not name.is_absolute() and ".." not in name.parts and "\\" not in member.name,
                    f"unsafe archive member: {member.name}")
            require(member.isfile() or member.isdir(), f"archive links/special files are unsupported: {member.name}")
            roots.add(name.parts[0])
        require(len(roots) == 1, f"archive must have exactly one root: {path.name}")
        prefix = next(iter(roots))
        files = {}
        for member in members:
            if member.isfile():
                name = str(PurePosixPath(member.name).relative_to(prefix))
                files[name] = archive.extractfile(member).read()
        return prefix, files


def validate_manifest(manifest, files, expected, workspace):
    package = manifest.get("package", {})
    name, version = expected["name"], expected["version"]
    require(package.get("name") == name and package.get("version") == version,
            f"archive identity differs from workspace package: {name}")
    for field in ("description", "license", "repository", "edition", "rust-version"):
        require(isinstance(package.get(field), str) and package[field].strip(),
                f"{name}: normalized package metadata is missing {field}")
    require(package.get("publish") is not False, f"{name}: nonpublishable package in release set")
    readme = package.get("readme")
    require(isinstance(readme, str) and readme in files and files[readme].strip(),
            f"{name}: packaged README is missing")
    require("LICENSE" in files and files["LICENSE"].strip(), f"{name}: packaged LICENSE is missing")
    edges = []
    for location, dependency, declaration in production_dependencies(manifest):
        require(isinstance(declaration, dict), f"{name}: invalid dependency {location}")
        require(not any(key in declaration for key in ("path", "git", "workspace")),
                f"{name}: nonregistry dependency survives normalization at {location}")
        requirement = declaration.get("version")
        require(isinstance(requirement, str) and requirement.strip(),
                f"{name}: dependency has no registry version at {location}")
        if dependency in workspace:
            target = workspace[dependency]
            require(target["publish"], f"{name}: depends on unpublished workspace package {dependency}")
            require(requirement in {target["version"], "=" + target["version"], "^" + target["version"], "~" + target["version"]},
                    f"{name}: internal dependency version differs at {location}: {requirement}")
            edges.append(dependency)
    return sorted(set(edges))


def validate_archives(paths, workspace):
    expected = {name: value for name, value in workspace.items() if value["publish"]}
    loaded = {}
    records = []
    for path in paths:
        prefix, files = read_archive(path)
        require("Cargo.toml" in files, f"missing normalized Cargo.toml: {path.name}")
        manifest = tomllib.loads(files["Cargo.toml"].decode("utf8"))
        name = manifest.get("package", {}).get("name")
        require(name in expected, f"unexpected release crate: {name}")
        require(name not in loaded, f"duplicate release crate: {name}")
        package = expected[name]
        require(prefix == f"{name}-{package['version']}", f"incorrect archive root for {name}: {prefix}")
        require(path.name == f"{prefix}.crate", f"incorrect archive filename: {path.name}")
        edges = validate_manifest(manifest, files, package, workspace)
        loaded[name] = {"prefix": prefix, "files": files, "manifest": manifest}
        records.append({"name": name, "version": package["version"], "file": path.name,
                        "bytes": path.stat().st_size, "sha256": digest(path),
                        "internal_dependencies": edges})
    require(set(loaded) == set(expected),
            f"release archive closure is incomplete: missing={sorted(set(expected) - set(loaded))}")
    for record in records:
        require(set(record["internal_dependencies"]) <= loaded.keys(),
                f"archive dependency closure is incomplete for {record['name']}")
    return loaded, sorted(records, key=lambda item: item["name"])


def validate_registry_lock(reviewed, resolved):
    allowed = {(item["name"], item["version"], item["source"]): item.get("checksum")
               for item in reviewed["package"] if item.get("source")}
    registry_count = 0
    for item in resolved["package"]:
        source = item.get("source")
        if not source:
            continue
        identity = item["name"], item["version"], source
        require(identity in allowed, f"consumer resolved an unreviewed dependency: {identity}")
        require(item.get("checksum") == allowed[identity], f"consumer dependency checksum changed: {identity}")
        if source.startswith("registry+"):
            require(isinstance(item.get("checksum"), str) and len(item["checksum"]) == 64,
                    f"registry dependency checksum is missing: {identity}")
            registry_count += 1
    return registry_count


def validate_external_graph(metadata, consumer, extracted, expected):
    consumer = consumer.resolve()
    extracted = extracted.resolve()
    found = set()
    for package in metadata["packages"]:
        if package.get("source") is not None:
            continue
        manifest = Path(package["manifest_path"]).resolve()
        if package["name"] == "noon-archive-consumer":
            require(manifest == consumer / "Cargo.toml", "consumer manifest escaped its temporary directory")
            continue
        require(package["name"] in expected, f"unexpected local dependency: {package['name']}")
        wanted = extracted / f"{package['name']}-{expected[package['name']]['version']}" / "Cargo.toml"
        require(manifest == wanted, f"consumer used a workspace/nonarchive dependency: {manifest}")
        found.add(package["name"])
    require(found == set(expected), f"consumer graph omitted release crates: {sorted(set(expected) - found)}")


def command(args, *, cwd, environment, capture=False):
    print("+ " + " ".join(map(str, args)), file=sys.stderr, flush=True)
    result = subprocess.run([str(value) for value in args], cwd=cwd, env=environment,
                            text=True, stdout=subprocess.PIPE if capture else sys.stderr,
                            stderr=subprocess.PIPE if capture else None)
    if result.returncode:
        if capture:
            print(result.stdout, file=sys.stderr)
            print(result.stderr, file=sys.stderr)
        raise PackageError(f"command failed ({result.returncode}): {' '.join(map(str, args))}")
    return result.stdout if capture else None


def source_identity(root, environment):
    commit = command(["git", "rev-parse", "HEAD"], cwd=root, environment=environment, capture=True).strip()
    status = command(["git", "status", "--porcelain", "--untracked-files=all"],
                     cwd=root, environment=environment, capture=True).splitlines()
    return {"commit": commit, "dirty": bool(status), "working_tree_changes": status}


def write_consumer(consumer, extracted, loaded, workspace, root=ROOT, *, render=False):
    consumer.mkdir()
    extracted.mkdir()
    for value in loaded.values():
        package_root = extracted / value["prefix"]
        for relative, data in value["files"].items():
            destination = package_root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
    versions = {name: package["version"] for name, package in workspace.items() if package["publish"]}
    text = [
        '[package]', 'name = "noon-archive-consumer"', 'version = "0.0.0"', 'edition = "2021"', 'publish = false',
        'default-run = "noon-archive-consumer"',
        '', '[workspace]', '', '[features]', 'default = []',
        'native-text = ["noon/native-text"]', 'typst = ["noon/typst"]',
        'bundled-fonts = ["noon/bundled-fonts"]',
        'product = ["noon/default", "native-text", "typst", "bundled-fonts"]',
        'native-host = ["dep:noon-native", "dep:noon-export"]', 'browser-host = ["dep:noon-web"]',
        '', '[dependencies]', f'noon = {{ version = "={versions["noon"]}", default-features = false }}',
        '', '[target.\'cfg(not(target_arch = "wasm32"))\'.dependencies]',
        f'noon-native = {{ version = "={versions["noon-native"]}", optional = true }}',
        f'noon-export = {{ version = "={versions["noon-export"]}", optional = true }}',
        '', '[target.\'cfg(target_arch = "wasm32")\'.dependencies]',
        f'noon-web = {{ version = "={versions["noon-web"]}", optional = true }}',
        '', '[patch.crates-io]',
    ]
    for name in sorted(versions):
        path = str((extracted / loaded[name]["prefix"]).resolve())
        text.append(f'{name} = {{ path = {json.dumps(path)} }}')
    text += ['', '[profile.dev]', 'debug = 0', '', '[profile.test]', 'debug = 0', '']
    if render:
        text += ['[[bin]]', 'name = "first-lesson-export"', 'path = "src/bin/first_lesson.rs"',
                 'required-features = ["product", "native-host"]', '']
    (consumer / "Cargo.toml").write_text("\n".join(text), encoding="utf8")
    (consumer / "src").mkdir()
    (consumer / "tests").mkdir()
    if render:
        require(LESSON_EXAMPLE in loaded["noon-export"]["files"],
                "noon-export archive is missing the ordinary first-lesson export example")
        (consumer / "src/bin").mkdir()
        # Execute the advertised source verbatim, with the engine supplied only
        # by the extracted packages. Do not substitute a workspace-only fixture.
        (consumer / "src/bin/first_lesson.rs").write_bytes(loaded["noon-export"]["files"][LESSON_EXAMPLE])
    fixture = root / "fixtures/provider-consumer"
    shutil.copyfile(fixture / "src/main.rs", consumer / "src/main.rs")
    for name in FIXTURE_TESTS:
        shutil.copyfile(fixture / "tests" / name, consumer / "tests" / name)
    (consumer / "tests/host_dependencies.rs").write_text('''
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
#[test]
fn native_and_export_hosts_are_available_to_consumers() {
    let _ = noon_native::NativeViewportConfig::default();
    let _ = noon_export::CaptureOptions::new(160, 90);
    let _ = noon_export::output::OutputOptions::mp4("not-created.mp4");
}

#[cfg(all(feature = "browser-host", target_arch = "wasm32"))]
use noon_web as _;
''', encoding="utf8")
    shutil.copyfile(root / "Cargo.lock", consumer / "Cargo.lock")
    shutil.copyfile(root / "rust-toolchain.toml", consumer / "rust-toolchain.toml")


def validate_lesson_timing(metadata):
    streams = metadata.get("streams", [])
    require(len(streams) == 1, "first lesson must contain exactly one visual stream")
    stream = streams[0]
    require(stream.get("codec_name") == "h264", "first lesson is not H.264")
    require((stream.get("width"), stream.get("height")) == (LESSON_WIDTH, LESSON_HEIGHT),
            "first-lesson dimensions differ from its documented example")
    require(int(stream.get("nb_frames", -1)) == LESSON_FRAMES, "first-lesson frame count differs")
    require(Fraction(stream["avg_frame_rate"]) == LESSON_FPS, "first-lesson frame rate differs")
    tick = Fraction(stream["time_base"])
    require(int(stream.get("start_pts", -1)) == 0, "first-lesson output does not start at zero")
    require(int(stream["duration_ts"]) * tick == 2, "first-lesson duration is not two seconds")
    packets = metadata.get("packets", [])
    require(len(packets) == LESSON_FRAMES, "first-lesson packet count differs")
    for index, packet in enumerate(packets):
        require(int(packet["pts"]) * tick == Fraction(index, LESSON_FPS),
                f"first-lesson presentation time differs at frame {index}")
        require(int(packet["duration"]) * tick == Fraction(1, LESSON_FPS),
                f"first-lesson sample duration differs at frame {index}")
    return stream


def validate_lesson_observations(observations):
    require(len(observations) == LESSON_FRAMES, "first-lesson decoded frame count differs")
    pixels_per_unit = LESSON_HEIGHT / 8
    for index, frame in enumerate(observations):
        time = index / LESSON_FPS
        shift = 1.5 * min(max(time - 0.5, 0.0), 1.0)
        require(frame["title_white_pixels"] >= 20,
                f"first-lesson title is missing at frame {index}")
        require(frame["equation_white_pixels"] >= 15,
                f"first-lesson equation is missing at frame {index}")
        if index < 45:
            require(frame["caption_white_pixels"] == 0,
                    f"first-lesson late caption leaked into frame {index}")
        else:
            require(frame["caption_white_pixels"] >= 15,
                    f"first-lesson late caption is missing at frame {index}")
        for name, initial_x in (("square", -0.8), ("circle", 0.8)):
            require(frame[name]["pixels"] >= 50, f"first-lesson {name} is missing at frame {index}")
            expected_x = (LESSON_WIDTH - 1) / 2 + pixels_per_unit * (initial_x + shift)
            require(abs(frame[name]["center_x"] - expected_x) <= 1.25,
                    f"first-lesson {name} motion/order differs at frame {index}")
            # Absolute raster tolerance can exceed one frame's displacement.
            # The known moving interval must also progress in presentation order.
            if 16 <= index <= 45:
                require(frame[name]["center_x"] > observations[index - 1][name]["center_x"],
                        f"first-lesson {name} did not progress at frame {index}")


def inspect_lesson_pixels(decoded):
    frame_bytes = LESSON_WIDTH * LESSON_HEIGHT * 3
    require(len(decoded) == LESSON_FRAMES * frame_bytes,
            "first-lesson decoded byte count differs")
    observations = []
    for index in range(LESSON_FRAMES):
        frame = memoryview(decoded)[index * frame_bytes:(index + 1) * frame_bytes]
        square_x, circle_x = [], []
        title_white = equation_white = caption_white = 0
        for offset in range(0, frame_bytes, 3):
            red, green, blue = frame[offset:offset + 3]
            x, y = (offset // 3) % LESSON_WIDTH, (offset // 3) // LESSON_WIDTH
            if red < 140 and green > 120 and blue > 170:
                square_x.append(x)
            if red > 190 and 70 < green < 180 and blue < 100:
                circle_x.append(x)
            if min(red, green, blue) > 160:
                title_white += y < LESSON_HEIGHT * 0.235
                equation_white += LESSON_HEIGHT * 0.235 <= y < LESSON_HEIGHT * 0.4
                caption_white += y >= LESSON_HEIGHT * 0.62
        observations.append({
            "frame": index,
            "square": {"pixels": len(square_x), "center_x": sum(square_x) / len(square_x) if square_x else -1},
            "circle": {"pixels": len(circle_x), "center_x": sum(circle_x) / len(circle_x) if circle_x else -1},
            "title_white_pixels": title_white, "equation_white_pixels": equation_white,
            "caption_white_pixels": caption_white,
        })
    return observations


def verify_lesson_movie(movie, output, environment):
    evidence = output / "first-lesson"
    evidence.mkdir(exist_ok=True)
    retained_movie = evidence / "lesson.mp4"
    shutil.copyfile(movie, retained_movie)
    probe_command = ["ffprobe", "-v", "error", "-show_streams", "-show_packets", "-of", "json", str(movie)]
    metadata = json.loads(command(probe_command, cwd=movie.parent, environment=environment, capture=True))
    (evidence / "probe.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf8")
    stream = validate_lesson_timing(metadata)
    decode_command = ["ffmpeg", "-v", "error", "-nostdin", "-i", str(movie), "-map", "0:v:0",
                      "-frames:v", str(LESSON_FRAMES + 1), "-fps_mode", "passthrough",
                      "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"]
    decoded = subprocess.run(decode_command, cwd=movie.parent, env=environment,
                             capture_output=True, timeout=60)
    require(decoded.returncode == 0, f"first-lesson decode failed: {decoded.stderr.decode(errors='replace')}")
    observations = inspect_lesson_pixels(decoded.stdout)
    (evidence / "decoded-observations.json").write_text(json.dumps(observations, indent=2) + "\n", encoding="utf8")
    validate_lesson_observations(observations)
    require(sorted(path.name for path in movie.parent.iterdir()) == [movie.name],
            "first-lesson export left private staging files")
    return {"file": str(retained_movie.relative_to(output)), "sha256": digest(retained_movie),
            "bytes": retained_movie.stat().st_size, "frames": len(observations),
            "width": stream["width"], "height": stream["height"], "fps": stream["avg_frame_rate"],
            "source_seconds": 2, "all_frames_decoded": True, "title_equation_present": True,
            "parallel_group_motion": True, "late_caption_timing": True,
            "media_tools": {tool: command([tool, "-version"], cwd=movie.parent, environment=environment,
                                           capture=True).splitlines()[0] for tool in ("ffmpeg", "ffprobe")},
            "probe_command": probe_command, "decode_command": decode_command}


def resolve_consumer_metadata(consumer, *, root, environment, offline):
    # Packaging with --no-verify can leave a clean Cargo cache without dependency
    # sources. Fetch the reviewed workspace lock before the consumer's offline
    # resolution; never relax that resolution or update the reviewed versions.
    fetch = ["cargo", "fetch", "--locked"]
    if offline:
        fetch.append("--offline")
    command(fetch, cwd=root, environment=environment)
    return json.loads(command(["cargo", "metadata", "--offline", "--all-features", "--format-version", "1"],
                              cwd=consumer, environment=environment, capture=True))


def qualify_consumer(loaded, workspace, output, *, wasm, offline, environment, render=False, root=ROOT):
    expected = {name: value for name, value in workspace.items() if value["publish"]}
    reviewed = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf8"))
    checks = []
    with tempfile.TemporaryDirectory(prefix="noon-crate-consumer-") as directory:
        outside = Path(directory).resolve()
        require(not outside.is_relative_to(root.resolve()), "consumer temporary directory is inside checkout")
        consumer, extracted = outside / "consumer", outside / "archives"
        write_consumer(consumer, extracted, loaded, workspace, root, render=render)
        # Cargo updates only the local root/path identities. Offline resolution
        # and a full checksum comparison prevent a new registry resolution from
        # silently becoming the package proof.
        metadata = resolve_consumer_metadata(consumer, root=root, environment=environment, offline=offline)
        registry_count = validate_registry_lock(reviewed, tomllib.loads((consumer / "Cargo.lock").read_text(encoding="utf8")))
        validate_external_graph(metadata, consumer, extracted, expected)
        frozen_lock = digest(consumer / "Cargo.lock")
        commands = [
            ("minimal authoring executable", ["run", "--no-default-features"], []),
            ("minimal public API contracts", ["test", "--no-default-features", "--tests"], []),
            ("product public API contracts", ["test", "--features", "product", "--tests"], ["product"]),
            ("native/export dependency compilation", ["check", "--features", "product,native-host", "--all-targets"], ["product", "native-host"]),
        ]
        if wasm:
            commands.append(("WASM product and browser host", ["check", "--features", "product,browser-host", "--target", "wasm32-unknown-unknown", "--tests"], ["product", "browser-host"]))
        for title, arguments, features in commands:
            invocation = ["cargo", *arguments, "--locked"]
            if offline:
                invocation.append("--offline")
            command(invocation, cwd=consumer, environment=environment)
            require(digest(consumer / "Cargo.lock") == frozen_lock, f"consumer lock changed during {title}")
            checks.append({"name": title, "status": "passed", "command": invocation, "features": features})
        lesson = None
        if render:
            movie_directory = outside / "rendered-lesson"
            movie_directory.mkdir()
            movie = movie_directory / "lesson.mp4"
            invocation = ["cargo", "run", "--features", "product,native-host", "--bin", "first-lesson-export", "--locked"]
            if offline:
                invocation.append("--offline")
            invocation += ["--", str(movie)]
            producer = command(invocation, cwd=consumer, environment=environment, capture=True)
            require(digest(consumer / "Cargo.lock") == frozen_lock, "consumer lock changed during first-lesson export")
            evidence = output / "first-lesson"
            evidence.mkdir(exist_ok=True)
            (evidence / "producer.txt").write_text(producer, encoding="utf8")
            lesson = verify_lesson_movie(movie, output, environment)
            lesson["example_sha256"] = hashlib.sha256(loaded["noon-export"]["files"][LESSON_EXAMPLE]).hexdigest()
            lesson["producer_output"] = producer
            checks.append({"name": "packaged ordinary Rust first-lesson export", "status": "passed",
                           "command": invocation, "features": ["product", "native-host"]})
        shutil.copyfile(consumer / "Cargo.lock", output / "consumer-Cargo.lock")
        return {"outside_checkout": True, "archive_paths_only": True,
                "reviewed_registry_dependencies": registry_count, "consumer_lock_sha256": frozen_lock,
                "fixture_tests": list(FIXTURE_TESTS), "checks": checks, "wasm_requested": wasm,
                "render_requested": render, "first_lesson": lesson}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "dist/rust")
    parser.add_argument("--allow-dirty", action="store_true", help="explicitly package a working-tree candidate")
    parser.add_argument("--offline", action="store_true", help="require all Cargo dependencies to be cached")
    parser.add_argument("--wasm", action="store_true", help="also compile product APIs and noon-web for wasm32-unknown-unknown")
    parser.add_argument("--render", action="store_true", help="run the packaged ordinary Rust lesson and independently decode its complete MP4")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ)
    environment.setdefault("CARGO_INCREMENTAL", "0")
    environment.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    environment.setdefault("CARGO_PROFILE_TEST_DEBUG", "0")
    # Reuse compiled registry dependencies while keeping every consumer source
    # outside the workspace. Cargo fingerprints the distinct extracted sources.
    environment.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
    target_setting = Path(environment["CARGO_TARGET_DIR"])
    if not target_setting.is_absolute():
        environment["CARGO_TARGET_DIR"] = str((ROOT / target_setting).resolve())
    report = {"schema": 1, "status": "running", "registry_publication": False,
              "allow_dirty": args.allow_dirty, "offline": args.offline,
              "host": {"system": platform.system(), "release": platform.release(), "machine": platform.machine()},
              "python": platform.python_version()}
    try:
        report["source"] = source_identity(ROOT, environment)
        require(args.allow_dirty or not report["source"]["dirty"], "working tree is dirty; pass --allow-dirty for a local candidate")
        report["rustc"] = command(["rustc", "-Vv"], cwd=ROOT, environment=environment, capture=True).strip()
        report["cargo"] = command(["cargo", "-V"], cwd=ROOT, environment=environment, capture=True).strip()
        workspace = workspace_packages()
        expected_files = {f"{name}-{value['version']}.crate" for name, value in workspace.items() if value["publish"]}
        unexpected = {path.name for path in output.glob("*.crate")} - expected_files
        require(not unexpected, f"output directory contains unrelated/stale archives: {sorted(unexpected)}")
        package_command = ["cargo", "package", "--workspace", "--exclude", "noon-python", "--no-verify", "--locked"]
        if args.allow_dirty:
            package_command.append("--allow-dirty")
        if args.offline:
            package_command.append("--offline")
        command(package_command, cwd=ROOT, environment=environment)
        metadata_command = ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"]
        if args.offline:
            metadata_command.append("--offline")
        target = Path(json.loads(command(metadata_command, cwd=ROOT, environment=environment, capture=True))["target_directory"])
        archives = [target / "package" / name for name in sorted(expected_files)]
        require(all(path.is_file() for path in archives), "Cargo did not produce the complete expected archive set")
        loaded, records = validate_archives(archives, workspace)
        for path in archives:
            shutil.copyfile(path, output / path.name)
        (output / "SHA256SUMS").write_text("".join(f"{record['sha256']}  {record['file']}\n" for record in records), encoding="utf8")
        report["packages"] = records
        report["package_command"] = package_command
        report["consumer"] = qualify_consumer(loaded, workspace, output, wasm=args.wasm, offline=args.offline,
                                              environment=environment, render=args.render)
        report["status"] = "passed"
    except (PackageError, OSError, ValueError, KeyError, subprocess.SubprocessError, tarfile.TarError) as error:
        report["status"] = "failed"
        report["error"] = str(error)
        print(f"Rust package qualification failed: {error}", file=sys.stderr)
    (output / "release-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
