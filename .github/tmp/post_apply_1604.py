from pathlib import Path

# Test-only constructor: StoredGeometry has Rectangle, not Square.
core = Path('crates/noon-core/src/semantic_store/object_content.rs')
text = core.read_text()
old = 'StoredGeometry::Square { side: 1.0 }'
new = 'StoredGeometry::Rectangle { size: Vec2::new(1.0, 1.0) }'
assert text.count(old) == 1
core.write_text(text.replace(old, new, 1))

# Source-less expansion must start from ordinary receiver metadata, never target
# presentation/role/bindings. Only a real receiver-side prototype may contribute
# receiver-owned metadata.
state = Path('crates/noon/src/state_replacement.rs')
text = state.read_text()
old = '''                        let receiver_state = self
                            .store
                            .semantic_object_state_checked(prototype.unwrap_or(target))
                            .ok()
                            .cloned()
                            .unwrap_or_else(|| SemanticObjectState::new(state.content))
                            .with_visual_state_from(state);'''
new = '''                        let receiver_state = prototype
                            .and_then(|prototype| {
                                self.store.semantic_object_state_checked(prototype).ok()
                            })
                            .cloned()
                            .unwrap_or_else(|| SemanticObjectState::new(state.content))
                            .with_visual_state_from(state);'''
assert text.count(old) == 1
text = text.replace(old, new, 1)

old = 'aligned_source_prototype(current, target_members.len(), index),'
new = 'aligned_source_prototype(&current, target_members.len(), index),'
assert text.count(old) == 1
state.write_text(text.replace(old, new, 1))
