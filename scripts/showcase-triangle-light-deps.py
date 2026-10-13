#!/usr/bin/env python3
"""Archive the pinned Pyodide inputs atomically for #2056 review, not deployment."""
import json
from pathlib import Path
import re
import shutil
import tempfile
import urllib.request


def collect_resources(root, *, fetch=None):
    root = Path(root)
    worker = (root / "web/python-worker.js").read_text()
    match = re.search(r'from "(https://cdn\.jsdelivr\.net/pyodide/v[0-9]+\.[0-9]+\.[0-9]+/full/)pyodide\.mjs"', worker)
    if not match:
        raise ValueError("Cannot identify the pinned Pyodide distribution")
    base = match[1]
    output = root / "browser-smoke-artifacts/triangle-light-browser-deps"
    if output.exists():
        raise FileExistsError(f"Refusing to overwrite previous runtime inputs: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)

    if fetch is None:
        def fetch(url):
            print(f"Downloading {url}", flush=True)
            with urllib.request.urlopen(url, timeout=120) as response:
                return response.read()

    staging = Path(tempfile.mkdtemp(prefix=".triangle-deps-", dir=output.parent))
    try:
        loader = fetch(base + "pyodide.mjs")
        # The pinned v314 loader imports an ES module. Older distributions used
        # .js; identify the filename from the actual loader, never guess the suffix.
        modules = set(re.findall(r'\bpyodide\.asm\.m?js\b', loader.decode("utf-8")))
        if len(modules) != 1:
            raise ValueError("Pinned Pyodide loader has no unique assembly module")
        names = ("pyodide.mjs", modules.pop(), "pyodide.asm.wasm", "python_stdlib.zip", "pyodide-lock.json")
        urls = {}
        for name in names:
            data = loader if name == "pyodide.mjs" else fetch(base + name)
            if not data:
                raise ValueError(f"Empty Pyodide resource: {name}")
            (staging / name).write_bytes(data)
            urls[base + name] = name
        (staging / "url-map.json").write_text(json.dumps(urls, indent=2) + "\n")
        staging.rename(output)
    finally:
        shutil.rmtree(staging, ignore_errors=True)
    return output


if __name__ == "__main__":
    collect_resources(Path(__file__).resolve().parents[1])
