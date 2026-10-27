use super::*;
use crate::{text::TextDeviceMetrics, RetainedFramePreparer};
use noon_compile::{CompiledEffect, CompiledEffectStack, ExecutionPatch};
use noon_core::{ComposedSource, GlowSource, ObjectContentRef, SemanticNodeId};

fn stack(intensity: f64) -> Arc<CompiledEffectStack> {
    Arc::new(
        CompiledEffectStack::new([CompiledEffect {
            attachment: SemanticNodeId::new(9001, 2),
            definition: Glow::new(
                EffectUpdate::default()
                    .radius(Pixels(2.0))
                    .intensity(intensity)
                    .source(GlowSource::Painted),
            )
            .unwrap()
            .into(),
        }])
        .unwrap(),
    )
}

#[test]
#[cfg(feature = "ci-noop")]
fn large_group_parameter_edit_borrows_source_and_only_member_changes_recapture() {
    const COUNT: usize = 512;
    let owner = ObjectId::new(9000);
    let mut objects = (0..COUNT + 1024)
        .map(|index| {
            CompiledObject::new(
                ObjectId::new(index as u64 + 1),
                GeometryRef::rectangle(0.5, 0.5),
                Transform2D {
                    translation: if index < COUNT {
                        Vec2::ZERO
                    } else {
                        Vec2::new(100.0, 100.0)
                    },
                    ..Transform2D::IDENTITY
                },
                Style {
                    fill: Some(Color::WHITE),
                    stroke: None,
                    ..Style::default()
                },
            )
        })
        .collect::<Vec<_>>();
    let mut scope = CompiledObject::new(
        owner,
        ObjectContentRef::Composed(Arc::new(
            ComposedSource::new(objects[..COUNT].iter().map(|object| object.id), None).unwrap(),
        )),
        Transform2D::IDENTITY,
        Style {
            fill: None,
            stroke: None,
            ..Style::default()
        },
    );
    scope.effects = Some(stack(2.0));
    scope.draw_enabled = false;
    let scope_index = objects.len();
    objects.push(scope);
    let mut runtime = SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let mut text = renderer.create_retained_text_state(&device, &queue);
    let mut preparer = RetainedFramePreparer::new();
    let mut previous = None;
    for step in 0..4 {
        match step {
            1 => {
                runtime
                    .apply_execution_patch(&ExecutionPatch::SetEffects {
                        object: owner,
                        effects: stack(1.0),
                    })
                    .unwrap();
            }
            2 | 3 => {
                let mut style = Style {
                    fill: Some(Color::rgba(1.0, 0.0, 0.0, 0.5)),
                    stroke: None,
                    ..Style::default()
                };
                style.opacity = 0.75;
                runtime
                    .apply_execution_patch(&ExecutionPatch::SetStyle {
                        object: ObjectId::new(if step == 2 { 1 } else { COUNT as u64 + 1 }),
                        style,
                        fill_paint: None,
                    })
                    .unwrap();
            }
            _ => {}
        }
        let publication = runtime.take_renderer_publication();
        let visible = renderer
            .effect_source_visibility(&publication, &[COUNT])
            .unwrap();
        let prepared = preparer
            .prepare_planned_publication_visible(
                &device,
                &publication,
                visible,
                TextDeviceMetrics::uniform(10.0).unwrap(),
            )
            .unwrap();
        renderer.upload_retained(&device, &queue, &prepared, &mut text);
        if step > 0 {
            assert_eq!(
                prepared.geometry.stats.full_rebuilds, 0,
                "controller/member value edits cannot rebuild unrelated geometry"
            );
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        let work = renderer
            .prepare_retained_effects(
                &device,
                &queue,
                &mut encoder,
                &prepared,
                &text,
                &publication,
                1_000_000,
            )
            .unwrap();
        let SourceInstance::Draws { source, .. } =
            &renderer.effect_captures.as_ref().unwrap().scopes[&(2, scope_index)].effective
        else {
            panic!("group draw source")
        };
        if step == 0 {
            assert_eq!((work.sources_prepared, work.source_passes), (1, 1));
        } else {
            assert_eq!(work.rows_examined, 1, "only the published row is examined");
            assert_eq!(work.source_passes, usize::from(step == 2));
            assert_eq!(work.sources_prepared, usize::from(step != 3));
            assert_eq!(
                Arc::ptr_eq(previous.as_ref().unwrap(), source),
                step != 2,
                "parameter/unrelated edits must borrow the existing immutable prepared source"
            );
        }
        previous = Some(source.clone());
        queue.submit([encoder.finish()]);
    }
}
