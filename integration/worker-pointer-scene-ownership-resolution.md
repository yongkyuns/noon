# #2296 worker pointer/SceneExecution ownership integration candidate

Source-pinned, **uncompiled** two-file conflict-resolution patch. This workbench is not intended to merge.

- Product PR: [#2296](https://github.com/yongkyuns/noon/pull/2296), exact head `5cc7e16e2df651e62fe66e71b1b607e4e418f90e`.
- Master inspected: `f71a4c121bc29921133f790ae9783e89e962e7d9`; master browser adapter blob `646215e2e001ad46c2f9a155e0bc9ee773242e74`.
- PR browser adapter blob: `16e82745d00d2fad6f506f444e6854d639583991`.
- Existing [worker pointer patch](./worker-pointer-input-resolution.patch) is included unchanged in this new combined patch.
- The combined [patch](./worker-pointer-scene-ownership-resolution.patch) applies to **the #2296 source**, not to master directly. It is an integration input, **not** a verified merge.

## Resolution

Keep master's `BrowserPointerTarget for noon::SceneExecution` and its canonical Scene/session ownership. Layer #2296's `capture_pointer_frame`, `validate_pointer_position`, and `pointer_frame_token` methods onto the Scene wrapper, using the same live `session()` for all three. Retain #2296's position-aware presented-frame admission, the LiveProgram continuation boundary, and all `PickingChanged` recovery handling. The accompanying existing patch requires the canonical semantic-store reference in the worker target, retires the storeless input fallback, and retains playback-control publication only from the successful paired transaction.

Static generation checks: exact Git blob identities, unique implementation anchors, PR side replacement, preservation of frame-validation / token admission, and unchanged original worker-pointer patch. **No Rust formatting, compilation, WASM build, browser tests, or complete 3-way merge are claimed.** The remaining worker, direct, native, tests, JS and other conflicting paths need reconciliation against latest master before any #2296 update.

Do not apply only this bundle and claim merge readiness. Keep the product PR draft, and validate a complete integrated tree (including original audio fixture) separately.
