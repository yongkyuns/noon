#![cfg(feature = "prepared-prefix")]
// M2d admission probes: real shared Rust Scene and completed source boundaries.
// Do not silently skip a rejected animation family or a missing text dependency.
use noon::{AnimationOptions, FadeEndpoint, FadeTranslation, RateFunction, Scene};
use noon_core::{SemanticFadeDirection, SemanticVec3};
use noon_runtime::ReplayLimits;

fn options(duration: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(duration)
        .rate_func(RateFunction::Linear)
}

#[test]
fn incrementally_prepared_fade_in_out_matches_first_pass() {
    let mut scene = Scene::new();
    let anchor = scene.circle(0.15).unwrap();
    let target = scene.square(1.0).unwrap();
    scene.add(&anchor).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 256,
            payloads: 8192,
        })
        .unwrap();
    let initial = source.wait_segment(1.0).unwrap();
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(initial).unwrap();
    reader.reserve_object_capacity(12).unwrap();

    let fade_in = scene
        .live(&mut source)
        .declare_and_activate_fade_with_endpoint(
            &target,
            SemanticFadeDirection::In,
            FadeEndpoint::new(
                0.5,
                FadeTranslation::Shift(SemanticVec3::new(0.5, 0.0, 0.0)),
            ),
            options(1.0),
        )
        .unwrap();
    source.advance_to(1.5).unwrap();
    let visible_in = source.frame().clone();
    {
        let mut live = scene.live(&mut source);
        live.advance_segment_to(fade_in, fade_in.end_time())
            .unwrap();
        live.complete_segment(fade_in).unwrap();
    }
    let batch = source
        .publish_prepared_prefix(fade_in, &mut publisher)
        .expect("completed FadeIn should prepare time-qualified lifecycle/driver changes");
    reader.append(batch).unwrap();
    assert_eq!(reader.seek(1.5).unwrap(), &visible_in);

    let fade_out = scene
        .live(&mut source)
        .declare_and_activate_fade_with_endpoint(
            &target,
            SemanticFadeDirection::Out,
            FadeEndpoint::new(
                0.25,
                FadeTranslation::Shift(SemanticVec3::new(1.0, 0.0, 0.0)),
            ),
            options(1.0),
        )
        .unwrap();
    source.advance_to(2.5).unwrap();
    let visible_out = source.frame().clone();
    {
        let mut live = scene.live(&mut source);
        live.advance_segment_to(fade_out, fade_out.end_time())
            .unwrap();
        live.complete_segment(fade_out).unwrap();
    }
    reader
        .append(
            source
                .publish_prepared_prefix(fade_out, &mut publisher)
                .expect(
                    "completed FadeOut should prepare historical membership and driver changes",
                ),
        )
        .unwrap();
    assert_eq!(reader.seek(2.5).unwrap(), &visible_out);
    assert_eq!(reader.seek(1.5).unwrap(), &visible_in);
}

#[test]
fn incrementally_prepared_morph_matches_first_pass() {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    let mut circle = scene.circle(0.7).unwrap();
    circle.shift(1.0, 0.0).unwrap();
    scene.add(&shape).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 256,
            payloads: 8192,
        })
        .unwrap();
    let initial = source.wait_segment(1.0).unwrap();
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(initial).unwrap();
    let segment = scene
        .live(&mut source)
        .declare_and_activate_transform_to(&shape, &circle, options(1.0))
        .unwrap();
    source.advance_to(1.4).unwrap();
    let expected = source.frame().clone();
    {
        let mut live = scene.live(&mut source);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }
    reader
        .append(
            source
                .publish_prepared_prefix(segment, &mut publisher)
                .expect("morph execution/reconciliation must preserve prepared geometry"),
        )
        .unwrap();
    assert_eq!(reader.seek(1.4).unwrap(), &expected);
}

#[cfg(feature = "native-text")]
#[test]
fn late_native_text_and_font_closure_survive_preparation() {
    use noon::Text;
    let mut scene = Scene::new();
    let anchor = scene.circle(0.15).unwrap();
    scene.add(&anchor).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 256,
            payloads: 8192,
        })
        .unwrap();
    let initial = source.wait_segment(1.0).unwrap();
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(initial).unwrap();
    reader.reserve_object_capacity(16).unwrap();
    let heading = scene
        .live(&mut source)
        .create_text(Text::new("Late text in Noon").with_font_size(32.0))
        .unwrap();
    scene.live(&mut source).add(&heading).unwrap();
    let handle = heading.state().unwrap().content.text().unwrap();
    let next = source.wait_segment(1.0).unwrap();
    source.advance_to(1.5).unwrap();
    let expected = source.frame().clone();
    source.advance_to(2.0).unwrap();
    let batch = source
        .publish_prepared_prefix(next, &mut publisher)
        .unwrap();
    assert!(
        batch.stats().resource_payloads > 0,
        "late text must transfer a resource closure"
    );
    reader.append(batch).unwrap();
    drop(heading);
    drop(source);
    drop(scene);
    reader.seek(1.5).unwrap();
    let publication = reader.take_renderer_publication();
    assert!(
        publication.text_resources().get(handle).is_some(),
        "late text handle must remain alive"
    );
    assert_eq!(publication.frame(), &expected);
}

#[cfg(feature = "native-text")]
mod raster_support;

#[cfg(feature = "native-text")]
#[test]
#[ignore = "requires actual Vulkan fallback adapter; invoked explicitly by M2d CI"]
fn late_text_reentry_raster_is_identical_after_preparation_and_source_destruction() {
    use noon::Text;
    pollster::block_on(async {
        let mut scene = Scene::new();
        let anchor = scene.circle(0.15).unwrap();
        scene.add(&anchor).unwrap();
        let mut source = scene.execution_session().unwrap();
        source
            .begin_replay_retention(ReplayLimits {
                revisions: 256,
                payloads: 8192,
            })
            .unwrap();
        let initial = source.wait_segment(1.0).unwrap();
        let mut source_raster = raster_support::Raster::new().await;
        source.advance_to(0.5).unwrap();
        let mut reference = vec![(
            0.5,
            source_raster.capture(&source.take_renderer_publication()),
        )];
        source.advance_to(1.0).unwrap();
        let (mut publisher, mut reader) = source.create_prepared_prefix(initial).unwrap();
        reader.reserve_object_capacity(16).unwrap();

        let heading = scene
            .live(&mut source)
            .create_text(Text::new("Noon prepared text").with_font_size(38.0))
            .unwrap();
        scene.live(&mut source).add(&heading).unwrap();
        for step in 1..=3 {
            if step == 2 {
                scene.live(&mut source).remove(&heading).unwrap();
            }
            if step == 3 {
                scene.live(&mut source).add(&heading).unwrap();
            }
            let boundary = source.wait_segment(1.0).unwrap();
            source.advance_to(step as f64 + 0.5).unwrap();
            reference.push((
                source.frame().time,
                source_raster.capture(&source.take_renderer_publication()),
            ));
            source.advance_to(boundary.end_time()).unwrap();
            let batch = source
                .publish_prepared_prefix(boundary, &mut publisher)
                .unwrap();
            reader.append(batch).unwrap();
        }
        assert_ne!(reference[0].1, reference[1].1, "late text is visible");
        assert_ne!(reference[1].1, reference[2].1, "removal is visible");
        assert_eq!(
            reference[1].1, reference[3].1,
            "reentry retains original font/geometry"
        );
        drop(heading);
        drop(source);
        drop(scene);
        drop(publisher);
        let mut playback_raster = raster_support::Raster::new().await;
        for _ in 0..3 {
            for (time, pixels) in reference.iter().rev() {
                reader.seek(*time).unwrap();
                assert_eq!(
                    &playback_raster.capture(&reader.take_renderer_publication()),
                    pixels
                );
            }
        }
        let mut fresh = raster_support::Raster::new().await;
        reader.seek(1.5).unwrap();
        assert_eq!(
            &fresh.capture(&reader.take_renderer_publication()),
            &reference[1].1
        );
        eprintln!("M2D_LATE_TEXT_RASTER: 13 exact comparisons passed across late text, removal/reentry, destroyed source, fresh renderer");
    });
}
