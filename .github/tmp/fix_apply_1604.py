from pathlib import Path

path = Path('.github/tmp/apply_1604.py')
text = path.read_text()

old = 'StoredGeometry::Square { side: 1.0 }'
new = 'StoredGeometry::Rectangle { size: Vec2::new(1.0, 1.0) }'
assert text.count(old) == 1
text = text.replace(old, new, 1)

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

path.write_text(text)
