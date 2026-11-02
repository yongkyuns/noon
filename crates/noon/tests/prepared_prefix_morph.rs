#![cfg(feature = "prepared-prefix")]
mod raster_support;
use noon::{AnimationOptions, ExecutionSession, PrefixReader, RateFunction, Scene};
use noon_core::{SemanticStyle, Vec2, VectorPath};
use noon_runtime::ReplayLimits;

fn history(paths: bool) -> (ExecutionSession, PrefixReader) {
    let mut scene = Scene::new();
    let curve = |height| {
        VectorPath::new().move_to(Vec2::new(-1.0, 0.0)).cubic_to(
            Vec2::new(-0.4, height),
            Vec2::new(0.4, -height),
            Vec2::new(1.0, 0.0),
        )
    };
    let (mut shape, mut first, mut last) = if paths {
        (
            scene.path(curve(1.0), SemanticStyle::default()).unwrap(),
            scene.path(curve(-1.5), SemanticStyle::default()).unwrap(),
            scene.path(curve(0.3), SemanticStyle::default()).unwrap(),
        )
    } else {
        (
            scene.square(1.0).unwrap(),
            scene.circle(0.7).unwrap(),
            scene.rectangle(2.0, 0.4).unwrap(),
        )
    };
    for object in [&mut shape, &mut first, &mut last] {
        if paths {
            object.set_stroke_color(1.0, 1.0, 1.0, 1.0).unwrap();
            object.set_stroke_width(0.12).unwrap();
        } else {
            object.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
            object.disable_stroke().unwrap();
        }
    }
    first.shift(1.2, 0.6).unwrap();
    last.shift(-1.0, -0.6).unwrap();
    scene.add(&shape).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(boundary).unwrap();
    reader.seek(0.5).unwrap();
    let baseline = reader.frame().clone();
    let publication = reader.publication_context();
    for target in [&first, &last] {
        let segment = scene
            .live(&mut source)
            .declare_and_activate_transform_to(
                &shape,
                target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let mut live = scene.live(&mut source);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        let batch = source
            .publish_prepared_prefix(segment, &mut publisher)
            .unwrap();
        reader.append(batch).unwrap();
        assert_eq!(
            reader.frame(),
            &baseline,
            "adopting a future morph cannot change the displayed past"
        );
        assert_eq!(reader.publication_context(), publication);
    }
    let boundary = source.wait_segment(0.5).unwrap();
    source.advance_to(boundary.end_time()).unwrap();
    source.seal_replay().unwrap();
    reader
        .append(
            source
                .finish_prepared_prefix(boundary, &mut publisher)
                .unwrap(),
        )
        .unwrap();
    (source, reader)
}

#[test]
fn incremental_morph_frames_match_replay_and_own_their_geometry_after_source_drop() {
    for paths in [false, true] {
        let (mut source, mut reader) = history(paths);
        let mut expected = Vec::new();
        for time in [0.5, 1.0, 1.25, 1.75, 2.0, 2.25, 2.75, 3.0, 3.5] {
            source.seek(time).unwrap();
            reader.seek(time).unwrap();
            assert_eq!(reader.frame(), source.frame(), "paths={paths}, time={time}");
            expected.push((time, source.frame().clone()));
        }
        drop(source);
        for (time, frame) in expected.iter().rev() {
            reader.seek(*time).unwrap();
            assert_eq!(reader.frame(), frame);
        }
    }
}

#[test]
#[ignore = "requires a native GPU; explicitly invoked by focused qualification"]
fn incremental_morph_pixels_match_replay_after_source_drop() {
    pollster::block_on(async {
        for paths in [false, true] {
            let (mut source, mut reader) = history(paths);
            let mut reference = raster_support::Raster::with_primary_dimensions(128, 128).await;
            let mut expected = Vec::new();
            for time in [0.5, 1.25, 1.75, 2.25, 2.75, 3.5] {
                source.seek(time).unwrap();
                let pixels = reference.capture(&source.take_renderer_publication());
                assert!(
                    pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .filter(|p| p[..3] != [0, 0, 0])
                        .count()
                        > 50,
                    "visible paths={paths}, time={time}"
                );
                expected.push((time, pixels));
            }
            assert!(
                expected[0].1 != expected[2].1 && expected[2].1 != expected[4].1,
                "both original morphs must visibly animate"
            );
            drop(source);
            drop(reference);
            let mut historical = raster_support::Raster::with_primary_dimensions(128, 128).await;
            for (time, pixels) in expected.iter().rev() {
                reader.seek(*time).unwrap();
                assert!(
                    &historical.capture(&reader.take_renderer_publication()) == pixels,
                    "paths={paths}, time={time}"
                );
            }
        }
    });
}
