//! Semantic attachment + ordinary execution channels -> published GPU pixels.
//! This does not bypass/qualify the still-guarded public Scene host or worker path.
use super::analytic_scene::{expected, frame, render_plain, semantic_runtime, target, SIGMA, VIEW};
use super::pixels::{device, readback};
use crate::{Camera2D, FramePreparer, GpuRenderer, PublishedAnalyticGlowRequest};
use noon_compile::{ExecutionMutationTransaction, ExecutionPatch};
use noon_core::{
    Color, Glow, GlowSource, GlowUpdate, Pixels, RateFunction, TrackDefinition, TrackId,
    TrackTiming, Vec2,
};
use noon_runtime::SceneInstance;

fn append_channels(runtime: &mut SceneInstance, update: GlowUpdate) {
    let row = &runtime.frame().objects[1];
    let captured = row.glow.as_ref().unwrap();
    let tracks = captured
        .parameter_channels(update)
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(i, (property, values))| {
            ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(2 + i as u64),
                object: row.id,
                property,
                values,
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: Default::default(),
            })
        })
        .collect::<Vec<_>>();
    runtime
        .apply_execution_transaction(&ExecutionMutationTransaction::from_mutations(tracks))
        .unwrap();
}

fn expected_parameters(start: Glow, end: Glow, alpha: f64) -> Glow {
    // Literal endpoint arithmetic independent of runtime track selection or the
    // production parameter sampler. Use convex weights to retain f64 radii.
    let lerp = |a: f64, b: f64| {
        if alpha == 0.0 {
            a
        } else if alpha == 1.0 {
            b
        } else {
            a * (1.0 - alpha) + b * alpha
        }
    };
    let channel = |a: f32, b: f32| lerp(f64::from(a), f64::from(b)) as f32;
    let (a, b) = (start.color(), end.color());
    GlowUpdate::default()
        .radius(Pixels(lerp(start.radius().value(), end.radius().value())))
        .intensity(lerp(start.intensity(), end.intensity()))
        .color(Color::rgba(
            channel(a.red, b.red),
            channel(a.green, b.green),
            channel(a.blue, b.blue),
            channel(a.alpha, b.alpha),
        ))
        .apply_to(start)
        .unwrap()
}

#[test]
#[ignore = "requires raster adapter; animated shared runtime publication, not full Scene orchestration"]
fn animated_glow_publication_pixels() {
    let (device, queue) = device();
    for (rectangle, offscreen, transparent, intensity_only) in [
        (false, false, false, false),
        (true, false, false, false),
        (false, true, false, false),
        (true, true, true, false),
        (false, false, true, false),
        (false, false, false, true),
    ] {
        let start = Glow::new(
            GlowUpdate::default()
                .radius(Pixels(SIGMA))
                .intensity(2.4)
                .color(Color::rgba(0.95, 0.35, 0.8, 0.8))
                .source(if transparent {
                    GlowSource::Silhouette
                } else {
                    GlowSource::Painted
                }),
        )
        .unwrap();
        let update = if intensity_only {
            GlowUpdate::default().intensity(0.0)
        } else {
            GlowUpdate::default()
                .radius(Pixels(4.75))
                .intensity(0.7)
                .color(Color::rgba(0.15, 0.85, 0.25, 0.5))
        };
        let end = update.apply_to(start).unwrap();
        let mut runtime = semantic_runtime(&frame(rectangle, offscreen, transparent), start);
        if intensity_only {
            runtime
                .apply_execution_patch(&ExecutionPatch::RemoveTrack(TrackId::new(1)))
                .unwrap();
        }
        append_channels(&mut runtime, update);
        let attachment = runtime.frame().objects[1].glow.as_ref().unwrap().attachment;
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, VIEW[0], VIEW[1]);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
        );
        let mut preparer = FramePreparer::new();
        let output = target(&device, VIEW);
        let view = output.create_view(&Default::default());
        let request = PublishedAnalyticGlowRequest {
            object_index: 1,
            texture_budget_bytes: 1_000_000,
        };
        let mut first = None;
        for time in [0.0_f64, 0.37, 0.8, 1.0, 0.0] {
            runtime.evaluate(time).unwrap();
            let wanted = expected_parameters(start, end, time);
            let publication = runtime.take_renderer_publication();
            let actual = publication.frame().objects[1].glow.as_ref().unwrap();
            assert_eq!(
                actual.attachment, attachment,
                "animation cannot replace attachment identity"
            );
            assert_eq!(
                actual.definition, wanted,
                "runtime channels must match independent endpoint arithmetic"
            );
            let prepared = preparer.prepare_incremental(publication.frame(), publication.changes());
            renderer.upload(&device, &queue, &prepared);
            let mut encoder = device.create_command_encoder(&Default::default());
            let stats = renderer
                .prepare_published_analytic_glow(
                    &device,
                    &queue,
                    &mut encoder,
                    &prepared,
                    &publication,
                    request,
                )
                .unwrap();
            renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::TRANSPARENT);
            queue.submit([encoder.finish()]);
            let pixels = readback(&device, &queue, &output);
            let reference = expected(&device, &queue, publication.frame(), wanted);
            let error = pixels
                .iter()
                .zip(&reference)
                .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
                .max()
                .unwrap();
            assert!(
                error <= 2,
                "animated semantic publication pixel mismatch: {error}"
            );
            let ordinary = render_plain(&device, &queue, publication.frame(), VIEW);
            let signal = pixels
                .iter()
                .zip(&ordinary)
                .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
                .max()
                .unwrap();
            if wanted.is_neutral() {
                assert_eq!(
                    pixels, ordinary,
                    "animated zero intensity must reach bit-exact ordinary output"
                );
            } else {
                assert!(signal > 2, "animated glow cannot disappear");
            }
            if intensity_only && time > 0.0 && time < 1.0 {
                assert_eq!(stats.source_passes, 0);
                assert_eq!(stats.source_texture_allocations, 0);
                assert_eq!(stats.filter.texture_allocations, 0);
                assert_eq!(stats.filter.pipeline_compiles, 0);
                assert_eq!(stats.filter.kernel_builds, 0);
                assert_eq!(stats.filter.bytes_uploaded, 32);
                assert_eq!(stats.filter.blur_passes, 0);
                assert_eq!(stats.filter.composite_passes, 1);
            }
            eprintln!("animated rectangle={rectangle} offscreen={offscreen} silhouette={transparent} intensity_only={intensity_only} t={time} max_byte_error={error} halo_signal={signal} source_passes={} blur_passes={}", stats.source_passes, stats.filter.blur_passes);
            if time == 0.0 {
                if let Some(original) = &first {
                    assert_eq!(
                        &pixels, original,
                        "rewind must recover exact original pixels"
                    );
                } else {
                    first = Some(pixels);
                }
            }
        }
        runtime
            .apply_execution_patch(&ExecutionPatch::RemoveObject(runtime.frame().objects[1].id))
            .unwrap();
        let publication = runtime.take_renderer_publication();
        let prepared = preparer.prepare_incremental(publication.frame(), publication.changes());
        renderer.upload(&device, &queue, &prepared);
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer
            .prepare_published_analytic_glow(
                &device,
                &queue,
                &mut encoder,
                &prepared,
                &publication,
                request,
            )
            .unwrap();
        assert_eq!(
            renderer.analytic_glow_texture_bytes(),
            0,
            "removing animated source releases its retained textures"
        );
    }
}

#[test]
#[ignore = "requires raster adapter; prepared existing-value publication, not public Scene activation"]
fn live_glow_value_publication_pixels() {
    use noon_compile::CompiledGlow;
    use std::sync::Arc;
    let (device, queue) = device();
    for (rectangle, offscreen, transparent) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (true, true, true),
        (false, false, true),
    ] {
        let original = Glow::new(
            GlowUpdate::default()
                .radius(Pixels(SIGMA))
                .color(Color::rgba(0.95, 0.35, 0.8, 0.8))
                .intensity(2.4)
                .source(if transparent {
                    GlowSource::Silhouette
                } else {
                    GlowSource::Painted
                }),
        )
        .unwrap();
        let mut runtime = semantic_runtime(&frame(rectangle, offscreen, transparent), original);
        runtime
            .apply_execution_patch(&ExecutionPatch::RemoveTrack(TrackId::new(1)))
            .unwrap();
        let owner = runtime.frame().objects[1].id;
        let attachment = runtime.frame().objects[1].glow.as_ref().unwrap().attachment;
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, VIEW[0], VIEW[1]);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
        );
        let mut preparer = FramePreparer::new();
        let output = target(&device, VIEW);
        let view = output.create_view(&Default::default());
        let request = PublishedAnalyticGlowRequest {
            object_index: 1,
            texture_budget_bytes: 1_000_000,
        };
        let definitions = [
            original,
            GlowUpdate::default()
                .intensity(0.8)
                .apply_to(original)
                .unwrap(),
            GlowUpdate::default()
                .color(Color::rgba(0.2, 0.7, 0.9, 0.75))
                .radius(Pixels(4.75))
                .apply_to(original)
                .unwrap(),
            GlowUpdate::default()
                .intensity(0.0)
                .apply_to(original)
                .unwrap(),
            original,
        ];
        let mut first = None;
        for (step, &definition) in definitions.iter().enumerate() {
            // Staging and dropping a valid value proof never publishes a frame.
            let old = runtime.frame().clone();
            let context = runtime.publication_context();
            let transaction =
                ExecutionMutationTransaction::from_mutations([ExecutionPatch::SetGlow {
                    object: owner,
                    glow: Arc::new(CompiledGlow {
                        attachment,
                        definition,
                    }),
                }]);
            let prepare = |runtime: &SceneInstance| {
                runtime
                    .prepare_authored_value_publication(
                        &transaction,
                        context,
                        context.scene_revision().checked_next().unwrap(),
                    )
                    .unwrap()
                    .unwrap()
            };
            drop(prepare(&runtime));
            assert_eq!(runtime.frame(), &old);
            let proof = prepare(&runtime);
            runtime.commit_prepared_authored_value_publication(proof);
            let publication = runtime.take_renderer_publication();
            assert_eq!(
                publication.frame().objects[1]
                    .glow
                    .as_ref()
                    .unwrap()
                    .definition,
                definition
            );
            let prepared = preparer.prepare_incremental(publication.frame(), publication.changes());
            renderer.upload(&device, &queue, &prepared);
            let mut encoder = device.create_command_encoder(&Default::default());
            let stats = renderer
                .prepare_published_analytic_glow(
                    &device,
                    &queue,
                    &mut encoder,
                    &prepared,
                    &publication,
                    request,
                )
                .unwrap();
            renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::TRANSPARENT);
            queue.submit([encoder.finish()]);
            let pixels = readback(&device, &queue, &output);
            let reference = expected(&device, &queue, publication.frame(), definition);
            let error = pixels
                .iter()
                .zip(&reference)
                .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
                .max()
                .unwrap();
            assert!(
                error <= 2,
                "live existing-value publication pixel mismatch: {error}"
            );
            let ordinary = render_plain(&device, &queue, publication.frame(), VIEW);
            if definition.is_neutral() {
                assert_eq!(
                    pixels, ordinary,
                    "live neutral restores exact ordinary output"
                );
            } else {
                assert!(
                    pixels
                        .iter()
                        .zip(&ordinary)
                        .any(|(&a, &b)| (i32::from(a) - i32::from(b)).abs() > 2),
                    "live glow cannot disappear"
                );
            }
            if step == 1 {
                assert_eq!(stats.source_passes, 0);
                assert_eq!(stats.source_texture_allocations, 0);
                assert_eq!(stats.filter.texture_allocations, 0);
                assert_eq!(stats.filter.kernel_builds, 0);
                assert_eq!(stats.filter.pipeline_compiles, 0);
                assert_eq!(stats.filter.bytes_uploaded, 32);
                assert_eq!(stats.filter.blur_passes, 0);
                assert_eq!(stats.filter.composite_passes, 1);
            }
            if step == 0 {
                first = Some(pixels.clone());
            }
            if step == 4 {
                assert_eq!(
                    Some(&pixels),
                    first.as_ref(),
                    "restoring live base reproduces pixels"
                );
            }
            eprintln!("live rectangle={rectangle} offscreen={offscreen} silhouette={transparent} step={step} max_byte_error={error}");
        }
    }
}
