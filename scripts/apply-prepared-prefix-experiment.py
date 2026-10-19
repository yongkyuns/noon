#!/usr/bin/env python3
"""One-use checked wiring for the experimental branch; removed after validation.
This changes no default feature and no production viewer. The dedicated CI job
commits only the resulting inspected paths, on this branch, after tests pass.
"""
from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if new in text:
        return
    if text.count(old) != 1:
        raise RuntimeError(f"Unexpected integration anchor in {path}: {old!r}")
    file.write_text(text.replace(old, new, 1))


def append_once(path: str, text: str) -> None:
    file = Path(path)
    current = file.read_text()
    if text.strip() not in current:
        file.write_text(current.rstrip() + "\n\n" + text.rstrip() + "\n")


def feature(path: str, dependencies: str) -> None:
    file = Path(path)
    text = file.read_text()
    line = f'prepared-prefix = [{dependencies}]\n'
    if line in text:
        return
    if '[features]\n' in text:
        replace_once(path, '[features]\n', '[features]\n# Experimental #2211; not enabled by default.\n' + line)
    else:
        append_once(path, '[features]\n# Experimental #2211; not enabled by default.\n' + line)


feature('crates/noon-compile/Cargo.toml', '')
feature('crates/noon-runtime/Cargo.toml', '"noon-compile/prepared-prefix"')
feature('crates/noon/Cargo.toml', '"noon-runtime/prepared-prefix"')
append_once('crates/noon-compile/src/replay_revision.rs', '#[cfg(feature = "prepared-prefix")]\nmod prefix;')
append_once('crates/noon-runtime/src/replay.rs', '''#[cfg(feature = "prepared-prefix")]
mod prefix;
#[cfg(feature = "prepared-prefix")]
pub use prefix::{PrefixBatch, PrefixBatchStats, PrefixError, PrefixPublisher, PrefixReader};''')
replace_once('crates/noon-runtime/src/replay.rs',
    'pub(crate) struct ReplayHistory {\n    start: f64,',
    'pub(crate) struct ReplayHistory {\n    start: f64,\n    #[cfg(feature = "prepared-prefix")]\n    scope: std::sync::Arc<()>,')
replace_once('crates/noon-runtime/src/replay.rs',
    'self.replay_history = Some(ReplayHistory {\n            start: self.frame.time,',
    'self.replay_history = Some(ReplayHistory {\n            start: self.frame.time,\n            #[cfg(feature = "prepared-prefix")]\n            scope: std::sync::Arc::new(()),')
exports = 'PrefixBatch, PrefixBatchStats, PrefixError, PrefixPublisher, PrefixReader'
append_once('crates/noon-runtime/src/lib.rs', '#[cfg(feature = "prepared-prefix")]\npub use replay::{' + exports + '};')
append_once('crates/noon/src/execution_session/replay.rs', '#[cfg(feature = "prepared-prefix")]\nmod prefix;')
append_once('crates/noon/src/lib.rs', '#[cfg(feature = "prepared-prefix")]\npub use noon_runtime::{' + exports + '};')

# Semantic lowering always installs a ReactiveRuntime, even for an empty graph.
# Admission must inspect current VM state, not whether that container exists.
# Read the existing storage length so later signal enrollment is visible without
# a duplicated signal counter or a per-publication scan.
replace_once('crates/noon-core/src/reactive/native_reactive/compute_ir.rs',
    'impl ComputeState {\n    pub fn value(',
    '''impl ComputeState {
    /// Current instantiated signal count, including inputs enrolled after lowering.
    pub fn signal_count(&self) -> usize {
        self.values.len()
    }

    pub fn value(''')
replace_once('crates/noon-runtime/src/reactive/runtime.rs',
    'impl ReactiveRuntime {\n    pub(crate) fn owns_property(',
    '''impl ReactiveRuntime {
    /// Empty semantic reactive scaffolding is not externally driven behavior.
    #[cfg(feature = "prepared-prefix")]
    pub(crate) fn is_empty(&self) -> bool {
        self.state.signal_count() == 0 && self.targets.is_empty()
    }

    pub(crate) fn owns_property(''')
prefix_path = Path('crates/noon-runtime/src/replay/prefix.rs')
prefix_source = prefix_path.read_text()
old_guard = 'if self.reactive.is_some() { return Err(ReplayError::UnsupportedDomain.into()); }'
new_guard = '''if self.reactive.as_ref().is_some_and(|reactive| !reactive.is_empty()) {
            return Err(ReplayError::UnsupportedDomain.into());
        }'''
if old_guard in prefix_source:
    if prefix_source.count(old_guard) != 2:
        raise RuntimeError('Expected both bootstrap and extension reactive admission guards')
    prefix_path.write_text(prefix_source.replace(old_guard, new_guard))
elif prefix_source.count('!reactive.is_empty()') != 2:
    raise RuntimeError('Reactive prefix admission has an unexpected shape')

test_path = Path('crates/noon/tests/prepared_prefix_incremental.rs')
test_source = test_path.read_text()
test_path.write_text(test_source.replace(
    'let (mut scene, object, mut source) = recorded_scene();',
    'let (scene, object, mut source) = recorded_scene();'))

replace_once('docs/architecture.md',
    '### Identity generations, revisions, versions and sequences',
    '''### Experimental completed-prefix delivery

The opt-in `prepared-prefix` experiment keeps the producer's sole Semantic Scene
and uses the existing runtime evaluator in an independent derived history reader.
After one explicit execution bootstrap, completed extensions transfer only new
compiler-owned row/track revisions. Appending future history does not advance the
reader or replace its displayed frame. Source-owner segment/callback admission
precedes extraction; a tokenless wait needs its actual continuation boundary too.
The advertised interval is half-open `[start, prepared_frontier)`, since further
source statements may still mutate the frontier's exact timestamp. Only one
ordered, scope-bound stream may extend a reader; budgets and failures preserve
already-published history. New suffix conversion uses a sparse reverse overlay,
not a copy of the source scene or a rewind of either execution cursor.

This initial slice admits fixed membership/content and ordinary property tracks.
An empty reactive runtime installed by semantic lowering is admissible; actual
signals or bindings, including later enrollment, require their own history policy.
Structural/resource extension, family channels, reactive/native input, host
callbacks, terminal endpoint publication and a realtime producer/player shell
remain outside admission. It is disabled by default and does not change gallery
presentation or establish physical-device performance. Implementation and
qualification remain in #2211; this is not another scene or rendering authority.

### Identity generations, revisions, versions and sequences''')
print('Prepared-prefix wiring applied; production defaults unchanged.')
