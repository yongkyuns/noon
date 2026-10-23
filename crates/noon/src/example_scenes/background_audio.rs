//! Finite authored animation and original background music on one scene timeline.
//!
//! Native Rust and direct Rust/WASM lower the same initial scene. The browser
//! example supplies device output and playback controls; this builder owns no
//! device, wall clock, or transport. Python worker audio remains gated by #2263.

pub mod music;

use crate::{
    AnimationOptions, AudioClipOptions, AudioResource, DeclaredAnimation, RateFunction, Scene,
    SectionType, SemanticAnimationCompositionKind, SemanticVec3,
    DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
};
use noon_core::{
    CompositionTimeMap, SemanticAnimationIntent, SemanticMutationTransaction,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, TrackTiming,
};

pub const DURATION_SECONDS: f64 = 30.0;
pub const SAMPLE_RATE: u32 = 24_000;

/// A warm point visits three vertices while a quiet original score accompanies it.
///
/// The sections at 0, 10 and 20 seconds identify the same authored times used by
/// the animation and audio. Lower with
/// `scene.execution_session_with_animation_root(&animation)`; keeping the Scene
/// available also retains its section-navigation metadata. Music is generated
/// once during preparation, never on a frame, seek, or audio-device callback.
pub fn scene() -> Result<(Scene, DeclaredAnimation), String> {
    let pcm = music::render_music(SAMPLE_RATE).map_err(|error| error.to_string())?;
    let resource = AudioResource::from_interleaved(
        pcm.sample_rate(),
        music::CHANNELS as u16,
        pcm.into_samples(),
        DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
    )
    .map_err(|error| error.to_string())?;
    scene_with_music(resource)
}

fn scene_with_music(resource: AudioResource) -> Result<(Scene, DeclaredAnimation), String> {
    let mut scene = Scene::new();
    let vertices = [(-2.7, -1.5), (0.0, 1.8), (2.7, -1.5)];
    for (start, end) in vertices
        .iter()
        .copied()
        .zip(vertices.iter().copied().cycle().skip(1))
    {
        let mut edge = scene.line(start, end).map_err(|error| error.to_string())?;
        edge.set_color(0.28, 0.42, 0.50, 1.0)
            .map_err(|error| error.to_string())?;
        edge.set_stroke_width(0.035)
            .map_err(|error| error.to_string())?;
        scene.add(&edge).map_err(|error| error.to_string())?;
    }
    for (x, y) in vertices {
        let mut marker = scene.circle(0.08).map_err(|error| error.to_string())?;
        marker
            .set_translation(x, y)
            .map_err(|error| error.to_string())?;
        marker
            .set_fill(0.42, 0.65, 0.70, 1.0)
            .map_err(|error| error.to_string())?;
        marker
            .set_stroke_width(0.0)
            .map_err(|error| error.to_string())?;
        scene.add(&marker).map_err(|error| error.to_string())?;
    }
    let mut square = scene.square(1.0).map_err(|error| error.to_string())?;
    square
        .set_color(0.42, 0.65, 0.70, 1.0)
        .map_err(|error| error.to_string())?;
    square
        .set_fill(0.18, 0.28, 0.34, 0.3)
        .map_err(|error| error.to_string())?;
    square
        .set_stroke_width(0.035)
        .map_err(|error| error.to_string())?;
    scene.add(&square).map_err(|error| error.to_string())?;
    let mut point = scene.circle(0.26).map_err(|error| error.to_string())?;
    point
        .set_translation(vertices[0].0, vertices[0].1)
        .map_err(|error| error.to_string())?;
    point
        .set_fill(0.95, 0.76, 0.40, 1.0)
        .map_err(|error| error.to_string())?;
    point
        .set_stroke_width(0.0)
        .map_err(|error| error.to_string())?;
    scene.add(&point).map_err(|error| error.to_string())?;

    let mut transaction = SemanticMutationTransaction::new();
    let mut tracks = Vec::with_capacity(4);
    for (index, (start, end)) in vertices
        .iter()
        .copied()
        .zip(vertices.iter().copied().cycle().skip(1))
        .enumerate()
    {
        tracks.push(transaction.create_object_property_track(
            point.node_id(),
            SemanticObjectTrackProperty::Position,
            SemanticObjectTrackValues::Vec3 {
                from: SemanticVec3::new(start.0, start.1, 0.0),
                to: SemanticVec3::new(end.0, end.1, 0.0),
            },
            TrackTiming::new(index as f64 * 10.0, 10.0, RateFunction::Smooth),
            CompositionTimeMap::identity(),
        ));
    }
    tracks.push(transaction.create_object_property_track(
        square.node_id(),
        SemanticObjectTrackProperty::Rotation,
        SemanticObjectTrackValues::Scalar {
            from: 0.0,
            to: std::f64::consts::TAU,
        },
        TrackTiming::new(0.0, DURATION_SECONDS, RateFunction::Linear),
        CompositionTimeMap::identity(),
    ));
    let result = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .map_err(|error| error.to_string())?;
    let animation = scene.declare_animation(
        SemanticAnimationIntent::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            children: tracks
                .into_iter()
                .map(|token| result.resolve(token).expect("committed authored track"))
                .collect(),
        },
        AnimationOptions::new(),
    )?;

    scene
        .audio(
            resource,
            AudioClipOptions {
                duration: Some(DURATION_SECONDS),
                gain: 0.5,
                fade_in: 1.5,
                fade_out: 2.5,
                ..AudioClipOptions::default()
            },
        )
        .map_err(|error| error.to_string())?;
    for (name, time) in [("Begin", 0.0), ("Discover", 10.0), ("Resolve", 20.0)] {
        scene.next_section_at(name, SectionType::Normal, time)?;
    }
    scene.wait(10.0)?;
    Ok((scene, animation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SectionNavigation;

    #[test]
    fn sections_animation_and_music_share_the_same_finite_authored_interval() {
        // Use small constant PCM here; synthesis has separate deterministic tests.
        let resource = AudioResource::from_interleaved(
            8_000,
            1,
            vec![0.1; 30 * 8_000],
            DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
        )
        .unwrap();
        let (scene, animation) = scene_with_music(resource).unwrap();
        assert_eq!(scene.time(), DURATION_SECONDS);
        assert_eq!(
            scene
                .sections()
                .iter()
                .map(|section| section.time)
                .collect::<Vec<_>>(),
            [0.0, 10.0, 20.0]
        );
        assert_eq!(
            scene
                .section_target(0.0, SectionNavigation::Next)
                .unwrap()
                .unwrap()
                .1
                .time,
            10.0
        );
        let mut session = scene
            .execution_session_with_animation_root(&animation)
            .unwrap();
        let clip = {
            let store = scene.integration_store().borrow();
            let clip = store
                .semantic_scoped_audio_clips(scene.root())
                .unwrap()
                .next()
                .unwrap();
            *store.semantic_audio_clip_state(clip).unwrap().definition()
        };
        assert_eq!(clip.end_time(), DURATION_SECONDS);
        assert_eq!(clip.gain(), 0.5);
        assert_eq!(clip.state_at(0.0).unwrap().unwrap().gain, 0.0);
        assert_eq!(clip.state_at(DURATION_SECONDS).unwrap(), None);
        for (time, expected) in [
            (20.0, (2.7_f32, -1.5_f32)),
            (0.0, (-2.7, -1.5)),
            (10.0, (0.0, 1.8)),
            (30.0, (-2.7, -1.5)),
        ] {
            session.seek(time).unwrap();
            let point = session.frame().objects.last().unwrap();
            assert!((point.transform.translation.x - expected.0).abs() < 1.0e-5);
            assert!((point.transform.translation.y - expected.1).abs() < 1.0e-5);
            assert_eq!(session.frame().time, time);
        }
    }
}
