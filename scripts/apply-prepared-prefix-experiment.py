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
Structural/resource extension, family channels, reactive/native input, host
callbacks, terminal endpoint publication and a realtime producer/player shell
remain outside admission. It is disabled by default and does not change gallery
presentation or establish physical-device performance. Implementation and
qualification remain in #2211; this is not another scene or rendering authority.

### Identity generations, revisions, versions and sequences''')
print('Prepared-prefix wiring applied; production defaults unchanged.')
