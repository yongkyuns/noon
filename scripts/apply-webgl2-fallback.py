from pathlib import Path


def replace_once(path, old, new):
    path = Path(path)
    text = path.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:80]!r}")
    path.write_text(text.replace(old, new, 1))


replace_once(
    "crates/noon-render-wgpu/Cargo.toml",
    'web = ["wgpu/webgpu"]',
    'web = ["wgpu/webgpu", "wgpu/webgl"]',
)
replace_once(
    "crates/noon-web/Cargo.toml",
    'features = ["std", "wgsl", "webgpu"]',
    'features = ["std", "wgsl", "webgpu", "webgl"]',
)

lib = "crates/noon-web/src/lib.rs"
replace_once(
    lib,
    '    /// Persistent browser player that connects the deterministic runtime to a WebGPU canvas.\n',
    '    /// Persistent browser player that connects the deterministic runtime to a GPU canvas.\n',
)
replace_once(
    lib,
    '        queue: wgpu::Queue,\n        canvas: HtmlCanvasElement,',
    '        queue: wgpu::Queue,\n        backend: wgpu::Backend,\n        canvas: HtmlCanvasElement,',
)
replace_once(
    lib,
    '            instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU;\n            let instance = wgpu::Instance::new(instance_descriptor);',
    '            instance_descriptor.backends =\n                wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;\n            let instance =\n                wgpu::util::new_instance_with_webgpu_detection(instance_descriptor).await;',
)
replace_once(
    lib,
    '                .await\n                .map_err(js_error)?;\n            let timestamp_queries_supported =\n                adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);',
    '                .await\n                .map_err(js_error)?;\n            let backend = adapter.get_info().backend;\n            let timestamp_queries_supported =\n                adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);',
)
replace_once(
    lib,
    '            let (device, queue) = adapter\n                .request_device(&wgpu::DeviceDescriptor {\n                    label: Some("Noon WebGPU device"),\n                    required_features,\n                    ..Default::default()\n                })',
    '            let required_limits = if backend == wgpu::Backend::Gl {\n                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())\n            } else {\n                wgpu::Limits::default()\n            };\n            let (device, queue) = adapter\n                .request_device(&wgpu::DeviceDescriptor {\n                    label: Some("Noon browser GPU device"),\n                    required_features,\n                    required_limits,\n                    ..Default::default()\n                })',
)
replace_once(
    lib,
    '                .ok_or_else(|| js_message("WebGPU adapter cannot present to this canvas"))?;',
    '                .ok_or_else(|| js_message("GPU adapter cannot present to this canvas"))?;',
)
replace_once(
    lib,
    '                queue,\n                canvas,',
    '                queue,\n                backend,\n                canvas,',
)
replace_once(
    lib,
    '        #[wasm_bindgen(js_name = nextSequence)]\n        pub fn next_sequence(&self) -> u64 {\n            self.player.next_sequence()\n        }\n\n        #[wasm_bindgen(js_name = lastDrawCalls)]',
    '        #[wasm_bindgen(js_name = nextSequence)]\n        pub fn next_sequence(&self) -> u64 {\n            self.player.next_sequence()\n        }\n\n        #[wasm_bindgen(js_name = rendererBackend)]\n        pub fn renderer_backend(&self) -> String {\n            match self.backend {\n                wgpu::Backend::BrowserWebGpu => "WebGPU".to_owned(),\n                wgpu::Backend::Gl => "WebGL2".to_owned(),\n                other => format!("{other:?}"),\n            }\n        }\n\n        #[wasm_bindgen(js_name = lastDrawCalls)]',
)
replace_once(
    lib,
    '                        return Err(js_message("WebGPU rejected the canvas surface texture"));',
    '                        return Err(js_message("GPU backend rejected the canvas surface texture"));',
)

main = "web/main.js"
replace_once(
    main,
    'try {\n  if (!navigator.gpu) {\n    throw new Error("This browser does not expose WebGPU");\n  }\n\n  await init();',
    'try {\n  await init();',
)
replace_once(
    main,
    '  const player = await NoonCanvasPlayer.create(canvas, demoSceneJson(), 4.0);\n  const gpuProfilingSupported = player.gpuProfilingSupported();',
    '  const player = await NoonCanvasPlayer.create(canvas, demoSceneJson(), 4.0);\n  const rendererBackend = player.rendererBackend();\n  status.dataset.rendererBackend = rendererBackend;\n  const gpuProfilingSupported = player.gpuProfilingSupported();',
)
replace_once(
    main,
    '        setRuntimeStatus(`${objectCount} objects · WebGPU live`, "running");',
    '        setRuntimeStatus(`${objectCount} objects · ${rendererBackend} live`, "running");',
)

smoke_js = "web/browser-smoke.js"
replace_once(
    smoke_js,
    '    geometryCacheMisses: player?.lastGeometryCacheMisses() ?? 0,\n',
    '    geometryCacheMisses: player?.lastGeometryCacheMisses() ?? 0,\n    rendererBackend: player?.rendererBackend() ?? null,\n',
)
replace_once(
    smoke_js,
    'async function start() {\n  if (!navigator.gpu) {\n    throw new Error("This browser does not expose WebGPU");\n  }\n\n  await init();',
    'async function start() {\n  await init();',
)

index = "web/index.html"
replace_once(
    index,
    'content="Noon Playground: author mathematical animation in Python and run it in a persistent Rust/WebGPU runtime."',
    'content="Noon Playground: author mathematical animation in Python and run it in a persistent Rust/WASM GPU runtime with WebGPU and WebGL2 fallback."',
)
replace_once(index, '<title>Noon Playground · Python + WebGPU</title>', '<title>Noon Playground · Python + GPU</title>')
replace_once(
    index,
    '<p class="subtitle">Write Python. Keep the animation loop in Rust + WebGPU.</p>',
    '<p class="subtitle">Write Python. Keep the animation loop in Rust + GPU.</p>',
)
replace_once(index, '<span id="status-text">Starting WebGPU…</span>', '<span id="status-text">Starting GPU renderer…</span>')
replace_once(index, '<section class="preview-pane" aria-label="WebGPU preview">', '<section class="preview-pane" aria-label="GPU preview">')
replace_once(index, '<span class="badge">WebGPU</span>', '<span class="badge">WebGPU / WebGL2</span>')

smoke = "scripts/browser-smoke.mjs"
replace_once(
    smoke,
    'const baseUrl = `http://127.0.0.1:${port}`;\n\nawait mkdir(artifactDir, { recursive: true });',
    'const baseUrl = `http://127.0.0.1:${port}`;\nconst backendMode = process.env.NOON_BROWSER_SMOKE_BACKEND ?? "webgpu";\nassert.ok(\n  backendMode === "webgpu" || backendMode === "webgl",\n  `unknown browser smoke backend: ${backendMode}`,\n);\nconst expectedRendererBackend = backendMode === "webgpu" ? "WebGPU" : "WebGL2";\n\nawait mkdir(artifactDir, { recursive: true });',
)
replace_once(
    smoke,
    '''  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: [
      "--enable-unsafe-webgpu",
      "--enable-unsafe-swiftshader",
      "--use-webgpu-adapter=swiftshader",
      "--use-gpu-in-tests",
      "--ignore-gpu-blocklist",
      "--enable-features=Vulkan",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--use-vulkan=swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ],
  });''',
    '''  const browserArgs =
    backendMode === "webgpu"
      ? [
          "--enable-unsafe-webgpu",
          "--enable-unsafe-swiftshader",
          "--use-webgpu-adapter=swiftshader",
          "--use-gpu-in-tests",
          "--ignore-gpu-blocklist",
          "--enable-features=Vulkan",
          "--use-gl=angle",
          "--use-angle=swiftshader",
          "--use-vulkan=swiftshader",
          "--disable-gpu-sandbox",
          "--disable-dev-shm-usage",
        ]
      : [
          "--disable-features=WebGPU",
          "--enable-unsafe-swiftshader",
          "--ignore-gpu-blocklist",
          "--use-gl=angle",
          "--use-angle=swiftshader",
          "--disable-gpu-sandbox",
          "--disable-dev-shm-usage",
        ];
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs,
  });''',
)
replace_once(
    smoke,
    '    throw new Error(`WebGPU harness failed to initialize: ${initial.error}`);\n  }\n\n  for (const [index, example] of examples.entries()) {',
    '    throw new Error(`${expectedRendererBackend} harness failed to initialize: ${initial.error}`);\n  }\n  assert.equal(\n    initial.rendererBackend,\n    expectedRendererBackend,\n    `browser smoke selected ${initial.rendererBackend}; expected ${expectedRendererBackend}`,\n  );\n\n  for (const [index, example] of examples.entries()) {',
)
replace_once(
    smoke,
    '  console.log(`Browser WebGPU smoke passed for ${examples.length} picker scenes at four semantic checkpoints each.`);',
    '  console.log(\n    `Browser ${expectedRendererBackend} smoke passed for ${examples.length} picker scenes at four semantic checkpoints each.`,\n  );',
)

final_ci = '''name: CI

on:
  pull_request:
  push:
    branches:
      - master
  workflow_dispatch:

permissions:
  contents: read

concurrency:
  group: ci-${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  ci:
    name: CI
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Use stable Rust
        run: |
          rustup default stable
          rustup component add rustfmt clippy
          rustup target add wasm32-unknown-unknown

      - name: Cache Cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-noon-${{ hashFiles('crates/**/Cargo.toml') }}
          restore-keys: |
            ${{ runner.os }}-noon-

      - name: Install pinned wasm-pack
        uses: taiki-e/install-action@13608cbb45b01feb47ef444ab1a42dc41ad56f1a # v2.79.11
        with:
          tool: wasm-pack@0.15.0
          fallback: none

      - name: Format workspace
        run: cargo fmt --all -- --check

      - name: Compile workspace
        run: cargo check --workspace --all-targets --all-features

      - name: Clippy workspace
        run: cargo clippy --workspace --all-targets --all-features -- -D warnings

      - name: Test geometry correctness invariants
        run: |
          cargo test -p noon-geometry --test tessellation_correctness
          cargo test -p noon-geometry --test stroke_style_parity
          cargo test -p noon-geometry --test filled_morph
          cargo test -p noon-geometry --test filled_morph_interval

      - name: Test workspace
        run: cargo test --workspace --all-features

      - name: Compile browser renderer
        run: cargo check -p noon-render-wgpu --target wasm32-unknown-unknown --no-default-features --features web

      - name: Compile browser runtime
        run: cargo check -p noon-web --target wasm32-unknown-unknown

      - name: Build and validate browser package
        run: bash scripts/build-web-demo.sh

      - name: Cache Playwright Chromium
        uses: actions/cache@v4
        with:
          path: ~/.cache/ms-playwright
          key: ${{ runner.os }}-playwright-chromium-1.62.1

      - name: Install browser smoke dependencies
        run: |
          npm install --no-save --ignore-scripts playwright@1.62.1 pngjs@7.0.0
          npx playwright install --with-deps chromium

      - name: Test browser WebGPU rendering
        env:
          NOON_BROWSER_SMOKE_BACKEND: webgpu
          NOON_BROWSER_SMOKE_ARTIFACTS: browser-smoke-artifacts/webgpu
        run: node scripts/browser-smoke.mjs

      - name: Test browser WebGL2 fallback
        env:
          NOON_BROWSER_SMOKE_BACKEND: webgl
          NOON_BROWSER_SMOKE_ARTIFACTS: browser-smoke-artifacts/webgl
        run: node scripts/browser-smoke.mjs

      - name: Upload browser smoke screenshots
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: browser-smoke-screenshots
          path: browser-smoke-artifacts
          if-no-files-found: ignore
          retention-days: 7
'''
Path(".github/workflows/ci.yml").write_text(final_ci)
Path(".github/workflows/apply-webgl2-fallback.yml").unlink(missing_ok=True)
Path(__file__).unlink()
