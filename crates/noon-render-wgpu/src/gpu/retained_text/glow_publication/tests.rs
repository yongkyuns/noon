use super::*;
use noon_compile::{CompiledGlow, CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{Glow, GlowUpdate, Pixels, SemanticNodeId};
use noon_runtime::SceneInstance;

fn runtime(count: usize, glowing: bool, mixed: bool) -> SceneInstance {
    let mut objects = (0..count)
        .map(|index| {
            CompiledObject::new(
                ObjectId::new(100 + index as u64),
                GeometryRef::circle(0.4),
                Transform2D {
                    translation: Vec2::new(if index == 1 { -4.3 } else { 0.0 }, 0.0),
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
    if glowing {
        objects[1].glow = Some(Arc::new(CompiledGlow {
            attachment: SemanticNodeId::new(31, 5),
            definition: Glow::new(GlowUpdate::default().radius(Pixels(3.0))).unwrap(),
        }));
    }
    let mut resources = noon_compile::CompiledResources::default();
    if mixed {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let bounds = resources
            .capture_text_from_arenas(
                &texts,
                &artifact.fonts,
                &GeometryResourceArena::new(),
                handle,
            )
            .unwrap();
        objects[0].content = ObjectContentRef::Text(handle);
        objects[0].text_bounds = Some(bounds);
    }
    // Painter order differs from source index order: a visibility union must NOT
    // sort by object index. This also forces original index1 -> mixed scratch0.
    objects[0].base_z_index = 2.0;
    objects[1].base_z_index = 1.0;
    let mut compiled = CompiledScene::compile_objects(objects, &[]).unwrap();
    compiled.merge_prepared_resources(resources);
    SceneInstance::new(compiled)
}

struct Fixture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
}
impl Fixture {
    fn new() -> Self {
        let (device, queue) = wgpu::Device::noop(&Default::default());
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, 80, 60);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
        );
        let text = renderer.create_retained_text_state(&device, &queue);
        Self {
            device,
            queue,
            renderer,
            preparer: RetainedFramePreparer::new(),
            text,
        }
    }

    fn prepare(&mut self, runtime: &mut SceneInstance, submit: bool) -> RetainedGlowStats {
        let publication = runtime.take_renderer_publication();
        let mut visible = publication
            .painter_order()
            .iter()
            .map(|&i| i as usize)
            .filter(|&i| i != 1)
            .collect::<Vec<_>>();
        let expanded = self
            .renderer
            .glow_source_visibility(&publication, &visible)
            .unwrap();
        if publication.frame().objects[1].glow.is_some() && publication.frame().is_present(1) {
            assert_eq!(
                expanded,
                publication
                    .painter_order()
                    .iter()
                    .map(|&i| i as usize)
                    .collect::<Vec<_>>()
            );
        }
        let metrics = TextDeviceMetrics::uniform(10.0).unwrap();
        let prepared = self
            .preparer
            .prepare_planned_publication_visible(&self.device, &publication, expanded, metrics)
            .unwrap();
        self.renderer
            .upload_retained(&self.device, &self.queue, &prepared, &mut self.text);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let result = self
            .renderer
            .prepare_retained_analytic_glows(
                &self.device,
                &self.queue,
                &mut encoder,
                &prepared,
                &publication,
                1_000_000,
            )
            .unwrap();
        if submit {
            self.queue.submit([encoder.finish()]);
        } else {
            drop(encoder);
            self.renderer.invalidate_analytic_glows();
        }
        visible.clear();
        result
    }
}

#[test]
fn no_effect_visibility_is_borrowed_and_clean_glow_path_is_zero_work() {
    let mut fixture = Fixture::new();
    let mut runtime = runtime(4096, false, false);
    let publication = runtime.take_renderer_publication();
    let visible = [2, 0];
    let actual = fixture
        .renderer
        .glow_source_visibility(&publication, &visible)
        .unwrap();
    assert_eq!(actual.as_ptr(), visible.as_ptr());
    assert!(fixture.renderer.retained_glow.sources.is_empty());
    drop(publication);
    fixture.prepare(&mut runtime, true);
    let clean = fixture.prepare(&mut runtime, true);
    assert_eq!(clean, RetainedGlowStats::default());
    assert!(fixture.renderer.analytic_glows.is_none());
    assert_eq!(fixture.renderer.retained_glow.visible.capacity(), 0);
}

#[test]
fn automatic_prepare_preserves_painter_order_mixed_mapping_and_local_updates() {
    for mixed in [false, true] {
        let mut fixture = Fixture::new();
        let mut runtime = runtime(4096, true, mixed);
        let first = fixture.prepare(&mut runtime, true);
        assert_eq!(first.rows_examined, 4096);
        assert_eq!(
            (
                first.sources_prepared,
                first.source_passes,
                first.blur_passes
            ),
            (1, 1, 2)
        );
        let clean = fixture.prepare(&mut runtime, true);
        assert_eq!(
            clean,
            RetainedGlowStats {
                retained_texture_bytes: first.retained_texture_bytes,
                ..Default::default()
            }
        );
        let row = &runtime.frame().objects[1];
        let mut glow = **row.glow.as_ref().unwrap();
        glow.definition = GlowUpdate::default()
            .intensity(0.3)
            .apply_to(glow.definition)
            .unwrap();
        runtime
            .apply_execution_patch(&ExecutionPatch::SetGlow {
                object: row.id,
                glow: Arc::new(glow),
            })
            .unwrap();
        let edited = fixture.prepare(&mut runtime, true);
        assert_eq!((edited.rows_examined, edited.sources_prepared), (1, 1));
        assert_eq!(
            (
                edited.source_passes,
                edited.blur_passes,
                edited.composite_passes
            ),
            (0, 0, 1)
        );
        assert_eq!((edited.bytes_uploaded, edited.texture_allocations), (32, 0));
        let removed = runtime.frame().objects[1].id;
        runtime
            .apply_execution_patch(&ExecutionPatch::RemoveObject(removed))
            .unwrap();
        let retired = fixture.prepare(&mut runtime, true);
        assert_eq!(retired.retained_texture_bytes, 0);
        assert!(fixture.renderer.retained_glow.packed.is_empty());
    }
}

#[test]
fn omitted_source_is_rejected_and_abandoned_encoder_reprepares() {
    let mut fixture = Fixture::new();
    let mut runtime = runtime(3, true, false);
    let publication = runtime.take_renderer_publication();
    fixture
        .renderer
        .glow_source_visibility(&publication, &[2, 0])
        .unwrap();
    // Deliberately violate the visibility contract: the populated source map is
    // not sufficient evidence that this *prepared* draw retained its source.
    let prepared = fixture
        .preparer
        .prepare_planned_publication_visible(
            &fixture.device,
            &publication,
            &[2, 0],
            TextDeviceMetrics::uniform(10.0).unwrap(),
        )
        .unwrap();
    fixture.renderer.upload_retained(
        &fixture.device,
        &fixture.queue,
        &prepared,
        &mut fixture.text,
    );
    let mut encoder = fixture.device.create_command_encoder(&Default::default());
    assert_eq!(
        fixture.renderer.prepare_retained_analytic_glows(
            &fixture.device,
            &fixture.queue,
            &mut encoder,
            &prepared,
            &publication,
            1_000_000
        ),
        Err(GlowPrepareError::PublicationMismatch)
    );
    drop((encoder, prepared, publication));
    let abandoned = fixture.prepare(&mut runtime, false);
    assert_eq!(abandoned.source_passes, 1);
    let retry = fixture.prepare(&mut runtime, true);
    assert_eq!((retry.source_passes, retry.blur_passes), (1, 2));
    assert_eq!(retry.texture_allocations, 0);
    let clean = fixture.prepare(&mut runtime, true);
    assert_eq!(
        clean.source_passes + clean.blur_passes + clean.bytes_uploaded,
        0
    );
    fixture.renderer.set_camera(
        &fixture.queue,
        Camera2D::new(Vec2::new(0.1, 0.0), Vec2::new(8.0, 6.0)).unwrap(),
    );
    let moved = fixture.prepare(&mut runtime, true);
    assert_eq!(moved.rows_examined, 0);
    assert_eq!(moved.sources_prepared, 1);
    assert_eq!(moved.source_passes, 1);
}

#[test]
fn bad_visibility_and_reduced_clean_budget_fail_without_hiding_work() {
    let mut fixture = Fixture::new();
    let mut runtime = runtime(3, true, true);
    let first = fixture.prepare(&mut runtime, true);
    let publication = runtime.take_renderer_publication();
    for bad in [&[0, 0][..], &[3][..]] {
        assert_eq!(
            fixture.renderer.glow_source_visibility(&publication, bad),
            Err(GlowPrepareError::PublicationMismatch)
        );
    }
    let expanded = fixture
        .renderer
        .glow_source_visibility(&publication, &[2, 0])
        .unwrap();
    let prepared = fixture
        .preparer
        .prepare_planned_publication_visible(
            &fixture.device,
            &publication,
            expanded,
            TextDeviceMetrics::uniform(10.0).unwrap(),
        )
        .unwrap();
    fixture.renderer.upload_retained(
        &fixture.device,
        &fixture.queue,
        &prepared,
        &mut fixture.text,
    );
    let mut encoder = fixture.device.create_command_encoder(&Default::default());
    assert_eq!(
        fixture.renderer.prepare_retained_analytic_glows(
            &fixture.device,
            &fixture.queue,
            &mut encoder,
            &prepared,
            &publication,
            first.retained_texture_bytes - 1
        ),
        Err(GlowPrepareError::ScratchBudgetExceeded)
    );
    assert_eq!(
        fixture.renderer.analytic_glow_texture_bytes(),
        first.retained_texture_bytes
    );
    drop((encoder, prepared, publication));
    let recovered = fixture.prepare(&mut runtime, true);
    assert_eq!(
        (
            recovered.source_passes,
            recovered.blur_passes,
            recovered.texture_allocations
        ),
        (1, 2, 0)
    );
}

#[test]
fn order_only_publication_reuses_glow_pixels_and_stale_publication_is_rejected() {
    let mut fixture = Fixture::new();
    let mut scene = runtime(3, true, true);
    fixture.prepare(&mut scene, true);
    scene
        .apply_execution_patch(&ExecutionPatch::SetZIndex {
            object: ObjectId::new(101),
            value: -2.0,
        })
        .unwrap();
    let changed = fixture.prepare(&mut scene, true);
    // Z-order publishes a painter-range change, not a value-row change.
    assert_eq!(changed.rows_examined, 0);
    assert_eq!(
        changed.source_passes + changed.blur_passes + changed.texture_allocations,
        0
    );
    let context = fixture.renderer.retained_glow.context;
    let bytes = fixture.renderer.analytic_glow_texture_bytes();
    let mut old = runtime(3, true, true);
    let old_publication = old.take_renderer_publication();
    assert_eq!(
        fixture
            .renderer
            .glow_source_visibility(&old_publication, &[2, 0]),
        Err(GlowPrepareError::PublicationMismatch)
    );
    assert_eq!(fixture.renderer.retained_glow.context, context);
    assert_eq!(fixture.renderer.analytic_glow_texture_bytes(), bytes);
}

#[test]
fn already_visible_glow_borrows_candidates_and_keeps_ordinary_validation() {
    let mut fixture = Fixture::new();
    let mut scene = runtime(4096, true, true);
    let publication = scene.take_renderer_publication();
    let visible = publication
        .painter_order()
        .iter()
        .map(|&index| index as usize)
        .collect::<Vec<_>>();
    let expanded = fixture
        .renderer
        .glow_source_visibility(&publication, &visible)
        .unwrap();
    assert_eq!(expanded.as_ptr(), visible.as_ptr());
    assert_eq!(expanded.len(), visible.len());
    let _ = fixture
        .preparer
        .prepare_planned_publication_visible(
            &fixture.device,
            &publication,
            expanded,
            TextDeviceMetrics::uniform(10.0).unwrap(),
        )
        .unwrap();
    assert_eq!(fixture.renderer.retained_glow.visible.capacity(), 0);
    // Even on the borrowed fast path, the existing retained owner rejects a bad
    // candidate list; expansion must never turn duplicate source input valid.
    let duplicate = [1, 1];
    let expanded = fixture
        .renderer
        .glow_source_visibility(&publication, &duplicate)
        .unwrap();
    assert!(fixture
        .preparer
        .prepare_planned_publication_visible(
            &fixture.device,
            &publication,
            expanded,
            TextDeviceMetrics::uniform(10.0).unwrap()
        )
        .is_err());
}
