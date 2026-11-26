//! Direct Rust counterpart to the Follow the label gallery scene.
use noon::{
    Color, Joint2d, JointAttachment2d, Mobject, RateFunction, RetargetedRig2d, Scene, Text,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn rectangle(scene: &mut Scene, width: f64, height: f64, color: Color) -> Result<Mobject> {
    let object = scene.rectangle(width, height)?;
    scene.set_fill(&object, color, 1.0)?;
    scene.set_stroke_width(&object, 0.0)?;
    scene.add(&object)?;
    Ok(object)
}

fn pointing_scene() -> Result<Scene> {
    let mut scene = Scene::new();
    let phase = scene.value_tracker(0.0)?;
    let path: Vec<_> = (0..=24)
        .map(|index| {
            let t = f64::from(index) / 24.0;
            (t as f32, [-0.42 + 0.82 * t, -0.34 + 1.16 * t])
        })
        .collect();
    let label_edge = 0.98;
    let fingertip = 0.34;
    rectangle(&mut scene, 14.4, 8.2, Color::from_hex(0xFFFAEF))?;
    for (origin, upper, forearm, side, color, height) in [
        ([-3.66, -1.00], 1.70, 1.73, -1.0, 0x30756D, 1.65),
        ([3.52, -1.50], 2.02, 1.14, 1.0, 0x894E68, 1.10),
    ] {
        let color = Color::from_hex(color);
        let body = rectangle(&mut scene, 1.28, height, color)?;
        scene.set_translation(&body, origin[0] + 0.58 * side, -2.37 + height / 2.0)?;
        let joints = [
            ("shoulder", None, origin),
            ("elbow", Some("shoulder"), [upper, 0.0]),
            ("wrist", Some("elbow"), [forearm, 0.0]),
        ]
        .map(|(name, parent, offset)| Joint2d {
            name: name.into(),
            parent: parent.map(str::to_owned),
            offset,
            rest_angle: 0.0,
        });
        let targets: Vec<_> = path
            .iter()
            .map(|&(t, [x, y])| (t, [x + side * label_edge, y]))
            .collect();
        let rig = RetargetedRig2d::new(joints.to_vec(), &[], &[])?;
        let tracks = rig.pointing_tracks("wrist", &targets, fingertip, side > 0.0)?;
        let sleeve = rectangle(&mut scene, upper + 0.10, 0.30, color)?;
        let cuff = rectangle(&mut scene, forearm + 0.10, 0.22, color)?;
        let hand = rectangle(&mut scene, fingertip, 0.13, Color::from_hex(0xEBAC4C))?;
        let attachments = [
            ("shoulder", sleeve, upper / 2.0),
            ("elbow", cuff, forearm / 2.0),
            ("wrist", hand, fingertip / 2.0),
        ]
        .map(|(joint, object, along)| JointAttachment2d {
            joint: joint.into(),
            object,
            offset: [along, 0.0],
        });
        scene.bind_rig_2d(&phase, &joints, &tracks, &attachments)?;
    }
    let card = rectangle(
        &mut scene,
        2.0 * label_edge,
        0.68,
        Color::from_hex(0x293643),
    )?;
    let label = scene.text(Text::new("LOOK HERE").with_font_size(20.0))?;
    scene.set_fill(&label, Color::WHITE, 1.0)?;
    scene.add(&label)?;
    for object in [&card, &label] {
        let rows = path
            .iter()
            .flat_map(|&(t, [x, y])| [t, x as f32, y as f32])
            .collect();
        scene.bind_sampled(object, &phase, "position", true, rows)?;
    }
    scene.wait(1.1)?;
    scene
        .play_value(&phase, 1.0)
        .rate_func(RateFunction::Smooth)
        .run_time(2.0)?;
    scene.wait(0.8)?;
    scene
        .play_value(&phase, 0.0)
        .rate_func(RateFunction::Smooth)
        .run_time(1.2)?;
    scene.wait(1.25)?;
    Ok(scene)
}

fn main() -> Result<()> {
    noon_native::run_with_config(
        pointing_scene()?.into_execution()?,
        noon_native::NativeViewportConfig {
            title: "Follow the label".into(),
            playback_controls: true,
            ..Default::default()
        },
    )?;
    Ok(())
}
