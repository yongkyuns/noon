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

#[test]
#[ignore = "requires native GPU; paired creator programme and shared export clock"]
fn paired_scope_opacity_programme_animates_and_restores_through_native_export() {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon::RustHostCallbackTable;
    use noon_export::capture_frames;
    let mut program = noon::example_scenes::scope_opacity::program().unwrap();
    let mut options = CaptureOptions::new(640, 360);
    options.backends = if cfg!(target_os = "macos") {
        Backends::METAL
    } else {
        Backends::VULKAN
    };
    let times = [
        4.75, 5.5, 7.0, 8.25, 8.5, 8.75, 9.25, 9.75, 10.25, 11.5, 12.0, 13.25, 13.75,
    ];
    let mut samples = std::collections::BTreeMap::new();
    let summary = capture_frames(
        &mut program,
        &mut RustHostCallbackTable::new(),
        ExportFrameOptions {
            frame_rate: FrameRate::new(8, 1).unwrap(),
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 113,
            max_transitions_per_sample: 64,
            final_hold_seconds: 0.0,
        },
        options,
        |frame| {
            let time = frame.observation.requested_time;
            if times.contains(&time) {
                samples.insert((time * 8.0) as u32, frame.rgba.to_vec());
            }
            Ok::<_, std::convert::Infallible>(())
        },
    )
    .unwrap();
    assert_eq!(summary.sampling.frames, 112);
    assert_eq!(samples.len(), times.len());
    for (time, red, overlap) in [
        (5.5, 159, 80),
        (7.0, 96, 48),
        (8.25, 48, 24),
        (9.25, 96, 48),
    ] {
        let frame = &samples[&((time * 8.0) as u32)];
        for (x, expected) in [(260, [red, 0, 0, 255]), (320, [overlap, 0, overlap, 255])] {
            let actual = &frame[(180 * 640 + x) * 4..][..4];
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
                "time={time} x={x}: {actual:?} vs {expected:?}"
            );
        }
    }
    assert_eq!(
        samples[&66], samples[&70],
        "there-and-back samples agree exactly"
    );
    assert_eq!(
        samples[&38], samples[&78],
        "returning completion restores ordinary output"
    );
    assert_eq!(
        samples[&38], samples[&110],
        "mask and halo removal restore ordinary output"
    );
    assert_eq!(
        samples[&82], samples[&106],
        "Visibility in restores the entire parent halo"
    );
    let mut empty = [0_u8, 0, 0, 255].repeat(640 * 360);
    for y in 282..318 {
        for x in 62..98 {
            empty[(y * 640 + x) * 4..][..4].fill(255);
        }
    }
    for index in [68, 92, 96] {
        assert_eq!(
            samples[&index], empty,
            "empty child cannot leave ancestor coverage at frame {index}"
        );
    }
    eprintln!("ANIMATED_SCOPE_EXPORT_PASS frames=112 samples=13 literal_primary_overlap=true empty_exact=true returning_exact=true ancestor_visibility_exact=true final_restoration_exact=true");
}

#[test]
#[ignore = "requires native GPU; ordered/composed appearance restoration"]
fn paired_effect_restore_programme_preserves_exact_pixels_and_copy_independence() {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon::RustHostCallbackTable;
    use noon_export::capture_frames;
    let mut program = noon::example_scenes::effect_restore::program().unwrap();
    let mut options = CaptureOptions::new(640, 360);
    options.backends = if cfg!(target_os = "macos") {
        Backends::METAL
    } else {
        Backends::VULKAN
    };
    let samples_at = [2, 6, 10, 14, 18, 22, 26];
    let mut samples = std::collections::BTreeMap::new();
    let summary = capture_frames(
        &mut program,
        &mut RustHostCallbackTable::new(),
        ExportFrameOptions {
            frame_rate: FrameRate::new(8, 1).unwrap(),
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 29,
            max_transitions_per_sample: 64,
            final_hold_seconds: 0.0,
        },
        options,
        |frame| {
            let index = (frame.observation.requested_time * 8.0).round() as u32;
            if samples_at.contains(&index) {
                samples.insert(index, frame.rgba.to_vec());
            }
            Ok::<_, std::convert::Infallible>(())
        },
    )
    .unwrap();
    assert_eq!(summary.sampling.frames, 28);
    assert_eq!(samples.len(), 7);
    if let Some(directory) = std::env::var_os("NOON_EFFECT_RESTORE_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        for (index, pixels) in &samples {
            std::fs::write(directory.join(format!("frame-{index}.rgba")), pixels).unwrap();
        }
    }
    for index in [6, 10, 14, 18, 22, 26] {
        let mut maximum = 0;
        let mut changed = 0;
        let mut left = 0;
        let mut right = 0;
        for (pixel, (a, b)) in samples[&2]
            .as_chunks::<4>()
            .0
            .iter()
            .zip(samples[&index].as_chunks::<4>().0.iter())
            .enumerate()
        {
            if a != b {
                changed += 1;
                if pixel % 640 < 320 {
                    left += 1;
                } else {
                    right += 1;
                }
            }
            for channel in 0..4 {
                maximum = maximum.max(a[channel].abs_diff(b[channel]));
            }
        }
        eprintln!("restore frame={index} maximum={maximum} changed_pixels={changed} left={left} right={right}");
    }
    assert!(
        samples[&2] == samples[&14],
        "restore recovers the whole original frame"
    );
    assert!(
        samples[&2] == samples[&18],
        "editing the saved copy cannot mutate the restored scene"
    );
    for index in [6, 10, 22, 26] {
        assert!(
            samples[&2] != samples[&index],
            "changed/removed output must be observable at {index}"
        );
    }
    for (index, opacity) in [
        (2, 0.6),
        (6, 0.25),
        (10, 0.25),
        (14, 0.6),
        (18, 0.6),
        (22, 0.0),
        (26, 1.0),
    ] {
        for (x, alpha) in [(410_usize, 0.84_f64), (460, 0.6)] {
            let expected = (f64::from((255.0 * alpha).round() as u8) * opacity).round() as u8;
            let at = (180 * 640 + x) * 4;
            assert!(
                samples[&index][at..at + 3]
                    .iter()
                    .all(|value| value.abs_diff(expected) <= 1),
                "independent interior alpha at frame {index}, x={x}"
            );
        }
    }
    eprintln!("EFFECT_RESTORE_EXPORT_PASS frames=28 retained_samples=7 restoration_exact=true saved_copy_independent=true interior_alpha_bound=1");
}
