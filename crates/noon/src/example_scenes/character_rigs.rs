//! Paired with `web/python/examples/showcase_character_rigs.py`.
use std::{f64::consts::PI, rc::Rc};

use crate::{
    Color, Joint2d, JointAttachment2d, JointTrack2d, Mobject, NoisePaint, RateFunction, Scene,
    SvgImportOptions, ValueTracker,
};

const ARTWORK: &str = include_str!("../../assets/paper_birds.svg");
const BROWN: Color = Color::from_hex(0x7A5132);
const INK: Color = Color::from_hex(0x253048);
const EYE_WHITE: Color = Color::from_hex(0xFFF8E7);
const GOLD: Color = Color::from_hex(0xDF9A24);
const ATTENTION: &[[f64; 2]] = &[
    [0.0, 0.0],
    [1.02, 0.0],
    [1.55, 0.72],
    [2.55, 1.0],
    [4.45, 1.0],
    [5.55, 0.0],
];
pub(super) const SHOULDER: &[[f64; 2]] = &[
    [0.0, 0.0],
    [0.96, 0.0],
    [1.27, -0.08],
    [1.91, 0.95],
    [3.15, 0.95],
    [3.48, 0.82],
    [3.96, 0.97],
    [5.03, 0.97],
    [5.65, 0.0],
];
pub(super) const WRIST: &[[f64; 2]] = &[
    [0.0, 0.0],
    [1.66, 0.0],
    [2.70, 2.52],
    [3.15, 2.52],
    [3.54, 2.20],
    [3.96, 2.52],
    [4.32, 2.52],
    [5.25, 0.0],
];

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn scene() -> Result<Scene> {
    let mut scene = Scene::new();
    let backdrop = scene.rectangle(14.4, 8.3)?;
    let paper = NoisePaint::new(
        31,
        [122.0, 154.0],
        [0.0, 0.0],
        [Color::from_hex(0xFCF1DC), Color::from_hex(0xF2E3C8)],
    )?;
    scene.set_fill_noise(&backdrop, paper, Some(1.0))?;
    scene.set_stroke_width(&backdrop, 0.0)?;
    scene.add(&backdrop)?;
    let ground = scene.line((-6.7, -2.97), (6.5, -2.97))?;
    stroke(&mut scene, &ground, BROWN, 0.012, 0.36)?;
    scene.add(&ground)?;
    for (x, width) in [(-4.23, 3.2), (3.53, 2.8)] {
        for (y, height, opacity) in [(-2.98, 0.16, 0.11), (-2.96, 0.082, 0.16)] {
            let shadow = disk(&mut scene, 1.0, BROWN, opacity)?;
            scene.set_translation(&shadow, x, y)?;
            scene.set_scale(&shadow, width / 2.0, height / 2.0)?;
        }
    }

    let clock = scene.value_tracker(0.0)?;
    for bird in [
        Bird {
            species: "heron",
            facing: 1.0,
            origin: [-4.02, -0.96],
            shoulder: [0.58, 0.13],
            upper: 1.45,
            rest: -0.52,
            head: [0.64, 2.24],
            eye: [0.56, 0.105],
            radius: 0.205,
            tilt: -0.20,
            gaze: -0.66,
            blink: [0.81, 5.97],
        },
        Bird {
            species: "sparrow",
            facing: -1.0,
            origin: [3.46, -1.89],
            shoulder: [-0.60, 0.19],
            upper: 0.66,
            rest: -0.20,
            head: [-0.17, 0.74],
            eye: [-0.425, 0.106],
            radius: 0.235,
            tilt: 0.24,
            gaze: 0.96,
            blink: [0.91, 5.87],
        },
    ] {
        add_bird(&mut scene, &clock, bird)?;
    }
    for (radius, color, opacity, x, y) in [
        (0.37, GOLD, 0.14, 0.15, 1.08),
        (0.136, GOLD, 1.0, 0.15, 1.08),
        (0.044, Color::from_hex(0xFFE6A5), 1.0, 0.115, 1.123),
    ] {
        let disc = disk(&mut scene, radius, color, opacity)?;
        scene.set_translation(&disc, x, y)?;
    }
    let ring = scene.circle(0.16)?;
    scene.set_translation(&ring, 0.15, 1.08)?;
    scene.set_fill_opacity(&ring, 0.0)?;
    stroke(&mut scene, &ring, GOLD, 0.027, 1.0)?;
    scene.add(&ring)?;
    scene.bind_sampled(
        &ring,
        &clock,
        "scale",
        true,
        vec![0.0, 1.0, 1.0, 2.67, 1.0, 1.0, 3.28, 2.75, 2.75],
    )?;
    scene.bind_sampled(
        &ring,
        &clock,
        "opacity",
        true,
        vec![0.0, 0.0, 2.67, 0.0, 2.76, 0.8, 3.28, 0.0],
    )?;
    scene
        .play_value(&clock, 3.0)
        .rate_func(RateFunction::Linear)
        .run_time(3.0)?;
    scene
        .play_value(&clock, 6.2)
        .rate_func(RateFunction::Linear)
        .run_time(3.2)?;
    scene.wait(1.0)?;
    Ok(scene)
}

struct Bird {
    species: &'static str,
    facing: f64,
    origin: [f64; 2],
    shoulder: [f64; 2],
    upper: f64,
    rest: f64,
    head: [f64; 2],
    eye: [f64; 2],
    radius: f64,
    tilt: f64,
    gaze: f64,
    blink: [f32; 2],
}

fn add_bird(scene: &mut Scene, clock: &ValueTracker, bird: Bird) -> Result<()> {
    let reflected = bird.facing < 0.0;
    let joints = [
        joint("body", None, bird.origin, 0.0),
        joint(
            "shoulder",
            Some("body"),
            bird.shoulder,
            if reflected { PI - bird.rest } else { bird.rest },
        ),
        joint(
            "wrist",
            Some("shoulder"),
            [bird.upper, 0.0],
            -2.48 * bird.facing,
        ),
        joint("head", Some("body"), bird.head, 0.0),
        joint("eye", Some("head"), bird.eye, 0.0),
        joint(
            "gaze",
            Some("eye"),
            [0.0, 0.0],
            if reflected { PI } else { 0.0 },
        ),
    ];
    let tracks = [
        eased("shoulder", SHOULDER, bird.facing),
        eased("wrist", WRIST, bird.facing),
        eased("head", ATTENTION, bird.facing * bird.tilt),
        eased("gaze", ATTENTION, bird.facing * bird.gaze),
    ];
    let mut attachments = Vec::new();
    for (layer, joint) in [
        ("body", "body"),
        ("primaries", "wrist"),
        ("forewing", "shoulder"),
        ("head", "head"),
    ] {
        let name = format!("{}-{layer}", bird.species);
        let group_start = format!("<g id=\"{name}\">");
        let (_, after) = ARTWORK
            .split_once(&group_start)
            .ok_or("missing paper bird layer")?;
        let (fragment, _) = after
            .split_once("</g>")
            .ok_or("unclosed paper bird layer")?;
        let source = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="10"><g transform="scale(.01)">{fragment}</g></svg>"#
        );
        let group = scene.svg_from_str_with_options(
            &source,
            SvgImportOptions {
                should_center: false,
                width: None,
                height: None,
            },
        )?;
        scene.add_many(&[(&group).into()])?;
        for &node in group.layout()?.leaves() {
            let leaf = Mobject::from_node(Rc::clone(scene.integration_store()), node)?;
            if reflected && matches!(joint, "shoulder" | "wrist") {
                scene.set_scale(&leaf, 1.0, -1.0)?;
            }
            attachments.push(attachment(joint, leaf, [0.0, 0.0]));
        }
    }

    let radius = bird.radius;
    let outline = disk(scene, radius + 0.026, INK, 1.0)?;
    let white = disk(scene, radius, EYE_WHITE, 1.0)?;
    let pupil = disk(scene, radius * 0.53, INK, 1.0)?;
    let gleam = disk(scene, radius * 0.16, EYE_WHITE, 1.0)?;
    attachments.extend([
        attachment("eye", outline.clone(), [0.0, 0.0]),
        attachment("eye", white.clone(), [0.0, 0.0]),
        attachment("gaze", pupil.clone(), [radius * 0.34, 0.0]),
        attachment(
            "gaze",
            gleam.clone(),
            [radius * 0.19, radius * 0.15 * bird.facing],
        ),
    ]);
    scene.bind_rig_2d(clock, &joints, &tracks, &attachments)?;

    let mut closed = vec![[0.0, 1.0]];
    let mut visibility = vec![0.0, 1.0];
    for time in bird.blink {
        closed.extend([[time - 0.10, 1.0], [time, 0.055], [time + 0.12, 1.0]]);
        visibility.extend([
            time - 0.08,
            1.0,
            time - 0.04,
            0.0,
            time + 0.05,
            0.0,
            time + 0.10,
            1.0,
        ]);
    }
    for (eye, aspect) in [
        (&outline, 0.79),
        (&white, 0.79),
        (&pupil, 0.87),
        (&gleam, 1.0),
    ] {
        let keys = closed
            .iter()
            .flat_map(|&[time, openness]| [time, 1.0, aspect * openness])
            .collect();
        scene.bind_sampled(eye, clock, "scale", true, keys)?;
    }
    scene.bind_sampled(&gleam, clock, "opacity", true, visibility)?;
    Ok(())
}

fn eased(joint: &str, keys: &[[f64; 2]], direction: f64) -> JointTrack2d {
    let mut samples = Vec::new();
    for interval in keys.windows(2) {
        let [start, first] = interval[0];
        let [end, last] = interval[1];
        for step in 0..8 {
            let t = f64::from(step) / 8.0;
            let smooth = t * t * (3.0 - 2.0 * t);
            samples.push([
                (start + (end - start) * t) as f32,
                (direction * (first + (last - first) * smooth)) as f32,
            ]);
        }
    }
    let [end, value] = keys[keys.len() - 1];
    samples.push([end as f32, (direction * value) as f32]);
    JointTrack2d {
        joint: joint.into(),
        keys: samples,
    }
}

fn joint(name: &str, parent: Option<&str>, offset: [f64; 2], rest_angle: f64) -> Joint2d {
    Joint2d {
        name: name.into(),
        parent: parent.map(str::to_owned),
        offset,
        rest_angle,
    }
}

fn attachment(joint: &str, object: Mobject, offset: [f64; 2]) -> JointAttachment2d {
    JointAttachment2d {
        joint: joint.into(),
        object,
        offset,
    }
}

fn disk(scene: &mut Scene, radius: f64, tint: Color, opacity: f64) -> Result<Mobject> {
    let shape = scene.circle(radius)?;
    scene.set_fill(&shape, tint, opacity)?;
    scene.set_stroke_width(&shape, 0.0)?;
    scene.add(&shape)?;
    Ok(shape)
}

fn stroke(
    scene: &mut Scene,
    shape: &Mobject,
    color: Color,
    width: f64,
    opacity: f64,
) -> Result<()> {
    scene.set_stroke(
        shape,
        color.red.into(),
        color.green.into(),
        color.blue.into(),
        opacity,
    )?;
    scene.set_stroke_width(shape, width)?;
    Ok(())
}
