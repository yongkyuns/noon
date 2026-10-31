//! Private Visibility must mask both paint and original replacement coverage.
use super::*;
use noon_core::NoiseDissolve;
const MASK: SemanticNodeId = SemanticNodeId::new(84, 2);
fn stack(progress: f64) -> Arc<CompiledEffectStack> {
    let blur = panel_effect(2.0).iter().next().unwrap();
    Arc::new(
        CompiledEffectStack::new_with_visibility(
            [
                blur,
                CompiledEffect {
                    attachment: MASK,
                    definition: NoiseDissolve::new(7, 0.2)
                        .unwrap()
                        .with_progress(progress)
                        .unwrap()
                        .into(),
                },
            ],
            Some(MASK),
        )
        .unwrap(),
    )
}
fn mask(pixels: &[u8], progress: f64) -> Vec<u8> {
    crate::gpu::sample_filter::tests::reference(
        pixels,
        crate::SampleParameters {
            shader: None,
            definition: NoiseDissolve::new(7, 0.2)
                .unwrap()
                .with_progress(progress)
                .unwrap()
                .into(),
            output_height: VIEW[1],
            world_view_height: 6.0,
            // Authored 4x2 rectangle centered at world origin in the 8x6 view.
            // These are original source bounds, not a GPU tile or capture readback.
            source_bounds: crate::EffectPixelBounds {
                min: [20.0, 20.0],
                max: [60.0, 40.0],
            },
            scope_opacity: 1.0,
            scratch_budget_bytes: u64::MAX,
        },
        VIEW,
    )
}
#[test]
#[ignore = "requires a real raster adapter; private visibility coverage, exact endpoints and budget recovery"]
fn backdrop_private_visibility_clips_paint_and_coverage_without_repainting_prefix() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let clear = wgpu::Color {
        r: 24.0 / 255.0,
        g: 48.0 / 255.0,
        b: 64.0 / 255.0,
        a: 128.0 / 255.0,
    };
    let mut objects = rows(Some(2.0), 0.6);
    objects[1].content = GeometryRef::rectangle(4.0, 2.0).into();
    objects[1].base_transform = Transform2D::IDENTITY;
    objects[1].effects = Some(stack(0.0));
    let prefix = ordinary(&device, &queue, vec![objects[0].clone()], clear);
    let mut target = objects[1].clone();
    target.effects = None;
    target.base_style.opacity = 1.0;
    let paint = ordinary(
        &device,
        &queue,
        vec![target.clone()],
        wgpu::Color::TRANSPARENT,
    );
    target.base_style.fill = Some(Color::WHITE);
    let coverage = ordinary(&device, &queue, vec![target], wgpu::Color::TRANSPARENT);
    let mut scene = runtime(objects);
    let mut fixture = Fixture::new(&device, &queue);
    let mut first = Vec::new();
    let mut maximum = 0;
    let mut plateau = 0;
    let mut negative = 0;
    for (step, progress) in [0.0, 0.25, 0.5, 0.75, 1.0, 0.5, 0.0]
        .into_iter()
        .enumerate()
    {
        if step > 0 {
            scene
                .apply_execution_patch(&ExecutionPatch::SetEffects {
                    object: ObjectId::new(20),
                    effects: stack(progress),
                })
                .unwrap();
        }
        let work = fixture.render(&device, &queue, &mut scene, clear);
        let actual = read(&device, &queue, &fixture.target);
        let p = mask(&paint, progress);
        let m = mask(&coverage, progress);
        let reference = expected(&prefix, &p, &m, 2.0, 0.6);
        let error = actual
            .iter()
            .zip(&reference)
            .map(|(&a, &b)| a.abs_diff(b))
            .max()
            .unwrap();
        maximum = maximum.max(error);
        retain(
            &format!("private-visibility-{step}"),
            &actual,
            &reference,
            &[("prefix", &prefix), ("paint", &p), ("coverage", &m)],
            format!(
                r#"{{"width":80,"height":60,"bound":2,"progress":{progress},"seed":7,"softness":0.2,"profile":"private Visibility masks both backdrop paint and original coverage"}}"#
            ),
        );
        assert!(error <= 2, "visibility step {step}: maximum error {error}");
        if step == 0 {
            first = actual.clone();
        }
        if step == 1 {
            plateau = work.retained_texture_bytes;
        }
        if step > 1 {
            assert_eq!(work.texture_allocations, 0);
            assert_eq!(work.retained_texture_bytes, plateau);
        }
        if step > 0 {
            assert_eq!(
                work.source_passes, 0,
                "mask edits reuse prefix and original captures"
            );
        }
        if progress == 1.0 {
            assert_eq!(
                actual, prefix,
                "full visibility removal restores the untouched prefix exactly"
            );
        }
        if step == 6 {
            assert_eq!(
                actual, first,
                "neutral visibility restores the exact initial backdrop"
            );
        }
        if step == 2 {
            let wrong = expected(&prefix, &p, &coverage, 2.0, 0.6);
            negative = actual
                .iter()
                .zip(&wrong)
                .map(|(&a, &b)| a.abs_diff(b))
                .max()
                .unwrap();
            assert!(
                negative > 2,
                "specification control that masks only paint must fail the unchanged bound"
            );
        }
        let clean = fixture.render(&device, &queue, &mut scene, clear);
        assert_eq!(
            (
                clean.source_passes,
                clean.blur_passes,
                clean.composite_passes,
                clean.texture_allocations
            ),
            (0, 0, 0, 0)
        );
    }
    let bytes = fixture.renderer.effect_capture_texture_bytes();
    let rejected = fixture.render_budget(&device, &queue, &mut scene, clear, bytes - 1);
    assert_eq!(rejected, Err(EffectPrepareError::ScratchBudgetExceeded));
    assert_eq!(fixture.renderer.effect_capture_texture_bytes(), bytes);
    assert_eq!(
        read(&device, &queue, &fixture.target),
        first,
        "failed budget must not submit a partial frame"
    );
    let recovered = fixture.render(&device, &queue, &mut scene, clear);
    assert_eq!(recovered.texture_allocations, 0);
    assert_eq!(recovered.retained_texture_bytes, bytes);
    assert_eq!(read(&device, &queue, &fixture.target), first);
    eprintln!("backdrop visibility: 7 profiles max error {maximum}; full endpoint and restoration exact; paint-only-mask specification control error {negative}; clean reduced-budget rejection and storage-reusing recovery");
}
