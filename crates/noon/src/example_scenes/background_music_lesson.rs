//! A finite input/output lesson, paired with `background_music_lesson.py`.
//!
//! One scalar timeline drives both points through native position bindings. The
//! original score is prepared once and attached to the same Scene. There are no
//! callbacks or source continuations; native export and browser playback lower
//! this ordinary Scene directly.

use crate::{
    AudioClipOptions, AudioResource, Color, Mobject, RateFunction, Scene, SectionType,
    SemanticVec3, Text, DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
};

use super::background_audio::{music, SAMPLE_RATE};

type BuildResult<T> = Result<T, Box<dyn std::error::Error>>;

pub const DURATION_SECONDS: f64 = 30.0;
pub const ORIGIN_X: f64 = 1.0;
pub const INPUT_Y: f64 = 0.75;
pub const OUTPUT_Y: f64 = -1.1;
pub const SECTIONS: [(&str, f64); 3] = [
    ("Double a positive input", 2.0),
    ("Keep the sign", -2.0),
    ("Return to zero", 0.0),
];

const INK: Color = Color::from_hex(0xECF2F5);
const MUTED: Color = Color::from_hex(0x9CAFB9);
const GUIDE: Color = Color::from_hex(0x607884);
const PANEL: Color = Color::from_hex(0x13232D);
const BLUE: Color = Color::from_hex(0x58C4DD);
const GOLD: Color = Color::from_hex(0xF2C66D);

/// Complete finite lesson, suitable for `Scene::execution_session` and MP4 export.
pub fn scene() -> BuildResult<Scene> {
    let mut scene = Scene::new();
    let [input, output] = illustration(&mut scene)?;
    let tracker = scene.value_tracker(0.0)?;
    for (point, multiplier, y) in [(&input, 1.0, INPUT_Y), (&output, 2.0, OUTPUT_Y)] {
        let position = scene.position_from_tracker(
            &tracker,
            SemanticVec3::new(multiplier, 0.0, 0.0),
            SemanticVec3::new(ORIGIN_X, y, 0.0),
        )?;
        scene.bind_position(point, &position)?;
    }
    for (name, target) in SECTIONS {
        scene.next_section(name, SectionType::Normal)?;
        scene.wait(1.0)?;
        scene
            .play_value(&tracker, target)
            .rate_func(RateFunction::Linear)
            .run_time(8.0)?;
        scene.wait(1.0)?;
    }
    let pcm = music::render_music(SAMPLE_RATE)?;
    let resource = AudioResource::from_interleaved(
        pcm.sample_rate(),
        music::CHANNELS as u16,
        pcm.into_samples(),
        DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
    )?;
    scene.audio(
        resource,
        AudioClipOptions {
            duration: Some(DURATION_SECONDS),
            gain: 0.5,
            fade_in: 1.5,
            fade_out: 2.5,
            ..AudioClipOptions::default()
        },
    )?;
    Ok(scene)
}

/// Paint the paired lesson and return its two normal semantic point handles.
/// Keeping the illustration independent makes the binding qualification compare
/// the two authoring entrypoints against the same rendered composition.
pub fn illustration(scene: &mut Scene) -> BuildResult<[Mobject; 2]> {
    label(scene, "Double the input", 42.0, 0.0, 3.05, INK)?;
    label(
        scene,
        "One rule connects two moving points:  y = 2x",
        23.0,
        0.0,
        2.45,
        MUTED,
    )?;
    for (name, symbol, y, tint) in [
        ("INPUT", "x", INPUT_Y, BLUE),
        ("OUTPUT", "2x", OUTPUT_Y, GOLD),
    ] {
        let mut panel = scene.rectangle(12.5, 1.6)?;
        panel.set_translation(0.0, y)?;
        panel.set_fill(PANEL.red.into(), PANEL.green.into(), PANEL.blue.into(), 1.0)?;
        panel.set_stroke_width(0.0)?;
        scene.add(&panel)?;
        label(scene, name, 18.0, -4.8, y + 0.22, tint)?;
        label(scene, symbol, 32.0, -4.8, y - 0.22, INK)?;
        line(scene, (-3.0, y), (5.0, y), GUIDE, 0.025)?;
        for value in -4..=4 {
            let x = ORIGIN_X + f64::from(value);
            let tint = if value == 0 { INK } else { MUTED };
            line(scene, (x, y - 0.1), (x, y + 0.1), tint, 0.02)?;
            label(scene, &value.to_string(), 18.0, x, y - 0.43, tint)?;
        }
    }
    label(
        scene,
        "Same sign. Twice the distance from zero.",
        24.0,
        0.0,
        -2.25,
        INK,
    )?;
    for (x, heading, explanation) in [
        (-4.3, "01  POSITIVE", "2 becomes 4"),
        (0.0, "02  NEGATIVE", "-2 becomes -4"),
        (4.3, "03  ZERO", "0 stays 0"),
    ] {
        label(scene, heading, 18.0, x, -2.95, BLUE)?;
        label(scene, explanation, 18.0, x, -3.35, MUTED)?;
    }
    let mut points = Vec::with_capacity(2);
    for (y, tint) in [(INPUT_Y, BLUE), (OUTPUT_Y, GOLD)] {
        let mut point = scene.circle(0.18)?;
        point.set_translation(ORIGIN_X, y)?;
        point.set_fill(tint.red.into(), tint.green.into(), tint.blue.into(), 1.0)?;
        point.set_stroke_width(0.0)?;
        scene.add(&point)?;
        points.push(point);
    }
    Ok([points.remove(0), points.remove(0)])
}

fn label(
    scene: &mut Scene,
    source: &str,
    size: f32,
    x: f64,
    y: f64,
    tint: Color,
) -> BuildResult<()> {
    let mut label = scene.text(
        Text::new(source)
            .with_font("DejaVu Sans Mono")
            .with_font_size(size)
            .color(tint),
    )?;
    label.move_to(x, y)?;
    scene.add(&label)?;
    Ok(())
}

fn line(
    scene: &mut Scene,
    start: (f64, f64),
    end: (f64, f64),
    tint: Color,
    width: f64,
) -> BuildResult<()> {
    let mut line = scene.line(start, end)?;
    line.set_color(tint.red.into(), tint.green.into(), tint.blue.into(), 1.0)?;
    line.set_stroke_width(width)?;
    scene.add(&line)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lesson_sections_points_music_and_seek_share_one_finite_timeline() {
        let scene = scene().unwrap();
        assert_eq!(scene.time(), DURATION_SECONDS);
        assert_eq!(
            scene
                .sections()
                .iter()
                .map(|section| (section.name.as_str(), section.time))
                .collect::<Vec<_>>(),
            [
                (SECTIONS[0].0, 0.0),
                (SECTIONS[1].0, 10.0),
                (SECTIONS[2].0, 20.0)
            ]
        );
        let mut forward = scene.execution_session().unwrap();
        assert_eq!(forward.finite_timeline_end_time(), Some(DURATION_SECONDS));
        assert!(!forward.has_required_callbacks());
        for (time, input) in [
            (0.0, 0.0),
            (0.75, 0.0),
            (5.0, 1.0),
            (9.0, 2.0),
            (10.0, 2.0),
            (13.0, 1.0),
            (15.0, 0.0),
            (19.0, -2.0),
            (20.0, -2.0),
            (25.0, -1.0),
            (28.75, -0.0625),
            (29.0, 0.0),
            (30.0, 0.0),
        ] {
            forward.advance_to(time).unwrap();
            let mut seek = scene.execution_session().unwrap();
            seek.seek(time).unwrap();
            assert_eq!(
                forward.frame().objects,
                seek.frame().objects,
                "seek at {time}"
            );
            let points = &forward.frame().objects[forward.frame().objects.len() - 2..];
            assert_eq!(points[0].transform.translation.x, (ORIGIN_X + input) as f32);
            assert_eq!(points[0].transform.translation.y, INPUT_Y as f32);
            assert_eq!(
                points[1].transform.translation.x,
                (ORIGIN_X + 2.0 * input) as f32
            );
            assert_eq!(points[1].transform.translation.y, OUTPUT_Y as f32);
            let clips = forward.audio_playback().unwrap().collect::<Vec<_>>();
            if time == DURATION_SECONDS {
                assert!(clips.is_empty(), "music ends exactly with the lesson");
            } else {
                assert_eq!(clips.len(), 1);
                assert_eq!(clips[0].source_offset, time);
                assert_eq!(clips[0].duration, DURATION_SECONDS - time);
                let expected_gain =
                    0.5 * (time / 1.5).min(1.0) * ((DURATION_SECONDS - time) / 2.5).min(1.0);
                assert!(
                    (clips[0].gain_points.points()[0].gain - expected_gain).abs() < 1e-10,
                    "music gain at {time}"
                );
            }
        }
    }
}
