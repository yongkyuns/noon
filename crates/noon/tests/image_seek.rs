use noon::{AnimationOptions, ImageMobjectOptions, RateFunction, Scene};
use noon_core::SemanticFadeDirection;
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;

fn options() -> ImageMobjectOptions {
    ImageMobjectOptions::rgba8(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 255, 255,
        ],
    )
    .unwrap()
}

#[test]
fn image_center_preserves_translation_under_affine_bounds() -> TestResult {
    let mut scene = Scene::new();
    for angle in [0.1, 0.37, std::f64::consts::FRAC_PI_4, 1.2] {
        for translation in [(3.0, -1.0), (0.1, -0.1), (1.0e-10, -1.0e-10)] {
            let mut image = scene.image(options())?;
            image.scale(0.75, -1.25)?;
            image.rotate(angle)?;
            image.set_translation(translation.0, translation.1)?;
            assert_eq!(
                image.center()?,
                translation,
                "a centered image's affine bounds must preserve its translation at angle {angle}"
            );
        }
    }
    Ok(())
}

#[test]
fn image_seek_matches_forward_for_fades_and_affine_transform() -> TestResult {
    // All three animation kinds use the same public session evaluator. Seeking
    // must preserve the immutable pixel handle and reproduce each effective row.
    for kind in ["fade-in", "transform", "fade-out"] {
        let mut scene = Scene::new();
        let mut input = options();
        input.set_height(2.0)?;
        let image = scene.image(input)?;
        if kind != "fade-in" {
            scene.add(&image)?;
        }
        let resource = image.state()?.content.image().unwrap().resource();
        let mut session = scene.execution_session()?;
        let animation_options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear);
        let segment = match kind {
            "fade-in" => scene.live(&mut session).declare_and_activate_fade(
                &image,
                SemanticFadeDirection::In,
                animation_options,
            )?,
            "fade-out" => scene.live(&mut session).declare_and_activate_fade(
                &image,
                SemanticFadeDirection::Out,
                animation_options,
            )?,
            "transform" => {
                let target = scene.live(&mut session).target_editor(&image)?;
                let mut live = scene.live(&mut session);
                live.set_translation(&target, 3.0, -1.0)?;
                live.rotate(&target, std::f64::consts::FRAC_PI_4)?;
                live.scale(&target, 0.75, 1.25)?;
                live.set_opacity(&target, 0.4)?;
                live.declare_and_activate_transform_to(&image, &target, animation_options)?
            }
            _ => unreachable!(),
        };
        let times = [0.0, 0.125, 0.5, 1.0, 1.875, 2.0];
        let mut expected = Vec::new();
        for time in times {
            expected.push(session.advance_to(time)?.clone());
        }
        for index in [5, 2, 0, 4, 1, 3, 5] {
            let frame = session.seek(times[index])?;
            assert_eq!(frame, &expected[index], "{kind} at {}", times[index]);
            assert_eq!(
                frame.objects[0].content.image().unwrap().resource(),
                resource
            );
        }
        let mut live = scene.live(&mut session);
        live.advance_segment_to(segment, segment.end_time())?;
        live.complete_segment(segment)?;
        assert_eq!(live.contains(&image)?, kind != "fade-out");
        if kind == "transform" {
            assert_eq!(image.center()?, (3.0, -1.0));
            assert_eq!(image.fill_opacity()?, 0.4);
        }
        assert_eq!(image.state()?.content.image().unwrap().resource(), resource);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .raster_image_resources()
                .len(),
            1
        );
    }
    Ok(())
}
