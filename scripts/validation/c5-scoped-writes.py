"""Staging only: apply the two small wiring edits to a pinned production base."""
from pathlib import Path
import hashlib

expected = {
    'crates/noon-runtime/src/lib.rs': '96f69d7a129a68a1dffeb37c5ff47a5ea100a51b66ce279f159e82ce4e5d1a75',
    'crates/noon-runtime/src/prepared_frame.rs': 'fa97ea6b9fee7cedf72066c77539418a1bf2e25e25b4f7e50c753d08f25f7b9a',
    'crates/noon-runtime/src/effective_write.rs': '69841294b0215dde5d594b58e869e332f25636ba7a361a02aada3044dfc7393b',
    'crates/noon-runtime/src/effective_write/tests.rs': '107159eeef1b5942158b7bb2c9b0a3d3d1e98293c202b4af4e40fa3a2f57d0f2',
}
for name, digest in expected.items():
    actual = hashlib.sha256(Path(name).read_bytes()).hexdigest()
    assert actual == digest, (name, actual, digest)

p = Path('crates/noon-runtime/src/prepared_frame.rs')
s = p.read_text()
start = s.index('/// One transient host-driver value.')
end = s.index('#[derive(Clone, Debug)]\nstruct PreparedFrameRow', start)
s = s[:start] + s[end:]
for old, new in [
    ('    EffectiveObjectProperties, FrameRowState, FrameState, RuntimeIdentity, SceneInstance,',
     '    EffectiveObjectProperties, EffectivePropertyWrite, FrameRowState, FrameState, RuntimeIdentity, SceneInstance,'),
    ('use noon_core::{ObjectId, PublicationContext, Style, Transform2D};', 'use noon_core::PublicationContext;'),
    ('CompilePatchError, CompiledChannelKey, ExecutionMutationTransaction, ExecutionPatch,',
     'CompilePatchError, CompiledChannelKey, ExecutionMutationTransaction,'),
    ('lower_semantic_execution, CompilePatchError, CompiledObject, CompiledScene,',
     'lower_semantic_execution, CompilePatchError, CompiledObject, CompiledScene, ExecutionPatch,'),
    ('''            let property_tag = match write {
                EffectivePropertyWrite::Transform { .. } => 0_u8,
                EffectivePropertyWrite::Style { .. } => 1_u8,
            };''', '''            // Full Transform/Style writes overlap their component writes. Keep
            // that relative order; only a later write of the same shape fully
            // supersedes an earlier one. Validate even superseded writes above.
            let property_tag = std::mem::discriminant(&write);'''),
]:
    assert s.count(old) == 1, old
    s = s.replace(old, new)
p.write_text(s)

p = Path('crates/noon-runtime/src/lib.rs')
s = p.read_text()
for old, new in [
    ('mod derived_display_evaluation;', 'mod derived_display_evaluation;\nmod effective_write;'),
    ('pub use derived_display_evaluation::*;', 'pub use derived_display_evaluation::*;\nuse effective_write::apply_effective_property_to_row;\npub use effective_write::EffectivePropertyWrite;'),
]:
    assert s.count(old) == 1, old
    s = s.replace(old, new)
start = s.index('fn apply_effective_property_to_row(')
end = s.index('fn apply_evaluated_value(', start)
s = s[:start] + s[end:]
p.write_text(s)
print('Verified and reconstructed four-file scoped effective write candidate')
