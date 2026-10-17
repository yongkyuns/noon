#!/usr/bin/env python3
"""Stage two already built exact web trees and apply identical timing probes.

Fail closed on every runtime/probe hash and on any inter-arm site difference
outside the selected WASM and runtime identity. Never changes source checkouts.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def stage(root, inputs, output):
    identities = json.loads((root / "site-identities.json").read_text())
    require(not output.exists() or not any(output.iterdir()), "site output must be empty")
    for arm, source in inputs.items():
        target = output / arm
        require(not list((source / "pkg").glob("*.wasm-opt.wasm")), "optimizer temporary remains in input site")
        shutil.copytree(source, target)
        runtime = json.loads((target / "runtime-build-identity.json").read_text())
        require(runtime == identities["arms"][arm]["runtime"], "runtime identity differs from captured build")
        for entry in runtime["files"].values():
            require(sha(target / entry["path"]) == entry["sha256"], "runtime hash mismatch")
        shutil.copyfile(root / "harness/profile-probe.js", target / "profile-probe.js")
        for name in ["authoring-render-controller.js", "semantic-engine-endpoint.js"]:
            path = target / name
            original = path.read_text()
            require("./profile-probe.js" not in original, "input already instrumented")
            text = 'import "./profile-probe.js";\n' + original
            if name == "semantic-engine-endpoint.js":
                needle = '    renderPort.addEventListener("message", ({ data: message }) => {\n'
                require(text.count(needle) == 1, "engine message insertion point is not unique")
                text = text.replace(needle, needle + '      globalThis.__noonProfileMark("engine_receive", {kind:message?.type,sequence:message?.sequence,cadence:message?.cadence});\n')
            path.write_text(text)
        for name, digest in identities["arms"][arm]["instrumented_host_sha256"].items():
            require(sha(target / name) == digest, "probe/host differs from captured version: " + name)
    hashes = {arm: {str(path.relative_to(output / arm)): sha(path) for path in (output / arm).rglob("*") if path.is_file()} for arm in inputs}
    require(hashes["new"].keys() == hashes["old"].keys(), "site file sets differ")
    require(len(hashes["new"]) == identities["timed_site_file_count"], "unexpected site file count")
    changed = sorted(path for path in hashes["new"] if hashes["new"][path] != hashes["old"][path])
    require(changed == ["pkg/noon_web_bg.wasm", "runtime-build-identity.json"], "unexpected inter-arm site differences")
    print(json.dumps({"status": "passed", "files_per_arm": len(hashes["new"]), "differences": changed, "sites": str(output)}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--new-web", type=Path, required=True)
    parser.add_argument("--old-web", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    stage(args.package.resolve(), {"new": args.new_web.resolve(), "old": args.old_web.resolve()}, args.output.resolve())
