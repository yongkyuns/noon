from pathlib import Path
import re


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


compile_path = Path("crates/noon-compile/src/lib.rs")
text = compile_path.read_text()
text = replace_once(
    text,
    "    pub dynamic: DynamicProperties,\n}",
    "    pub dynamic: DynamicProperties,\n    /// Whether this stable compiled slot currently owns a live semantic object.\n    pub live: bool,\n}",
    "CompiledObject live field",
)
text = replace_once(
    text,
    "    object_indices: BTreeMap<ObjectId, u32>,\n}",
    "    object_indices: BTreeMap<ObjectId, u32>,\n    free_object_indices: Vec<u32>,\n}",
    "CompiledScene free list",
)
text = replace_once(
    text,
    "                dynamic: DynamicProperties::default(),\n            });\n        }\n\n        let mut tracks",
    "                dynamic: DynamicProperties::default(),\n                live: true,\n            });\n        }\n\n        let mut tracks",
    "initial live object",
)
text = replace_once(
    text,
    "            object_indices,\n        })",
    "            object_indices,\n            free_object_indices: Vec::new(),\n        })",
    "initial free list",
)

old_create = '''            ScenePatch::CreateObject(object) => {
                if self.object_indices.contains_key(&object.id) {
                    return Err(CompilePatchError::DuplicateObject(object.id));
                }
                let index = u32::try_from(self.objects.len())
                    .map_err(|_| CompilePatchError::TooManyObjects(self.objects.len()))?;
                self.objects.push(CompiledObject {
                    id: object.id,
                    geometry: object.geometry.clone(),
                    base_transform: object.transform,
                    base_style: object.style,
                    dynamic: DynamicProperties::default(),
                });
                self.object_indices.insert(object.id, index);
            }
'''
new_create = '''            ScenePatch::CreateObject(object) => {
                if self.object_indices.contains_key(&object.id) {
                    return Err(CompilePatchError::DuplicateObject(object.id));
                }
                let compiled = CompiledObject {
                    id: object.id,
                    geometry: object.geometry.clone(),
                    base_transform: object.transform,
                    base_style: object.style,
                    dynamic: DynamicProperties::default(),
                    live: true,
                };
                let index = if let Some(index) = self.free_object_indices.pop() {
                    self.objects[index as usize] = compiled;
                    index
                } else {
                    let index = u32::try_from(self.objects.len())
                        .map_err(|_| CompilePatchError::TooManyObjects(self.objects.len()))?;
                    self.objects.push(compiled);
                    index
                };
                self.object_indices.insert(object.id, index);
            }
'''
text = replace_once(text, old_create, new_create, "create object slot reuse")

old_remove = '''            ScenePatch::RemoveObject(id) => {
                let index = self
                    .object_index(*id)
                    .ok_or(CompilePatchError::UnknownObject(*id))?;
                self.objects.remove(index as usize);
                self.tracks.retain(|track| track.object_index != index);
                for track in &mut self.tracks {
                    if track.object_index > index {
                        track.object_index -= 1;
                    }
                }
                self.rebuild_object_indices();
                self.recompute_dynamic();
            }
'''
new_remove = '''            ScenePatch::RemoveObject(id) => {
                let index = self
                    .object_indices
                    .remove(id)
                    .ok_or(CompilePatchError::UnknownObject(*id))?;
                let object = &mut self.objects[index as usize];
                debug_assert!(object.live);
                object.live = false;
                object.dynamic = DynamicProperties::default();
                self.tracks.retain(|track| track.object_index != index);
                self.free_object_indices.push(index);
            }
'''
text = replace_once(text, old_remove, new_remove, "remove object tombstone")

rebuild = '''    fn rebuild_object_indices(&mut self) {
        self.object_indices.clear();
        for (index, object) in self.objects.iter().enumerate() {
            let index = u32::try_from(index).expect("compiled object count already validated");
            self.object_indices.insert(object.id, index);
        }
    }

'''
text = replace_once(text, rebuild, "", "remove dense reindex helper")
compile_path.write_text(text)

runtime_path = Path("crates/noon-runtime/src/lib.rs")
text = runtime_path.read_text()
text = replace_once(
    text,
    "pub struct FrameObjectState {\n    pub id: ObjectId,",
    "pub struct FrameObjectState {\n    /// Stable compiled slot liveness. Tombstoned slots remain addressable but are never rendered.\n    pub live: bool,\n    pub id: ObjectId,",
    "frame live field",
)
text = replace_once(
    text,
    "    pub fn is_present(&self, object_index: usize) -> bool {\n        self.presences[object_index]\n    }",
    "    pub fn is_present(&self, object_index: usize) -> bool {\n        self.objects[object_index].live && self.presences[object_index]\n    }\n\n    pub fn is_live(&self, object_index: usize) -> bool {\n        self.objects[object_index].live\n    }\n\n    pub fn live_object_count(&self) -> usize {\n        self.objects.iter().filter(|object| object.live).count()\n    }",
    "frame live helpers",
)
text = replace_once(
    text,
    "        .map(|(index, object)| FrameObjectState {\n            id: object.id,",
    "        .map(|(index, object)| FrameObjectState {\n            live: object.live,\n            id: object.id,",
    "base frame live value",
)
runtime_path.write_text(text)

for path in Path("crates").rglob("*.rs"):
    if path == runtime_path:
        continue
    source = path.read_text()
    updated = re.sub(
        r"FrameObjectState \{\n(?P<indent>\s*)id:",
        lambda m: f"FrameObjectState {{\n{m.group('indent')}live: true,\n{m.group('indent')}id:",
        source,
    )
    if updated != source:
        path.write_text(updated)

live_patch = Path("crates/noon-runtime/tests/live_patch.rs")
text = live_patch.read_text()
old_helper = '''    live.seek(time).expect("valid seek");
    assert_eq!(live.frame(), expected.frame());
}'''
new_helper = '''    live.seek(time).expect("valid seek");
    let live_objects = live
        .frame()
        .objects
        .iter()
        .enumerate()
        .filter(|(index, _)| live.frame().is_live(*index))
        .map(|(index, object)| {
            (
                object.clone(),
                live.frame().presences[index],
                live.frame().reveals[index],
                live.frame().morphs[index],
                live.frame().render_geometries[index].clone(),
            )
        })
        .collect::<Vec<_>>();
    let expected_objects = expected
        .frame()
        .objects
        .iter()
        .enumerate()
        .filter(|(index, _)| expected.frame().is_live(*index))
        .map(|(index, object)| {
            (
                object.clone(),
                expected.frame().presences[index],
                expected.frame().reveals[index],
                expected.frame().morphs[index],
                expected.frame().render_geometries[index].clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(live.frame().time, expected.frame().time);
    assert_eq!(live_objects, expected_objects);
}'''
text = replace_once(text, old_helper, new_helper, "live patch semantic comparison")
text = replace_once(
    text,
    "    assert_eq!(live.frame().objects.len(), 1);\n    assert_eq!(live.frame().objects[0].id, created);",
    "    assert_eq!(live.frame().live_object_count(), 1);\n    let created_index = live\n        .frame()\n        .objects\n        .iter()\n        .position(|object| object.live && object.id == created)\n        .expect(\"created object stays live\");\n    assert_eq!(live.frame().objects[created_index].id, created);",
    "live patch object count",
)
live_patch.write_text(text)

for path in [Path("crates/noon-web/src/legacy.rs"), Path("crates/noon-web/src/reactive_player.rs")]:
    text = path.read_text()
    text = text.replace("self.instance.frame().objects.len()", "self.instance.frame().live_object_count()")
    path.write_text(text)

compile_tests = Path("crates/noon-compile/tests/stable_slots.rs")
compile_tests.write_text('''use noon_compile::CompiledScene;
use noon_core::{GeometryRef, ObjectDefinition, ObjectId, SceneDefinition, ScenePatch};

#[test]
fn removal_tombstones_slot_without_renumbering_unrelated_objects() {
    let mut scene = SceneDefinition::new();
    let ids = (0..100_000)
        .map(|_| scene.add(GeometryRef::circle(1.0)))
        .collect::<Vec<_>>();
    let mut compiled = CompiledScene::compile(&scene).expect("scene compiles");
    let before_11 = compiled.object_index(ids[11]).expect("object 11");
    let before_last = compiled.object_index(*ids.last().unwrap()).expect("last object");
    let removed_slot = compiled.object_index(ids[10]).expect("object 10");

    compiled
        .apply_patch(&ScenePatch::RemoveObject(ids[10]))
        .expect("remove");
    assert_eq!(compiled.object_index(ids[11]), Some(before_11));
    assert_eq!(compiled.object_index(*ids.last().unwrap()), Some(before_last));
    assert!(!compiled.objects()[removed_slot as usize].live);

    let replacement = ObjectDefinition::new(
        ObjectId::new(200_000),
        GeometryRef::rectangle(2.0, 1.0),
    );
    compiled
        .apply_patch(&ScenePatch::CreateObject(replacement))
        .expect("create");
    assert_eq!(compiled.object_index(ObjectId::new(200_000)), Some(removed_slot));
    assert!(compiled.objects()[removed_slot as usize].live);
}
''')

doc = Path("docs/execution-slots.md")
text = doc.read_text()
text += '''\n## Slot-addressed compiled storage\n\n`CompiledScene` object indices are now stable slot addresses. Removal tombstones a slot and removes only tracks targeting that slot; later object indices are never decremented or rebuilt. Creation reuses free slots before extending slot capacity. `FrameObjectState.live` carries the tombstone boundary into runtime/render preparation, and browser object-count metrics report live objects rather than slot capacity.\n\nThe next #58 slice localizes timeline channel relowering and scheduler event updates; the temporary full group/scheduler rebuild in `SceneInstance::apply_patch` is intentionally left visible until that change is validated independently.\n'''
doc.write_text(text)
