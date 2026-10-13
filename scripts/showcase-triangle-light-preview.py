#!/usr/bin/env python3
"""Package the existing browser product for unmerged review; no new renderer.

Temporary #2056 tooling. Delete with the candidate gate after visual approval.
The output is a separate directory; the checked-in public gallery is untouched.
"""
import argparse
import hashlib
import html
import json
from pathlib import Path
import re
import shutil

PATHS = {"worker": "./python-worker.js", "wasm": "./pkg/noon_web_bg.wasm",
         "glue": "./pkg/noon_web.js", "verifier": "./runtime-build-verifier.js"}


def verify_runtime(web):
    identity = json.loads((web / "runtime-build-identity.json").read_text())
    if identity.get("schema") != 1 or set(identity.get("files", {})) != set(PATHS):
        raise ValueError("Missing or malformed built runtime; run scripts/build-web-demo.sh first")
    for name, expected in PATHS.items():
        entry = identity["files"][name]
        if entry.get("path") != expected:
            raise ValueError("Unexpected runtime file path")
        actual = hashlib.sha256((web / expected).read_bytes()).hexdigest()
        if actual != entry.get("sha256"):
            raise ValueError(f"Runtime hash mismatch: {name}")
    core = {"schema": 1, "sourceRevision": identity["sourceRevision"], "files": identity["files"]}
    digest = hashlib.sha256(json.dumps(core, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
    if digest != identity.get("buildId"):
        raise ValueError("Runtime build identity digest mismatch")
    return identity


SERVER = '''#!/usr/bin/env python3
"""Serve the review on localhost; no compilation or package install required."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import webbrowser

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8080)
    parser.add_argument("--no-open", action="store_true")
    args = parser.parse_args()
    if not 0 <= args.port <= 65535:
        parser.error("port must be between 0 and 65535")
    handler = partial(SimpleHTTPRequestHandler, directory=str(Path(__file__).resolve().parent))
    with ThreadingHTTPServer(("127.0.0.1", args.port), handler) as server:
        url = f"http://127.0.0.1:{server.server_port}/"
        print(f"Unmerged Noon triangle-light review: {url}", flush=True)
        print("Open the preview, then click Run. Ctrl-C stops the server.", flush=True)
        if not args.no_open:
            webbrowser.open(url)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass
'''


def package_preview(root, web, output, source_revision):
    if not re.fullmatch(r"[0-9a-f]{40}", source_revision):
        raise ValueError("Source revision must be an exact 40-character commit SHA")
    if output.exists():
        raise FileExistsError("Review output already exists; choose a fresh output directory")
    identity = verify_runtime(web)
    entry = json.loads((root / "web/python/examples/showcase_triangle_light.candidate.json").read_text())
    source = (root / "web" / entry["path"]).read_bytes()
    info = {"reviewStatus": "unapproved; rendering qualification is recorded separately",
            "sourceRevision": source_revision, "sourceSha256": hashlib.sha256(source).hexdigest(),
            "runtimeBuildIdentity": identity}
    output.mkdir(parents=True)
    try:
        shutil.copytree(web, output / "web")
        copied_web = output / "web"
        (copied_web / entry["path"]).write_bytes(source)
        manifest = {"version": 1, "publication": "curated", "entries": [entry]}
        # The product schema is reused inside this explicitly unapproved local
        # review bundle; this is not a commit or deployment of the public catalog.
        (copied_web / "python/examples/noon_showcase_manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        # Do not accidentally present older public posters as candidate evidence.
        (copied_web / "thumbnails/showcase/capture-evidence.json").unlink(missing_ok=True)
        (output / "review-build-info.json").write_text(json.dumps(info, indent=2) + "\n")
        (output / "serve.py").write_text(SERVER)
        revision = html.escape(source_revision)
        runtime_revision = html.escape(str(identity["sourceRevision"]))
        (output / "index.html").write_text(f'''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Noon PR 2057 — unmerged visual review</title>
<style>body{{max-width:52rem;margin:3rem auto;padding:0 1.3rem;font:17px/1.6 system-ui;background:#121624;color:#edf1fa}}a{{color:#9ad8ff}}code{{overflow-wrap:anywhere}}.button{{display:inline-block;padding:.8rem 1.2rem;background:#b9ddff;color:#132035;border-radius:.6rem;font-weight:bold;text-decoration:none}}small{{display:block;color:#bec8dc}}</style>
<h1>Triangle light laboratory</h1><p><strong>Unmerged review of PR #2057.</strong> This is the actual Noon browser product with the candidate scene selected, not an illustration or a substitute animation engine.</p>
<p><a class="button" href="web/index.html?example=showcase-triangle-light">Open the preview</a></p>
<p>Click <strong>Run</strong> in the playground. After the 25-second sequence completes, use its playback controls to replay or seek. Review the pink beam, the colored paths at 15.6 seconds, and the dense finale at 24 seconds.</p>
<p>The first Run downloads the pinned Python-in-the-browser runtime and may take time to initialize. An uncaptured gallery thumbnail can be blank; inspect the animation itself. Any displayed runtime error or unavailable replay is a review failure, not an approval.</p>
<p>Chrome or Edge on a desktop is the first review target. Mobile and other browsers still need their own validation. This package does not deploy anything or modify the public Noon gallery.</p>
<small>Candidate source: <code>{revision}</code><br>Built engine: <code>{runtime_revision}</code></small>
<p><a href="web/{entry['path']}">Editable Python source</a> · <a href="review-build-info.json">Exact source and runtime hashes</a></p></html>''')
        (output / "README.txt").write_text(
            "NOON TRIANGLE LIGHT — UNMERGED REVIEW\n\n"
            "1. Unzip this folder.\n2. In this folder run: python3 serve.py\n"
            "3. Open the displayed localhost address, open the preview, and click Run.\n\n"
            "No Rust build, Node installation, or merge is required. Python 3 and an\n"
            "internet-connected browser are required. Double-clicking HTML directly\n"
            "will not work because browsers restrict file:// workers and WASM fetches.\n\n"
            "Stop with Ctrl-C. The server binds only to 127.0.0.1. For a different\n"
            "port use: python3 serve.py --port 8081\n\n"
            f"Scene revision: {source_revision}\nEngine revision: {identity['sourceRevision']}\n"
            "The recorded engine and scene revisions may differ when an unchanged\n"
            "prebuilt engine is reused. Hashes are in review-build-info.json.\n\n"
            "This package is NOT evidence of a rendering, replay, or performance pass.\n"
            "Those results and actual frame/video evidence are separate CI artifacts.\n"
            "The PR must remain unmerged until its output is approved.\n")
    except BaseException:
        shutil.rmtree(output)
        raise
    return info


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--web", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--source-revision", required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    package_preview(root, args.web or root / "web", args.output.resolve(), args.source_revision)
    print(f"Unapproved runnable review package: {args.output}")
