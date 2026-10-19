#![cfg(feature = "prepared-prefix")]
// A real retained GPU-renderer witness for an appended immutable family plan.
// No renderer-cache, host callback, or speculative replay shortcut is involved.
mod raster_support;

use noon_compile::{CompiledFamilyAnimation, CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{
    Color, CompositionTimeMap, FamilyAnimationMode, FamilyAnimationSpec, GeometryRef,
    ObjectId, RateFunction, RetainedAnimationMembers, RetainedFamilyAnimationPlan,
    SemanticStore, Style, TextResourceArena, Transform2D,
};
use noon_runtime::{ReplayLimits, SceneInstance};

#[test]
#[ignore = "requires actual Vulkan fallback adapter; run in focused CI"]
fn appended_family_plan_reproduces_exact_frames_after_source_destroyed() {
    pollster::block_on(async {
        let object = CompiledObject::new(
            ObjectId::new(40), GeometryRef::circle(0.75), Transform2D::IDENTITY,
            Style {
                fill: Some(Color::WHITE),
                ..Style::default()
            },
        );
        let members = RetainedAnimationMembers::resolve(
            &object.content, &TextResourceArena::new(),
        ).unwrap();
        let mut semantics = SemanticStore::new();
        let leaf = semantics.insert_authoring_object();
        let plan = RetainedFamilyAnimationPlan::single_leaf(
            leaf, object.id, members,
        ).unwrap();
        let mut source = SceneInstance::new(
            CompiledScene::compile_objects(vec![object.clone()], &[]).unwrap(),
        );
        source.begin_replay_retention(ReplayLimits {
            revisions: 64, payloads: 256,
        }).unwrap();
        let mut forward = raster_support::Raster::new().await;
        source.advance_to(0.5).unwrap();
        let mut samples = vec![(0.5, forward.capture(&source.take_renderer_publication()))];
        source.advance_to(1.0).unwrap();
        let (mut publisher, mut reader) = source.create_prefix_reader().unwrap();
        source.apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(
            CompiledFamilyAnimation {
                target: object.id, plan,
                spec: FamilyAnimationSpec::new(
                    FamilyAnimationMode::DrawBorderThenFill,
                    1.0, 2.0, 0.0, RateFunction::Linear, false, false,
                ).unwrap(),
                time_map: CompositionTimeMap::identity(),
            },
        )).unwrap();
        for time in [1.5, 2.0, 2.5, 3.2] {
            source.advance_to(time).unwrap();
            samples.push((time, forward.capture(&source.take_renderer_publication())));
        }
        assert_ne!(samples[0].1, samples[1].1, "family progression changes pixels");
        source.advance_to(3.5).unwrap();
        reader.append(source.prepare_prefix_update(&mut publisher).unwrap()).unwrap();
        drop(source);
        drop(publisher);

        let mut retained = raster_support::Raster::new().await;
        for _ in 0..3 {
            for (time, pixels) in samples.iter().rev() {
                reader.seek(*time).unwrap();
                assert_eq!(
                    &retained.capture(&reader.take_renderer_publication()),
                    pixels,
                    "replay differs from first-pass at {time}",
                );
            }
        }
        let mut recreated = raster_support::Raster::new().await;
        reader.seek(2.0).unwrap();
        assert_eq!(
            recreated.capture(&reader.take_renderer_publication()),
            samples[2].1,
        );
        eprintln!("M2E_FAMILY_RASTER: 16 exact comparisons passed, 5 first-pass times");
    });
}
