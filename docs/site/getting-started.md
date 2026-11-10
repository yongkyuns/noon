# Getting started

Choose a host before installing anything. The browser is the easiest place to
try Python; native Rust or CPython is useful for a normal application or an
export workflow. Registry names are not finalized: do not install an unrelated
package named `noon` from a public registry.

## Browser

Open an example below, then press **Run** in the preview. The preview executes
the same Rust/WASM and Pyodide path as the full playground. **Open full editor**
lets you change the source. The first run downloads runtime resources, so it
requires network access and can take longer than subsequent runs.

<!-- noon:demo showcase-first-scene -->

Preview activation is explicit. Reading a guide does not start WASM or Python.
Only one documentation preview is open at a time; closing it, opening another,
or hiding the tab retires its embedded browsing context. Reopening starts fresh.

## Native Python

For development from a checkout, use CPython 3.11+ and the pinned Rust toolchain:

```sh
git clone https://github.com/yongkyuns/noon.git
cd noon
python3 scripts/build-native-python.py --profile dev
PYTHONPATH=build/python python3 -m noon_native web/python/examples/first_lesson.py
```

These commands use a POSIX shell. Source builds also need the platform's native C
and graphics build dependencies. A native viewer requires a compatible graphics
driver. For wheel installation, Windows shell instructions, host limitations and
headless modes, read [native Python details](native-python.html).

Continue with the [Python walkthrough](python.html).

## Native Rust

From the same checkout, run the ordinary synchronous lesson:

```sh
cargo run -p noon-native --example first_lesson
```

Rust uses the shared engine directly; this command does not need Python, Pyodide,
or a browser. The repository toolchain and example dependencies are selected by
Cargo. The [Rust walkthrough](rust.html) explains the lesson and its native host.

## Browser Rust and local web development

Build the repository's existing browser package, then serve its web directory:

```sh
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Open `http://localhost:8080`. This is the development playground; the published
project website is staged separately. Rust source is compiled into WASM. Editing
Rust code requires rebuilding; the Python editor does not interpret Rust.
[Browser integration](web-runtime.html) describes direct Rust/WASM examples and
the distinct optional Pyodide worker path.

## Check your first run

A successful run shows actual rendered output, not just a loaded editor. Inspect
the status and diagnostics when a scene fails. Start with the unchanged first
lesson before adding effects, assets, callbacks or external dependencies.
[Troubleshooting](troubleshooting.html) covers the common boundaries.
