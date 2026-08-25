# Fuzzing

Noon uses two complementary fuzz layers.

1. **Deterministic regression seeds** live in ordinary Rust tests and run on every PR. These cover malformed/future scene documents, rejected patch transactions, and representative degenerate geometry/morph cases.
2. **libFuzzer exploration** lives under `fuzz/` and runs on a weekly schedule or via `workflow_dispatch`. It is intentionally time-bounded so normal correctness CI remains fast and deterministic.

## Targets

| Target | Boundary | Primary invariants |
| --- | --- | --- |
| `scene_decode` | Scene and semantic-scene JSON | No panic/hang for bounded UTF-8; unsupported/invalid data returns errors |
| `patch_transaction` | Patch batch decode + transactional application | Rejected transaction leaves retained scene unchanged |
| `tessellation` | Vector path construction/tessellation | Finite vertices, valid indices, bounded output for bounded generated input |
| `morph` | Path morph planning/interpolation | Clean rejection or finite bounded frames across endpoint/out-of-range progress |

`fuzz/corpus/<target>/` contains checked-in starter inputs. New minimized crashing inputs should be copied from the workflow artifact into the appropriate corpus and, when practical, represented by an ordinary regression test as well.

The existing deterministic geometry stress work remains authoritative for broad generated geometry properties; these libFuzzer targets focus on adversarial byte-level state-space exploration rather than duplicating those tests.

## Running locally

```sh
cargo install cargo-fuzz --locked
cd fuzz
cargo fuzz run scene_decode -- -max_total_time=60
cargo fuzz run patch_transaction -- -max_total_time=60
cargo fuzz run tessellation -- -max_total_time=60
cargo fuzz run morph -- -max_total_time=60
```

Keep inputs bounded and deterministic outside libFuzzer. A fuzz target should not use wall-clock time, network access, threads, or GPU resources.
