#!/usr/bin/env python3
"""Qualify one isolated provider configuration; optionally measure cold/warm costs.

Correctness runs preserve compiler wrappers and reuse the caller's target directory.
Explicit --measure runs use fresh compiler output and disable wrappers/incremental
compilation. Downloads are prefetched; warm means an identical no-edit rebuild.
"""
from __future__ import annotations

import argparse
import gzip
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
CONFIGS = {
    "minimal": "",
    "native-text": "native-text",
    "native-bundled": "native-text,bundled-fonts",
    "typst": "typst",
    "product": "product",
}
COMMON = {"noon", "noon-core", "noon-compile", "noon-runtime"}


def check_graph(config: str, text: str) -> set[str]:
    """Validate Cargo's active normal/build tree, not metadata's package inventory."""
    if config not in CONFIGS:
        raise ValueError(f"unknown provider configuration: {config}")
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    names: set[str] = set()
    features: dict[str, set[str]] = {}
    for line in lines:
        # `{f}` is the activated feature set, unlike the feature definitions in
        # cargo metadata. Keep the delimiter explicit so missing data fails closed.
        match = re.fullmatch(
            r"([A-Za-z0-9_-]+) v[0-9][^ ]*(?: \(.*\))? features=([^ ]*)(?: \(\*\))?",
            line,
        )
        if not match:
            raise ValueError(f"unreadable Cargo tree line: {line!r}")
        name, enabled = match.groups()
        names.add(name)
        features.setdefault(name, set()).update(filter(None, enabled.split(",")))
    required = set(COMMON)
    if config in {"native-text", "native-bundled", "product"}:
        required |= {"noon-text-native", "swash"}
    if config in {"typst", "product"}:
        required |= {"noon-typst", "typst-library", "typst-layout"}
    if config in {"native-bundled", "product"}:
        required.add("typst-assets")
    forbidden = set()
    if config in {"minimal", "typst"}:
        forbidden.add("noon-text-native")
    if config == "minimal":
        forbidden.add("swash")
    if config not in {"typst", "product"}:
        forbidden |= {name for name in names if name == "noon-typst" or name == "typst" or name.startswith("typst-")}
        if config == "native-bundled":
            forbidden.discard("typst-assets")
    if config in {"minimal", "native-text"}:
        forbidden.add("typst-assets")
    # Typst requires the base typst-assets package (ICC/ICU/HTML/PDF resources).
    # Its optional typography font bundle is controlled separately by `fonts`.
    bundled = "fonts" in features.get("typst-assets", set())
    if bundled != (config in {"native-bundled", "product"}):
        raise ValueError(f"{config}: unexpected typst-assets/fonts enabled={bundled}")
    missing, unexpected = required - names, forbidden & names
    if missing or unexpected:
        raise ValueError(f"{config}: missing={sorted(missing)}, forbidden={sorted(unexpected)}")
    return names


def build_env(output: Path, *, measure: bool, base_env: dict[str, str] | None = None) -> dict[str, str]:
    """Keep the ordinary cached development path separate from measurements."""
    env = dict(os.environ if base_env is None else base_env)
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target/provider-consumer"))
    env.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    env.setdefault("CARGO_PROFILE_TEST_DEBUG", "0")
    if measure:
        env.pop("RUSTC_WRAPPER", None)
        env.pop("RUSTC_WORKSPACE_WRAPPER", None)
        env.update(CARGO_TARGET_DIR=str(output / "target"), CARGO_INCREMENTAL="0",
                   CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
    return env


def run(command: list[str], *, env: dict[str, str], output: Path | None = None) -> float:
    print("+", " ".join(command), flush=True)
    start = time.perf_counter()
    if output is None:
        subprocess.run(command, cwd=ROOT, env=env, check=True)
    else:
        with output.open("w") as stream:
            subprocess.run(command, cwd=ROOT, env=env, stdout=stream, check=True, text=True)
    return time.perf_counter() - start


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", choices=CONFIGS, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--measure", action="store_true", help="Explicit cold/warm measurement with a fresh target and no compiler wrapper")
    parser.add_argument("--baseline", type=Path, help="With --measure, compare geometry against an unmodified checkout (no provider assertions/tests)")
    args = parser.parse_args()
    if args.baseline and (not args.measure or args.config != "minimal"):
        parser.error("--baseline requires --measure and --config minimal")
    output = args.output.resolve()
    # Do not erase user data or accidentally call a cached build cold.
    output.mkdir(parents=True, exist_ok=False)
    manifest = ROOT / "fixtures/provider-consumer/Cargo.toml"
    if args.baseline:
        baseline = args.baseline.resolve()
        if not (baseline / "crates/noon/Cargo.toml").is_file():
            parser.error("baseline must be a Noon checkout")
        fixture = output / "baseline-consumer"
        (fixture / "src").mkdir(parents=True)
        # Only the unchanged public geometry program is used in the baseline.
        shutil.copyfile(ROOT / "fixtures/provider-consumer/src/main.rs", fixture / "src/main.rs")
        noon_path = json.dumps(str(baseline / "crates/noon"))
        (fixture / "Cargo.toml").write_text(
            '[package]\nname="noon-provider-consumer"\nversion="0.0.0"\nedition="2021"\n'
            '[workspace]\n[dependencies]\nnoon={path=' + noon_path + ',default-features=false}\n'
            '[profile.dev]\ndebug=0\n'
        )
        manifest = fixture / "Cargo.toml"
    env = build_env(output, measure=args.measure)
    common = ["--manifest-path", str(manifest), "--target", args.target, "--no-default-features"]
    if CONFIGS[args.config]:
        common += ["--features", CONFIGS[args.config]]
    run(["cargo", "fetch", "--manifest-path", str(manifest), "--target", args.target], env=env)
    common += ["--locked"]
    run(["cargo", "tree", *common, "-e", "normal,build", "--prefix", "none", "--format", "{p} features={f}"], env=env, output=output / "packages.txt")
    run(["cargo", "tree", *common, "-e", "normal,build,features"], env=env, output=output / "features.txt")
    run(["rustc", "-vV"], env=env, output=output / "rustc.txt")
    shutil.copyfile(manifest.parent / "Cargo.lock", output / "Cargo.lock")
    graph = (output / "packages.txt").read_text()
    if not args.baseline:
        packages = check_graph(args.config, graph)
    else:
        packages = {line.split()[0] for line in graph.splitlines() if line.strip()}
    command = ["cargo", "build", *common, "--bin", "noon-provider-consumer"]
    elapsed = run(command, env=env)
    warm = run(command, env=env) if args.measure else None
    suffix = ".wasm" if args.target.startswith("wasm32") else (".exe" if "windows" in args.target else "")
    target_dir = Path(env["CARGO_TARGET_DIR"])
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    binary = target_dir / args.target / "debug" / ("noon-provider-consumer" + suffix)
    data = binary.read_bytes()
    metrics = {
        "config": args.config, "baseline": bool(args.baseline), "target": args.target,
        "host": platform.platform(), "cpu_count": os.cpu_count(),
        "mode": "measurement" if args.measure else "correctness",
        "binary_bytes": len(data), "gzip_bytes": len(gzip.compress(data, mtime=0)),
        "active_packages": len(packages),
        "profile": "dev, debug=0, incremental=0, no compiler wrapper" if args.measure else "dev, caller compiler cache and target preserved",
        "command": command,
    }
    if args.measure:
        metrics.update(cold_seconds=round(elapsed, 3), warm_seconds=round(warm, 3))
    else:
        metrics["build_seconds"] = round(elapsed, 3)
    report = "measurements.json" if args.measure else "qualification.json"
    (output / report).write_text(json.dumps(metrics, indent=2) + "\n")
    print(json.dumps(metrics, indent=2), flush=True)
    if not args.target.startswith("wasm32"):
        run([str(binary)], env=env)
    if not args.baseline:
        test_command = ["cargo", "test", *common]
        if args.target.startswith("wasm32"):
            test_command.append("--no-run")
        run(test_command, env=env)
        run(["cargo", "clippy", *common, "--all-targets", "--", "-D", "warnings"], env=env)
        # Ordinary facade targets/examples must also compile independently.
        facade = ["--manifest-path", str(ROOT / "Cargo.toml"), "-p", "noon", "--target", args.target, "--no-default-features"]
        features = "default" if args.config == "product" else CONFIGS[args.config]
        if features:
            facade += ["--features", features]
        run(["cargo", "check", *facade, "--all-targets"], env=env)
    (output / "qualification-passed.txt").write_text("All requested checks completed successfully.\n")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"provider qualification failed: {error}", file=sys.stderr)
        sys.exit(1)
