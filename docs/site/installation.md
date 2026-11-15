# Choose your setup

Use the browser to explore immediately. Use a repository checkout when you need
a native window, Rust authoring, or native export. Choose a host before installing.

| Your goal | Start with | What you need |
| --- | --- | --- |
| Try Python and edit a live scene | [Browser tutorial](getting-started.html) | A supported browser and network access for the first load |
| Run Python in a native window | [Native Python](#native-python) | CPython 3.11+, the pinned Rust toolchain, native build dependencies, and a graphics driver |
| Author in Rust | [Native Rust](#native-rust) | The pinned Rust toolchain, native build dependencies, and a graphics driver |
| Develop the browser host | [Local web development](#local-web-development) | The repository's WASM build tools and a local HTTP server |

> **Package names are not final.** Do not assume that an unrelated `noon` package
> on a public registry is this project. These instructions use the repository.
> [Release-candidate details](reference.html#initial-release-candidates) describe
> supported candidate formats and platform limits.

## Get a source checkout

The commands below use a POSIX shell. Platform-specific build dependencies and
Windows shell instructions are documented in the linked host references.

```sh
git clone https://github.com/yongkyuns/noon.git
cd noon
```

The repository's `rust-toolchain.toml` selects the Rust toolchain. Run commands
from the checkout root so that paths and toolchain selection resolve correctly.

## Native Python

Build the extension and run the checked-in lesson:

```sh
python3 scripts/build-native-python.py --profile dev
PYTHONPATH=build/python python3 -m noon_native web/python/examples/first_lesson.py
```

The expected result is a native viewer, not a browser tab. This path uses CPython
with the Rust engine and does not need Pyodide. See [native Python details](native-python.html)
for interpreter/platform requirements, wheel candidates, headless modes and
unsupported combinations. Then continue with the [Python walkthrough](python.html).

## Native Rust

Run the same paired lesson through the Rust host:

```sh
cargo run -p noon-native --example first_lesson
```

The expected result is a native window with the lesson. Python is not required.
The [Rust walkthrough](rust.html) explains its complete source and synchronous
control flow. [Rust library details](native-rust.html) covers application setup.

## Local web development

Build the existing browser package, then serve it over HTTP:

```sh
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Open `http://localhost:8080`. This is the development playground. The project
website and documentation are staged separately. Follow [browser integration](web-runtime.html)
for the repository's tool prerequisites and direct Rust/WASM examples.

Rust source compiles to WASM; changes require a rebuild. The Python editor does
not interpret Rust. A browser target also does not make Python a mandatory part
of the Rust engine.

## Check before adding complexity

Run the unchanged first lesson before adding effects, assets, callbacks or custom
source. A loaded editor alone does not establish successful rendering. Confirm
that you can see the expected animation, then make a small edit.

If that fails, keep the diagnostic and source revision and use
[troubleshooting](troubleshooting.html). For video output, read
[exporting visuals](export.html) before installing additional tools.
