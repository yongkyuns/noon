# Reading capability evidence

Run `python3 -B scripts/noon-capabilities.py` from a trusted Noon checkout to obtain
the complete JSON report. Repeat `--symbol NAME` for focused API discovery. Repeat
`--example ID` to restrict the example records; explicit example filters are
independent of symbol filters. Without an explicit example filter, a symbol query
includes its ready tutorial references. Unknown names fail rather than returning
an empty result that could be mistaken for a supported API.

Schema version 1 reports `scope: source-inventory`. It reads the existing
`compat/manim-v0.21.0.json`, the tutorial manifest, and the coverage script's static
export discovery. There is no separately maintained agent support matrix.

`symbols.NAME.policy` preserves the explicit compatibility record, including
restrictions and declared evidence. `classification_source: unclassified-export`
means the name was statically discovered without a per-symbol policy entry; it is
not promoted to supported. Upstream module rules cannot be resolved without Manim
introspection and are explicitly not applied in this offline inventory.

A supported class does not prove every method, option, family combination, callback,
or backend. Read the policy reason and the actual example. Ready example references
come from feature tags, not a proof that all possible calls using the symbol work.

Examples retain `status`, `parity_status`, `qualification_mode`, upstream/reuse
metadata, and a repository-relative path plus source hash for ready fixtures.
`ready` is availability in the maintained corpus. `candidate` and `parity-qualified`
are different declared qualification levels; neither is a fresh test run by this
command. Blocked/deferred entries remain visible with their owning issues, but do
not become ready evidence for a symbol.

`provenance` includes Git revision/dirty state when available and SHA-256 hashes of
report inputs, adapters, and referenced ready/upstream sources. A source archive
has null Git metadata instead of borrowing an unrelated parent checkout's revision.
These values identify source, not a deployed binary. In schema 1, no runtime host
is started: renderer backend and session capabilities are null, and
`behavioral_tests_run` / `runtime_verified` are false.

Every query validates the whole inventory before applying filters. Fix malformed
metadata or missing evidence in its owning inventory; do not suppress errors or
modify the agent output to make an unsupported feature appear supported.
