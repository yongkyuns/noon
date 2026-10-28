//! Analytic source pixels and independent 2D convolution at each nested stage.
use super::*;
use noon::AnimationCompositionRequest;

fn animated_reference(time: f64, removed: usize) -> Vec<u8> {
    let alpha = 128.0 / 255.0 * 0.125;
    let mut image = field(
        [27, 24, 43, 36],
        [
            192.0 / 255.0 * alpha,
            48.0 / 255.0 * alpha,
            16.0 / 255.0 * alpha,
            alpha,
        ],
    );
    if removed < 3 {
        image = expected_rgba_blur(&image, 1.0 + 2.0 * time, 1.0);
    }
    let mut inner = compose(&[image, field([33, 26, 47, 34], [0.0, 0.0, 0.375, 0.375])]);
    if removed < 2 {
        inner = expected_rgba_blur(&inner, 2.0 + 2.0 * time, 1.0);
    }
    let alpha = 192.0 / 255.0 * 0.75;
    let mut outer = compose(&[
        inner,
        field(
            [37, 24, 53, 36],
            [
                16.0 / 255.0 * alpha,
                192.0 / 255.0 * alpha,
                96.0 / 255.0 * alpha,
                alpha,
            ],
        ),
    ]);
    if removed < 1 {
        outer = expected_rgba_blur(&outer, 3.0 - 2.0 * time, 1.0);
    }
    let unrelated = field([8, 26, 12, 34], [0.18, 0.0, 0.36, 0.6]);
    compose(&[
        expected_rgba_blur(&unrelated, 1.0, 1.0),
        outer,
        field([37, 28, 43, 32], [0.6, 0.45, 0.075, 0.6]),
    ])
}

#[test]
#[ignore = "requires a real native raster adapter; animated nested source oracle"]
fn public_nested_family_animation_pixels_seek_complete_and_retire() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("animated composed group adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut scene = Scene::new();
    let mut images = Vec::new();
    for (i, rgba) in [[192, 48, 16, 128], [16, 192, 96, 192]]
        .into_iter()
        .enumerate()
    {
        let mut options = noon::ImageMobjectOptions::rgba8(1, 1, rgba).unwrap();
        options.set_height(1.0).unwrap();
        options.set_opacity(if i == 0 { 0.5 } else { 1.0 }).unwrap();
        options.set_sampling(noon::RasterImageSampling::Nearest);
        let mut image = scene.image(options).unwrap();
        image.scale(1.6, 1.2).unwrap();
        image
            .set_translation(if i == 0 { -0.5 } else { 0.5 }, 0.0)
            .unwrap();
        image
            .set_object_opacity(if i == 0 { 0.25 } else { 0.75 })
            .unwrap();
        images.push(image);
    }
    images[0]
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(1.0))).unwrap(),
            "soften",
        )
        .unwrap();
    let mut rectangle = scene.rectangle(1.4, 0.8).unwrap();
    rectangle.disable_stroke().unwrap();
    rectangle.set_fill(0.0, 0.0, 1.0, 0.5).unwrap();
    rectangle.set_object_opacity(0.75).unwrap();
    let mut inner = scene
        .family(&[(&images[0]).into(), (&rectangle).into()])
        .unwrap();
    inner
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "soften",
        )
        .unwrap();
    let mut outer = scene
        .family(&[(&inner).into(), (&images[1]).into()])
        .unwrap();
    outer
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(3.0))).unwrap(),
            "soften",
        )
        .unwrap();
    let mut unrelated = scene.rectangle(0.4, 0.8).unwrap();
    unrelated.disable_stroke().unwrap();
    unrelated.set_fill(0.3, 0.0, 0.6, 0.6).unwrap();
    unrelated.set_translation(-3.0, 0.0).unwrap();
    unrelated
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(1.0))).unwrap(),
            "outside",
        )
        .unwrap();
    let mut foreground = scene.rectangle(0.6, 0.4).unwrap();
    foreground.disable_stroke().unwrap();
    foreground.set_fill(1.0, 0.75, 0.125, 0.6).unwrap();
    foreground.set_z_index(1.0).unwrap();
    scene
        .add_many(&[(&unrelated).into(), (&outer).into(), (&foreground).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let target = scene.live(&mut session).copy_family(&outer).unwrap();
    let target_inner = target.family(&inner).unwrap();
    let target_image = target.mobject(&images[0]).unwrap();
    let targets: [(noon::MobjectTarget<'_>, f64); 3] = [
        ((&target_image).into(), 3.0),
        ((&target_inner).into(), 4.0),
        (target.root().into(), 1.0),
    ];
    for (owner, radius) in targets {
        scene
            .live(&mut session)
            .set_effect(
                owner,
                "soften",
                EffectUpdate::default().radius(Pixels(radius)),
            )
            .unwrap();
    }
    let outer_object = session.execution_object_id(outer.node_id()).unwrap();
    let source_plan = session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == outer_object)
        .unwrap()
        .content
        .composed_source()
        .unwrap()
        .clone();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(
            &AnimationCompositionRequest::FamilyMethodTargetTo {
                source: &outer,
                target_state: target.root(),
                options: AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let mut surface = Surface::new(&device, &queue);
    let mut seen = std::collections::BTreeMap::<u32, Vec<u8>>::new();
    let mut references = std::collections::BTreeMap::<u32, Vec<u8>>::new();
    let mut maximum = 0;
    let mut warmed_bytes = 0;
    let times = [0.0, 0.25, 0.5, 0.75, 1.0]
        .into_iter()
        .chain((0..32).flat_map(|_| [0.25, 0.75, 0.5, 1.0]));
    for (sample, time) in times.enumerate() {
        if sample < 5 {
            session.advance_segment_to(segment, time).unwrap();
        } else {
            // Absolute seek restores runtime base state and marks the restored
            // frame dirty. Local invalidation is checked on forward playback;
            // seek must reproduce those forward pixels exactly.
            session.seek(time).unwrap();
        }
        let actual = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
        let key = (time * 4.0) as u32;
        let expected = references
            .entry(key)
            .or_insert_with(|| animated_reference(time, 0));
        let error = actual
            .iter()
            .zip(expected.iter())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        maximum = maximum.max(error);
        assert!(
            error <= 2,
            "animated nested t={time}: max RGBA byte error {error}"
        );
        if let Some(previous) = seen.insert(key, actual.clone()) {
            assert_eq!(actual, previous, "same-time seek must be pixel exact");
        }
        assert!(surface.effect_stats.retained_texture_bytes <= 1_000_000);
        if sample == 4 {
            warmed_bytes = surface.effect_stats.retained_texture_bytes;
        } else if sample >= 5 {
            assert_eq!(
                surface.effect_stats.retained_texture_bytes, warmed_bytes,
                "nested resident storage must plateau across repeated seeks"
            );
            assert_eq!(
                surface.effect_stats.texture_allocations, 0,
                "warmed nested captures must reuse their texture capacity"
            );
        }
        if sample > 0 && sample < 5 {
            assert!(
                surface.effect_stats.sources_prepared <= 3,
                "unrelated effect source must stay clean"
            );
        }
        let plan = session
            .frame()
            .objects
            .iter()
            .find(|row| row.id == outer_object)
            .unwrap()
            .content
            .composed_source()
            .unwrap();
        assert_eq!(
            plan, &source_plan,
            "parameter animation preserves source identities"
        );
        eprintln!(
            "animated composed t={time}: max byte error {error}, stats={:?}",
            surface.effect_stats
        );
    }
    session.advance_segment_to(segment, 1.0).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    let removals: [(usize, noon::MobjectTarget<'_>); 3] = [
        (1, (&outer).into()),
        (2, (&inner).into()),
        (3, (&images[0]).into()),
    ];
    let mut last_removal = Vec::new();
    for (removed, owner) in removals {
        scene
            .live(&mut session)
            .remove_effect(owner, "soften")
            .unwrap();
        let actual = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
        let expected = animated_reference(1.0, removed);
        let error = actual
            .iter()
            .zip(expected)
            .map(|(a, b)| a.abs_diff(b))
            .max()
            .unwrap();
        maximum = maximum.max(error);
        assert!(
            error <= 2,
            "animated nested removal={removed}: max byte error {error}"
        );
        last_removal = actual;
    }
    let held = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
    assert_eq!(
        held, last_removal,
        "clean held frame preserves the exact published pixels"
    );
    assert_eq!(
        (
            surface.effect_stats.sources_prepared,
            surface.effect_stats.source_passes,
            surface.effect_stats.bytes_uploaded
        ),
        (0, 0, 0)
    );
    eprintln!("ANIMATED_COMPOSED_PASS samples=136 max_error={maximum} analytic_source=true nested_convolution=true seek_exact=true unrelated_forward_clean=true warmed_bytes={warmed_bytes} warmed_cycles=32 warmed_texture_allocations=0");
}
