use super::*;
use noon_compile::{CompiledEffect, CompiledEffectStack, CompiledObject, CompiledScene};
use noon_core::{
    BackdropBlur, CompositionTimeMap, EffectUpdate, GeometryRef, Pixels, Property, SemanticNodeId,
    Style, TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D,
};
use noon_runtime::{ExecutionSlotId, ExecutionSpatialIndex, SceneInstance};

fn row(id: u64, z: f64) -> CompiledObject {
    let mut row = CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::circle(0.3),
        Transform2D::IDENTITY,
        Style::default(),
    );
    row.base_z_index = z;
    row
}
fn spatial() -> Box<noon_compile::CompiledSpatialState> {
    Box::new(noon_compile::CompiledSpatialState {
        world: noon_core::SemanticWorldTransform3D::IDENTITY,
        camera_projection: None,
        camera_profile: None,
        camera_motions: None,
        material: noon_core::SemanticSpatialMaterial::Unlit,
        point_light: false,
        composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
        draw_kind: noon_compile::CompiledSpatialDrawKind::Planar,
        spatial_anchor_family: None,
        fixed_orientation_center: None,
        cairo_path_appearance: None,
    })
}
fn fixture(count: usize, unsupported: bool) -> SceneInstance {
    let mut panel = row(20, 1.0);
    panel.effects = Some(Arc::new(
        CompiledEffectStack::new([CompiledEffect {
            attachment: SemanticNodeId::new(97, 4),
            definition: BackdropBlur::new(EffectUpdate::default().radius(Pixels(2.0)))
                .unwrap()
                .into(),
        }])
        .unwrap(),
    ));
    let mut moving = row(30, 2.0);
    if unsupported {
        moving.spatial = Some(spatial());
    }
    let mut rows = vec![row(10, 0.0), panel, moving];
    for i in 0..count {
        let mut extra = row(100 + i as u64, 3.0 + i as f64);
        extra.base_transform.translation.x = 30.0 + i as f32;
        if unsupported {
            extra.spatial = Some(spatial());
        }
        rows.push(extra);
    }
    let track = TrackDefinition {
        id: TrackId::new(1),
        object: ObjectId::new(20),
        property: Property::ZIndex,
        values: TrackValues::ZIndex { from: 1.0, to: 3.0 },
        timing: TrackTiming::instant(1.0),
        time_map: CompositionTimeMap::identity(),
    };
    SceneInstance::new(CompiledScene::compile_objects(rows, &[track]).unwrap())
}
fn inspect(
    renderer: &mut GpuRenderer,
    runtime: &mut SceneInstance,
) -> Result<(), EffectPrepareError> {
    let mut index = ExecutionSpatialIndex::default();
    index.rebuild(
        runtime.frame(),
        runtime.geometry_resources(),
        runtime
            .painter_order()
            .iter()
            .map(|&i| (ExecutionSlotId::new(i, 0), i as usize)),
    );
    let publication = runtime
        .take_renderer_publication()
        .with_spatial_index(&index);
    let visible = publication
        .painter_order()
        .iter()
        .map(|&i| i as usize)
        .collect::<Vec<_>>();
    renderer
        .effect_source_visibility(&publication, &visible)
        .map(|_| ())
}
#[test]
fn animated_prefix_crossing_an_unsupported_input_is_rejected_before_gpu_work_and_rewind_recovers() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let mut runtime = fixture(20_000, true);
    inspect(&mut renderer, &mut runtime).unwrap();
    assert_eq!(renderer.retained_effects.prefix_admission_rows, 20_003);
    renderer.retained_effects.prefix_admission_rows = 0;
    runtime.advance_to(0.5).unwrap();
    inspect(&mut renderer, &mut runtime).unwrap();
    assert_eq!(
        renderer.retained_effects.prefix_admission_rows, 0,
        "clean guard reads no rows"
    );
    runtime.advance_to(1.0).unwrap();
    assert_eq!(&runtime.painter_order()[..3], &[0, 2, 1]);
    assert_eq!(
        inspect(&mut renderer, &mut runtime),
        Err(EffectPrepareError::UnsupportedBackdropInput {
            owner: ObjectId::new(20),
            input: ObjectId::new(30)
        })
    );
    assert_eq!(
        renderer.retained_effects.prefix_admission_rows, 2,
        "only changed/crossed rows are checked among 20,003"
    );
    assert_eq!(renderer.effect_capture_texture_bytes(), 0);
    assert!(renderer.effect_captures.is_none());
    runtime.seek(0.0).unwrap();
    inspect(&mut renderer, &mut runtime).unwrap();
    assert_eq!(&runtime.painter_order()[..3], &[0, 1, 2]);
    assert_eq!(renderer.effect_capture_texture_bytes(), 0);
    assert!(!renderer.retained_effects.sync_failed);
}
#[test]
fn animated_planar_order_crossing_updates_the_same_prefix_selection() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let mut runtime = fixture(0, false);
    inspect(&mut renderer, &mut runtime).unwrap();
    assert_eq!(
        &*renderer.retained_effects.backdrops.entries[&1].candidates,
        &[ObjectId::new(10)]
    );
    renderer.retained_effects.prefix_admission_rows = 0;
    runtime.advance_to(1.0).unwrap();
    inspect(&mut renderer, &mut runtime).unwrap();
    assert_eq!(
        &*renderer.retained_effects.backdrops.entries[&1].candidates,
        &[ObjectId::new(10), ObjectId::new(30)]
    );
    assert_eq!(renderer.retained_effects.prefix_admission_rows, 1,
        "only the tracked target anchor needs capability admission; the crossed planar input does not");
}

#[test]
fn animated_member_crossing_uses_group_physical_anchor_instead_of_controller_rank() {
    let mut panel = row(20, 1.0);
    panel.base_style.opacity = 0.65;
    let mut unsupported = row(30, 2.0);
    unsupported.spatial = Some(spatial());
    let mut group = CompiledObject::new(
        ObjectId::new(90),
        noon_core::ObjectContentRef::Composed(Arc::new(
            noon_core::ComposedSource::new([ObjectId::new(20)], None).unwrap(),
        )),
        Transform2D::IDENTITY,
        Style {
            fill: None,
            stroke: None,
            opacity: 0.6,
            ..Style::default()
        },
    );
    group.draw_enabled = false;
    group.effects = Some(Arc::new(
        CompiledEffectStack::new([CompiledEffect {
            attachment: SemanticNodeId::new(97, 4),
            definition: BackdropBlur::new(EffectUpdate::default().radius(Pixels(2.0)))
                .unwrap()
                .into(),
        }])
        .unwrap(),
    ));
    let track = TrackDefinition {
        id: TrackId::new(1),
        object: ObjectId::new(20),
        property: Property::ZIndex,
        values: TrackValues::ZIndex { from: 1.0, to: 3.0 },
        timing: TrackTiming::instant(1.0),
        time_map: Default::default(),
    };
    let mut runtime = SceneInstance::new(
        CompiledScene::compile_objects(vec![row(10, 0.0), panel, unsupported, group], &[track])
            .unwrap(),
    );
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, 80, 60);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    inspect(&mut renderer, &mut runtime).unwrap();
    let controller = runtime.painter_rank(3);
    renderer.retained_effects.prefix_admission_rows = 0;
    runtime.advance_to(1.0).unwrap();
    assert_eq!(
        runtime.painter_rank(3),
        controller,
        "metadata rank did not move"
    );
    assert_eq!(
        inspect(&mut renderer, &mut runtime),
        Err(EffectPrepareError::UnsupportedBackdropInput {
            owner: ObjectId::new(90),
            input: ObjectId::new(30)
        })
    );
    assert_eq!(renderer.retained_effects.prefix_admission_rows, 2);
    assert_eq!(renderer.effect_capture_texture_bytes(), 0);
}
