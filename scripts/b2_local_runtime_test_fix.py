from pathlib import Path

Path("crates/noon-runtime/tests/local_timeline_patch.rs").write_text(r'''use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, Property, SceneDefinition, ScenePatch, TrackDefinition, TrackId,
    TrackTiming, TrackValues, Vec2,
};
use noon_runtime::SceneInstance;

#[test]
fn one_channel_edit_does_not_rebuild_hundred_thousand_runtime_groups() {
    let mut scene = SceneDefinition::new();
    let mut objects = Vec::with_capacity(100_000);
    for index in 0..100_000u64 {
        let object = scene.add(GeometryRef::circle(1.0));
        objects.push(object);
        let id = scene
            .add_track(
                object,
                Property::Position,
                TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::new(1.0, 0.0),
                },
                TrackTiming::new(1000.0 + index as f64, 1.0, Easing::Linear),
            )
            .expect("valid sparse track");
        assert_eq!(id, TrackId::new(index));
    }
    let compiled = CompiledScene::compile(&scene).expect("scene compiles");
    let mut runtime = SceneInstance::new(compiled);
    runtime.seek(0.0).expect("valid seek");

    runtime
        .apply_patch(&ScenePatch::ReplaceTrack(TrackDefinition {
            id: TrackId::new(50_000),
            object: objects[50_000],
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(5.0, 0.0),
            },
            timing: TrackTiming::new(2.0, 1.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        }))
        .expect("local replacement succeeds");

    let stats = runtime.last_patch_stats();
    assert_eq!(stats.affected_objects, 1);
    assert_eq!(stats.groups_rebuilt, 1);
    assert_eq!(stats.scheduler_groups_rebuilt, 1);
    assert_eq!(stats.full_group_rebuilds, 0);
    assert_eq!(stats.full_scheduler_rebuilds, 0);
    let scheduler = runtime.last_timeline_scheduler_patch_stats();
    assert_eq!(scheduler.groups_rebuilt, 1);
    assert_eq!(scheduler.events_removed, 2);
    assert_eq!(scheduler.events_added, 2);
}
''')

# Keep the zero-allocation active-set request invariant introduced by #137.
scheduler_path = Path("crates/noon-runtime/src/reactive/timeline_scheduler.rs")
text = scheduler_path.read_text()
old = '''            let active = self.active_groups.clone();
            for group in active {
                self.request(group);
            }
'''
if text.count(old) != 1:
    raise SystemExit(f"backward active snapshot: expected one match, found {text.count(old)}")
text = text.replace(old, "            self.request_active_groups();\n", 1)
old = '''        let active = self.active_groups.clone();
        for group in active {
            self.request(group);
        }
'''
if text.count(old) != 1:
    raise SystemExit(f"forward active snapshot: expected one match, found {text.count(old)}")
text = text.replace(old, "        self.request_active_groups();\n", 1)
needle = '''    fn request(&mut self, group: TrackGroupKey) {
'''
method = '''    /// Request active stable channels without cloning the active set.
    fn request_active_groups(&mut self) {
        for index in 0..self.active_groups.len() {
            let group = self.active_groups[index];
            self.request(group);
        }
    }

'''
if text.count(needle) != 1:
    raise SystemExit(f"request method insertion: expected one match, found {text.count(needle)}")
text = text.replace(needle, method + needle, 1)
scheduler_path.write_text(text)
