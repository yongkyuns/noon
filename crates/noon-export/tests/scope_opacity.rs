//! Final family opacity over literal overlapping coverage, through the normal
//! native Scene/LiveSession and SessionCapture path, without a scene transport.
#![cfg(not(target_arch = "wasm32"))]

use noon::{
    effects::{Blur, EffectUpdate, Pixels},
    Scene,
};
use noon_core::SemanticMutationTransaction;
use noon_export::{Backends, CaptureOptions, SessionCapture};

fn reference(opacity: f64, outer: f64, blur: bool) -> Vec<u8> {
    let mut source = vec![0_u8; 80 * 60 * 4];
    for y in 20..40 {
        for x in 25..55 {
            let rgba = if x < 35 {
                [255, 0, 0, 255]
            } else if x < 45 {
                [128, 0, 128, 255]
            } else {
                [0, 0, 128, 128]
            };
            source[(y * 80 + x) * 4..][..4].copy_from_slice(&rgba);
        }
    }
    if blur {
        // A full two-dimensional convolution, independent of engine kernels,
        // separable passes, sampling uniforms and projected bounds.
        let taps: Vec<_> = (-3_i32..=3)
            .flat_map(|y| {
                (-3_i32..=3).map(move |x| (x, y, (-f64::from(x * x + y * y) / 2.0).exp()))
            })
            .collect();
        let mass: f64 = taps.iter().map(|tap| tap.2).sum();
        let mut filtered = vec![0; source.len()];
        for y in 0..60_i32 {
            for x in 0..80_i32 {
                for channel in 0..4 {
                    let mut value = 0.0;
                    for &(dx, dy, weight) in &taps {
                        let (xx, yy) = (x + dx, y + dy);
                        if (0..80).contains(&xx) && (0..60).contains(&yy) {
                            value +=
                                f64::from(source[(yy as usize * 80 + xx as usize) * 4 + channel])
                                    * weight
                                    / mass;
                        }
                    }
                    filtered[(y as usize * 80 + x as usize) * 4 + channel] = value.round() as u8;
                }
            }
        }
        source = filtered;
    }
    let mut output = vec![0; source.len()];
    for (source, target) in source
        .as_chunks::<4>()
        .0
        .iter()
        .zip(output.as_chunks_mut::<4>().0.iter_mut())
    {
        let mut rgba = std::array::from_fn::<_, 4, _>(|c| f64::from(source[c]) / 255.0 * opacity);
        if outer != 1.0 {
            // The inner placement is sampled once into its parent's RGBA8
            // source; the parent multiplier applies after that composition.
            rgba = rgba.map(|v| (v * 255.0).round() / 255.0 * outer);
        }
        for c in 0..4 {
            let background = [24.0, 48.0, 64.0, 255.0][c];
            target[c] = (rgba[c] * 255.0 + background * (1.0 - rgba[3]))
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    output
}

#[test]
#[ignore = "requires native GPU; independent composed-family final-opacity oracle"]
fn native_family_opacity_applies_once_across_empty_neutral_filtered_nested_and_removed_stacks() {
    let mut captures = 0;
    let mut maximum = 0;
    for nested in [false, true] {
        for mode in 0..3 {
            let mut scene = Scene::new();
            scene.camera_frame().unwrap().set_scale(0.75, 0.75).unwrap();
            let mut a = scene.rectangle(2.0, 2.0).unwrap();
            a.disable_stroke().unwrap();
            a.set_fill(1.0, 0.0, 0.0, 1.0).unwrap();
            a.set_translation(-0.5, 0.0).unwrap();
            let mut b = scene.rectangle(2.0, 2.0).unwrap();
            b.disable_stroke().unwrap();
            b.set_fill(0.0, 0.0, 1.0, 0.5).unwrap();
            b.set_translation(0.5, 0.0).unwrap();
            let mut group = scene.family(&[(&a).into(), (&b).into()]).unwrap();
            group.set_object_opacity(0.5).unwrap();
            if mode > 0 {
                group
                    .add_effect(
                        Blur::new(EffectUpdate::default().radius(Pixels(if mode == 2 {
                            1.0
                        } else {
                            0.0
                        })))
                        .unwrap(),
                        "soften",
                    )
                    .unwrap();
            }
            let outer = if nested { 0.6 } else { 1.0 };
            if nested {
                let parent = scene.family(&[(&group).into()]).unwrap();
                parent.set_object_opacity(outer).unwrap();
                scene.add_many(&[(&parent).into()]).unwrap();
            } else {
                scene.add_many(&[(&group).into()]).unwrap();
            }
            let mut session = scene.execution_session().unwrap();
            let mut options = CaptureOptions::new(80, 60);
            options.backends = if cfg!(target_os = "macos") {
                Backends::METAL
            } else {
                Backends::VULKAN
            };
            options.background = [24.0 / 255.0, 48.0 / 255.0, 64.0 / 255.0];
            let mut capture = SessionCapture::new(&session, options).unwrap();
            for opacity in [0.5, 0.25, 0.0, 1.0, 0.75, 0.5] {
                scene
                    .live(&mut session)
                    .set_family_object_opacity(&group, opacity)
                    .unwrap();
                let expected = reference(opacity, outer, mode == 2);
                let rgba = capture.capture(&mut session).unwrap().rgba.to_vec();
                let error = rgba
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                maximum = maximum.max(error);
                if error > 2 {
                    eprintln!(
                        "opacity diagnostic red={:?} blue={:?} rows={:?}",
                        &rgba[(30 * 80 + 30) * 4..][..4],
                        &rgba[(30 * 80 + 50) * 4..][..4],
                        session
                            .frame()
                            .objects
                            .iter()
                            .enumerate()
                            .map(|(i, row)| (
                                row.id,
                                session.frame().is_present(i),
                                session.frame().is_drawable(i),
                                row.style.opacity,
                                row.content.composed_source().is_some()
                            ))
                            .collect::<Vec<_>>()
                    );
                }
                assert!(
                    error <= 2,
                    "nested={nested} mode={mode} opacity={opacity}: byte error {error}"
                );
                let held = capture.capture(&mut session).unwrap();
                assert_eq!(held.rgba, rgba);
                assert_eq!(held.receipt.work.geometry_instances_repacked, 0);
                assert_eq!(held.receipt.work.retained_upload_bytes, 0);
                captures += 2;
            }
            if mode > 0 {
                let mut tx = SemanticMutationTransaction::new();
                tx.remove_node(group.get_effect("soften").unwrap().node_id());
                scene.live(&mut session).apply(tx).unwrap();
                let frame = capture.capture(&mut session).unwrap();
                let expected = reference(0.5, outer, false);
                assert!(
                    frame
                        .rgba
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| a.abs_diff(b) <= 2),
                    "last effect removal must retain composite opacity"
                );
                captures += 1;
            }
        }
    }
    eprintln!("COMPOSED_OPACITY_PASS profiles=6 captures={captures} maximum_byte_error={maximum} independent_overlap_and_2d_convolution=true");
}

#[test]
#[ignore = "requires native GPU; private visibility must clip ancestor original coverage"]
fn dissolve_of_nested_family_removes_its_ancestor_silhouette_contribution() {
    use noon::effects::{DissolveDirection, DissolveStyle, Glow, GlowSource};
    for composed in [false, true] {
        let mut scene = Scene::new();
        scene.camera_frame().unwrap().set_scale(0.75, 0.75).unwrap();
        let mut a = scene.rectangle(2.0, 2.0).unwrap();
        a.disable_stroke().unwrap();
        a.set_fill(1.0, 0.0, 0.0, 1.0).unwrap();
        a.set_translation(-0.5, 0.0).unwrap();
        let mut b = scene.rectangle(2.0, 2.0).unwrap();
        b.disable_stroke().unwrap();
        b.set_fill(0.0, 0.0, 1.0, 0.5).unwrap();
        b.set_translation(0.5, 0.0).unwrap();
        let mut child = scene.family(&[(&a).into(), (&b).into()]).unwrap();
        child.set_object_opacity(0.5).unwrap();
        child
            .add_effect(
                Blur::new(EffectUpdate::default().radius(Pixels(1.0))).unwrap(),
                "soften",
            )
            .unwrap();
        if !composed {
            a.add_effect(
                Blur::new(EffectUpdate::default().radius(Pixels(1.0))).unwrap(),
                "soften",
            )
            .unwrap();
        }
        let target: noon::MobjectTarget<'_> = if composed {
            (&child).into()
        } else {
            (&a).into()
        };
        let mut parent = scene.family(&[target]).unwrap();
        parent
            .add_effect(
                Glow::new(
                    EffectUpdate::default()
                        .radius(Pixels(2.0))
                        .source(GlowSource::Silhouette),
                )
                .unwrap(),
                "halo",
            )
            .unwrap();
        scene.add_many(&[(&parent).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut options = CaptureOptions::new(80, 60);
        options.backends = if cfg!(target_os = "macos") {
            Backends::METAL
        } else {
            Backends::VULKAN
        };
        options.background = [24.0 / 255.0, 48.0 / 255.0, 64.0 / 255.0];
        let mut capture = SessionCapture::new(&session, options).unwrap();
        let baseline = capture.capture(&mut session).unwrap().rgba.to_vec();
        let empty = [24_u8, 48, 64, 255].repeat(80 * 60);
        assert!(baseline
            .iter()
            .zip(&empty)
            .any(|(a, b)| a.abs_diff(*b) > 20));
        let request = noon::AnimationCompositionRequest::Dissolve {
            target,
            direction: DissolveDirection::Out,
            style: DissolveStyle::new(EffectUpdate::default().seed(7).softness(0.2)).unwrap(),
            options: noon::AnimationOptions::new()
                .run_time(1.0)
                .rate_func(noon::RateFunction::Linear)
                .remover(false),
        };
        let segment = scene
            .live(&mut session)
            .declare_and_activate_composition(&request, noon::AnimationOptions::new())
            .unwrap();
        session.seek(segment.start_time() + 1.0).unwrap();
        let output = capture.capture(&mut session).unwrap().rgba.to_vec();
        let error = output
            .iter()
            .zip(&empty)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert_eq!(
            error, 0,
            "a fully dissolved child must not leave an ancestor halo"
        );
        session.seek(segment.start_time()).unwrap();
        assert_eq!(capture.capture(&mut session).unwrap().rgba, baseline);
        session.seek(segment.start_time() + 1.0).unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert_eq!(capture.capture(&mut session).unwrap().rgba, empty);
    }
}
