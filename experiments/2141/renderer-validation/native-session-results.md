# Focused native checks: session report

**Evidence type: narrative from the working session, not preserved stdout/stderr.** No separate raw logs or image files were retained for the focused native executions listed below. This file does not reconstruct those logs. The raw fast-gate logs elsewhere in the bundle do independently contain the 298 passing renderer library tests, including the family locality regressions; their ignored raster/oracle entries do not prove those ignored tests ran in that gate.

The focused native runs used the renderer changes later committed as local `94fb0297959b457e8a7b7a40e86858a726650ff8`, equivalent to remote `522e0982258d55d3515da6aad5c94331e263a2d9`. Those changes are present in browser revision `495580e`. The renderer subtree is also unchanged in final merged revision `aba3d4c6`. No exact candidate SHA was embedded in a retained focused-native log.

## Reported outcomes

- Renderer library suite: 298 passed. The existing mixed-family upload assertion remained 688 bytes; it was not loosened. The added family exclusion test covers compaction, unrelated packed paths, and releasing the exclusion when ordinary rendering resumes.
- `retained_animation_geometry_parity`: 3 integration tests passed.
- `transient_anchor_order`: the explicitly enabled raster test passed all three internal fixtures: analytic sources, one packed run, and reordered/chunked packed ranges. Its overlapping adjacent `After(A)`/`Before(B)` occurrences retained their intended order.
- `mixed_mega_native_exact_pixels_against_individual_paths`: the explicitly enabled oracle passed exact RGBA and 4x MSAA sample comparisons across four states. Its small mixed fixture reported 97 individual geometry draws versus 4 packed draws at each state. These numbers belong to that test fixture, not the separate 10,000-ray benchmark.

The reported native adapter was Mesa llvmpipe (LLVM 20.1.2, 256 bits), Vulkan, Mesa 25.2.8. The session used Rust 1.98.1, `CARGO_BUILD_JOBS=2`, `XDG_RUNTIME_DIR=/tmp/noon-xdg`, and `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`.

## Commands used

The session supplied `PATH=/tmp/noon-cargo/bin:$PATH`, `RUSTUP_HOME=/tmp/noon-rustup`, `CARGO_HOME=/tmp/noon-cargo`, and `CARGO_TARGET_DIR=/tmp/noon-target` in addition to the environment above.

```bash
cargo test -p noon-render-wgpu --lib
cargo test -p noon-render-wgpu --test retained_animation_geometry_parity
cargo test -p noon-render-wgpu --test transient_anchor_order -- --ignored --nocapture
cargo test -p noon-render-wgpu --lib mixed_mega_native_exact_pixels_against_individual_paths -- --ignored --nocapture
```

These commands are provided for reproduction. They were not rerun while assembling this evidence bundle.
