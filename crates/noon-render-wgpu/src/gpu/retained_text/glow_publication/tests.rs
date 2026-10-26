use super::*;
use noon_compile::{CompiledGlow, CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{Glow, GlowUpdate, Pixels, SemanticNodeId};
use noon_runtime::SceneInstance;

fn runtime(count: usize, glowing: bool, mixed: bool) -> SceneInstance {
    runtime_profile(count, glowing, mixed, None)
}

fn runtime_profile(
    count: usize,
    glowing: bool,
    mixed: bool,
    image: Option<noon_core::GlowSource>,
) -> SceneInstance {
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
    if let Some(source) = image {
        let mut images = noon_core::RasterImageResourceArena::new();
        let handle = images
            .intern_rgba8(
                2,
                2,
                vec![
                    255, 0, 0, 0, 32, 128, 255, 128, 0, 0, 255, 255, 255, 255, 0, 64,
                ],
            )
            .unwrap();
        let content = resources
            .capture_image_from_arena(&images, noon_core::SemanticImageContent::new(handle))
            .unwrap();
        objects[1].content = ObjectContentRef::Image(content);
        // The shared circle fixture inherits a nonzero default stroke width.
        // Images require the ordinary stroke-free image presentation profile.
        objects[1].base_style.stroke_width = 0.0;
        objects[1].base_style.opacity = 0.55;
        objects[1].base_style.fill = Some(Color {
            alpha: 0.43,
            ..Color::WHITE
        });
        objects[1].glow = Some(Arc::new(CompiledGlow {
            attachment: SemanticNodeId::new(31, 5),
            definition: Glow::new(GlowUpdate::default().radius(Pixels(3.0)).source(source))
                .unwrap(),
        }));
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
    last_image_upload: RasterImageUploadStats,
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
            last_image_upload: RasterImageUploadStats::default(),
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
        self.last_image_upload = self
            .renderer
            .upload_retained(&self.device, &self.queue, &prepared, &mut self.text)
            .images;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let result = self
            .renderer
            .prepare_retained_analytic_glows(
                &self.device,
                &self.queue,
                &mut encoder,
                &prepared,
                &self.text,
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
fn image_capture_reuses_texture_and_mask_for_local_effect_edits() {
    for source in [
        noon_core::GlowSource::Painted,
        noon_core::GlowSource::Silhouette,
    ] {
        let mut fixture = Fixture::new();
        let mut runtime = runtime_profile(4096, true, false, Some(source));
        let first = fixture.prepare(&mut runtime, true);
        assert_eq!(first.rows_examined, 4096);
        assert_eq!(
            first.source_passes,
            if source == noon_core::GlowSource::Painted {
                1
            } else {
                2
            }
        );
        assert_eq!(fixture.last_image_upload.textures_uploaded, 1);
        assert_eq!(fixture.last_image_upload.pixel_bytes_uploaded, 16);
        let resident = fixture.renderer.image_residency_stats();
        assert_eq!(
            (resident.textures, resident.objects, resident.pixel_bytes),
            (1, 1, 16)
        );
        let clean = fixture.prepare(&mut runtime, true);
        assert_eq!(
            clean.bytes_uploaded + clean.source_passes + clean.blur_passes + clean.composite_passes,
            0
        );
        assert_eq!(fixture.last_image_upload, RasterImageUploadStats::default());

        let object = runtime.frame().objects[1].id;
        let mut glow = **runtime.frame().objects[1].glow.as_ref().unwrap();
        glow.definition = GlowUpdate::default()
            .intensity(0.8)
            .apply_to(glow.definition)
            .unwrap();
        runtime
            .apply_execution_patch(&ExecutionPatch::SetGlow {
                object,
                glow: Arc::new(glow),
            })
            .unwrap();
        let edited = fixture.prepare(&mut runtime, true);
        assert_eq!(
            (
                edited.rows_examined,
                edited.source_passes,
                edited.blur_passes,
                edited.composite_passes,
                edited.texture_allocations
            ),
            (1, 0, 0, 1, 0)
        );
        assert_eq!(fixture.last_image_upload, RasterImageUploadStats::default());

        let mut style = runtime.frame().objects[1].style;
        style.opacity = 0.37;
        runtime
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                fill_gradient: None,
                fill_image: None,
                object,
                style,
            })
            .unwrap();
        let faded = fixture.prepare(&mut runtime, true);
        assert_eq!(
            (
                faded.rows_examined,
                faded.source_passes,
                faded.blur_passes,
                faded.composite_passes,
                faded.texture_allocations
            ),
            (1, 0, 0, 1, 0)
        );
        assert_eq!(
            fixture.last_image_upload.textures_uploaded
                + fixture.last_image_upload.pixel_bytes_uploaded,
            0
        );
        assert_eq!(fixture.last_image_upload.instances_uploaded, 1);
        assert_eq!(fixture.renderer.image_residency_stats(), resident);

        runtime
            .apply_execution_patch(&ExecutionPatch::RemoveObject(object))
            .unwrap();
        let retired = fixture.prepare(&mut runtime, true);
        assert_eq!(retired.retained_texture_bytes, 0);
        assert_eq!(fixture.last_image_upload.textures_retired, 1);
        assert_eq!(fixture.renderer.image_residency_stats().textures, 0);
        assert!(!fixture.renderer.has_draw_source_glow(object));
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
fn glow_order_initially_effect_free_publication_prepares_later_local_attachment() {
    let mut fixture = Fixture::new();
    let mut scene = runtime(4096, false, false);
    let initial = fixture.prepare(&mut scene, true);
    assert_eq!(initial.rows_examined, 4096);
    assert_eq!(initial.sources_prepared, 0);
    // Initial publication owns the full inverse-order construction, including
    // an effect-free scene. A later local attachment must not need that scan.
    let publication = scene.take_renderer_publication();
    assert_eq!(
        fixture.renderer.retained_glow.painter_ranks.len(),
        publication.frame().objects.len()
    );
    for (rank, &index) in publication.painter_order().iter().enumerate() {
        assert_eq!(
            fixture.renderer.retained_glow.painter_ranks[index as usize],
            rank
        );
    }
    drop(publication);
    scene
        .apply_execution_patch(&ExecutionPatch::SetZIndex {
            object: ObjectId::new(101),
            value: -2.0,
        })
        .unwrap();
    assert_eq!(
        fixture.prepare(&mut scene, true),
        RetainedGlowStats::default()
    );
    scene
        .apply_execution_patch(&ExecutionPatch::SetGlowAttachment {
            object: ObjectId::new(101),
            expected: None,
            glow: Some(Arc::new(CompiledGlow {
                attachment: SemanticNodeId::new(31, 5),
                definition: Glow::new(GlowUpdate::default().radius(Pixels(3.0))).unwrap(),
            })),
        })
        .unwrap();
    assert!(scene.frame_changes().painter_order_range().is_none());
    let attached = fixture.prepare(&mut scene, true);
    assert_eq!((attached.rows_examined, attached.sources_prepared), (1, 1));
    assert_eq!(
        fixture.prepare(&mut scene, true),
        RetainedGlowStats {
            retained_texture_bytes: attached.retained_texture_bytes,
            ..Default::default()
        }
    );
}

#[test]
fn glow_order_reattachment_keeps_reorders_published_without_effects() {
    for mixed in [false, true] {
        let mut fixture = Fixture::new();
        let mut scene = runtime(3, true, mixed);
        fixture.prepare(&mut scene, true);
        let old = scene.frame().objects[1].glow.as_ref().unwrap().clone();
        scene
            .apply_execution_patch(&ExecutionPatch::SetGlowAttachment {
                object: ObjectId::new(101),
                expected: Some(old.attachment),
                glow: None,
            })
            .unwrap();
        let removed = fixture.prepare(&mut scene, true);
        assert_eq!(removed.rows_examined, 1);
        assert_eq!(removed.retained_texture_bytes, 0);
        assert!(!fixture.renderer.has_retained_analytic_glow_sources());
        scene
            .apply_execution_patch(&ExecutionPatch::SetZIndex {
                object: ObjectId::new(101),
                value: -2.0,
            })
            .unwrap();
        assert_eq!(
            fixture.prepare(&mut scene, true),
            RetainedGlowStats::default()
        );
        scene
            .apply_execution_patch(&ExecutionPatch::SetGlowAttachment {
                object: ObjectId::new(101),
                expected: None,
                glow: Some(Arc::new(CompiledGlow {
                    attachment: SemanticNodeId::new(31, 6),
                    definition: old.definition,
                })),
            })
            .unwrap();
        assert!(scene.frame_changes().painter_order_range().is_none());
        // Fixture::prepare checks the offscreen source's expanded visibility
        // against the current publication's exact painter order.
        let attached = fixture.prepare(&mut scene, true);
        assert_eq!((attached.rows_examined, attached.sources_prepared), (1, 1));
    }
}

#[test]
fn glow_order_attachment_to_object_added_without_effects_stays_local() {
    let mut fixture = Fixture::new();
    let mut scene = runtime(4096, true, false);
    fixture.prepare(&mut scene, true);
    let old = scene.frame().objects[1].glow.as_ref().unwrap().clone();
    scene
        .apply_execution_patch(&ExecutionPatch::SetGlowAttachment {
            object: ObjectId::new(101),
            expected: Some(old.attachment),
            glow: None,
        })
        .unwrap();
    fixture.prepare(&mut scene, true);
    let added = ObjectId::new(10_000);
    scene
        .apply_execution_patch(&ExecutionPatch::CreateObject(CompiledObject::new(
            added,
            GeometryRef::circle(0.4),
            Transform2D::IDENTITY,
            Style {
                fill: Some(Color::WHITE),
                stroke: None,
                ..Style::default()
            },
        )))
        .unwrap();
    let created = fixture.prepare(&mut scene, true);
    assert_eq!((created.rows_examined, created.sources_prepared), (1, 0));
    scene
        .apply_execution_patch(&ExecutionPatch::SetGlowAttachment {
            object: added,
            expected: None,
            glow: Some(Arc::new(CompiledGlow {
                attachment: SemanticNodeId::new(32, 5),
                definition: old.definition,
            })),
        })
        .unwrap();
    assert!(scene.frame_changes().painter_order_range().is_none());
    let attached = fixture.prepare(&mut scene, true);
    assert_eq!((attached.rows_examined, attached.sources_prepared), (1, 1));
    assert_eq!(
        fixture.prepare(&mut scene, true),
        RetainedGlowStats {
            retained_texture_bytes: attached.retained_texture_bytes,
            ..Default::default()
        }
    );
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
fn endpoint_drains_retain_glow_changes_until_a_clean_presented_publication() {
    let mut fixture = Fixture::new();
    let mut runtime = runtime(4096, true, false);
    let persistent = runtime.frame().objects[1].id;
    let temporary = runtime.frame().objects[2].id;
    let temporary_attachment = SemanticNodeId::new(32, 5);
    runtime
        .apply_execution_patch(&ExecutionPatch::SetGlowAttachment {
            object: temporary,
            expected: None,
            glow: Some(Arc::new(CompiledGlow {
                attachment: temporary_attachment,
                definition: Glow::new(GlowUpdate::default().radius(Pixels(3.0))).unwrap(),
            })),
        })
        .unwrap();
    let warm = fixture.prepare(&mut runtime, true);
    assert_eq!((warm.sources_prepared, warm.source_passes), (2, 2));
    assert_eq!(fixture.renderer.retained_glow.packed.len(), 2);

    let mut transform = runtime.frame().objects[1].transform;
    transform.translation = Vec2::new(-4.1, 0.4);
    runtime
        .apply_execution_patch(&ExecutionPatch::SetTransform {
            object: persistent,
            transform,
        })
        .unwrap();
    let mut glow = **runtime.frame().objects[1].glow.as_ref().unwrap();
    glow.definition = GlowUpdate::default()
        .radius(Pixels(4.0))
        .apply_to(glow.definition)
        .unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::SetGlow {
            object: persistent,
            glow: Arc::new(glow),
        })
        .unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::SetGlowAttachment {
            object: temporary,
            expected: Some(temporary_attachment),
            glow: None,
        })
        .unwrap();

    for (step, time) in [1.6, 2.0, 2.125].into_iter().enumerate() {
        runtime.advance_to(time).unwrap();
        let publication = runtime.take_renderer_publication();
        assert!(!publication.changes().is_all());
        assert_eq!(
            publication.changes().object_indices(),
            if step == 0 { &[1, 2][..] } else { &[] }
        );
        let visible = publication
            .painter_order()
            .iter()
            .map(|&index| index as usize)
            .filter(|&index| index != 1)
            .collect::<Vec<_>>();
        let expanded = fixture
            .renderer
            .glow_source_visibility(&publication, &visible)
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
        assert_eq!(fixture.renderer.retained_glow.dirty, BTreeSet::from([1, 2]));
        assert_eq!(fixture.renderer.retained_glow.sources.len(), 1);
        if step < 2 {
            // Endpoint drains retain publication dirtiness and ordinary GPU
            // geometry without recording an obsolete capture/filter workload.
            assert_eq!(fixture.renderer.retained_glow.packed.len(), 2);
            continue;
        }

        let mut encoder = fixture.device.create_command_encoder(&Default::default());
        let presented = fixture
            .renderer
            .prepare_retained_analytic_glows(
                &fixture.device,
                &fixture.queue,
                &mut encoder,
                &prepared,
                &fixture.text,
                &publication,
                1_000_000,
            )
            .unwrap();
        assert_eq!(
            (
                presented.rows_examined,
                presented.sources_prepared,
                presented.source_passes,
                presented.blur_passes,
                presented.composite_passes,
            ),
            (2, 1, 1, 2, 1)
        );
        assert_eq!(
            prepared.geometry.observe_object(1).unwrap().transform,
            transform.into()
        );
        let target = fixture.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("retained endpoint glow regression"),
            size: wgpu::Extent3d {
                width: 80,
                height: 60,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        // Encoding checks that the retained effect captured the exact latest
        // source instance, including the pose changed in the drained endpoint.
        fixture
            .renderer
            .encode_retained(
                &mut encoder,
                &target.create_view(&Default::default()),
                &prepared,
                &fixture.text,
                wgpu::Color::TRANSPARENT,
                None,
            )
            .unwrap();
        fixture.queue.submit([encoder.finish()]);
        assert_eq!(fixture.renderer.retained_glow.packed.len(), 1);
        assert!(fixture.renderer.retained_glow.packed.contains_key(&1));
        assert!(fixture.renderer.retained_glow.dirty.is_empty());
    }
    // Retiring the only remaining binding must release every scope: the
    // temporary attachment removed during the endpoint drain cannot leak.
    let packed = fixture.renderer.retained_glow.packed[&1].1;
    fixture.renderer.remove_analytic_glow(packed);
    assert_eq!(fixture.renderer.analytic_glow_texture_bytes(), 0);
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
            &fixture.text,
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
            &fixture.text,
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

#[test]
fn general_capture_reuses_masks_for_glow_and_opacity_edits_and_retires_locally() {
    for geometry in [
        GeometryRef::circle(0.4),
        GeometryRef::rectangle(0.8, 0.6),
        GeometryRef::line(Vec2::new(-0.7, -0.5), Vec2::new(0.7, 0.5)),
        GeometryRef::VectorPath(
            noon_core::VectorPath::new()
                .move_to(Vec2::new(-0.7, -0.5))
                .cubic_to(
                    Vec2::new(-0.4, 0.8),
                    Vec2::new(0.4, -0.8),
                    Vec2::new(0.7, 0.5),
                ),
        ),
    ] {
        let mut fixture = Fixture::new();
        let mut runtime = runtime(4096, true, false);
        let object = runtime.frame().objects[1].id;
        runtime
            .apply_execution_patch(&ExecutionPatch::SetContent {
                object,
                content: ObjectContentRef::Geometry(geometry),
                text_bounds: None,
            })
            .unwrap();
        runtime
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                fill_gradient: None,
                fill_image: None,
                object,
                style: Style {
                    fill: None,
                    stroke: Some(Color::WHITE),
                    stroke_width: 0.2,
                    ..Style::default()
                },
            })
            .unwrap();
        let first = fixture.prepare(&mut runtime, true);
        assert_eq!(
            (
                first.sources_prepared,
                first.source_passes,
                first.blur_passes
            ),
            (1, 1, 2)
        );
        assert_eq!(
            first.texture_allocations, 5,
            "source, MSAA, and three filter images"
        );
        assert!(fixture.renderer.has_draw_source_glow(object));
        let clean = fixture.prepare(&mut runtime, true);
        assert_eq!(
            clean.bytes_uploaded + clean.source_passes + clean.blur_passes + clean.composite_passes,
            0
        );
        let mut glow = **runtime.frame().objects[1].glow.as_ref().unwrap();
        glow.definition = GlowUpdate::default()
            .intensity(0.4)
            .apply_to(glow.definition)
            .unwrap();
        runtime
            .apply_execution_patch(&ExecutionPatch::SetGlow {
                object,
                glow: Arc::new(glow),
            })
            .unwrap();
        let edited = fixture.prepare(&mut runtime, true);
        assert_eq!(
            (
                edited.rows_examined,
                edited.sources_prepared,
                edited.source_passes,
                edited.blur_passes,
                edited.composite_passes,
                edited.bytes_uploaded,
                edited.texture_allocations
            ),
            (1, 1, 0, 0, 1, 32, 0)
        );
        let mut style = runtime.frame().objects[1].style;
        style.opacity = 0.35;
        runtime
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                fill_gradient: None,
                fill_image: None,
                object,
                style,
            })
            .unwrap();
        let faded = fixture.prepare(&mut runtime, true);
        assert_eq!(
            (
                faded.rows_examined,
                faded.source_passes,
                faded.blur_passes,
                faded.composite_passes,
                faded.texture_allocations
            ),
            (1, 0, 0, 1, 0)
        );
        assert_eq!(faded.retained_texture_bytes, first.retained_texture_bytes);
        runtime
            .apply_execution_patch(&ExecutionPatch::RemoveObject(object))
            .unwrap();
        assert_eq!(
            fixture.prepare(&mut runtime, true).retained_texture_bytes,
            0
        );
        assert!(!fixture.renderer.has_draw_source_glow(object));
    }
}
