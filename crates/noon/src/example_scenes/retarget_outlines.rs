//! Direct Rust version of the outline bend, using the paper birds' clip.
use crate::{
    AuthoringError, Color, Joint2d, JointMapping2d, JointTrack2d, LiveProgram,
    ManimGeometryOptions, Mobject, RetargetedRig2d, Scene,
};

use super::{
    character_rigs::{SHOULDER, WRIST},
    retarget_hand::RetargetedPoses,
};

const FLOOR: f64 = -2.30;
const POSE_TIMES: [f64; 9] = [1.27, 1.91, 2.70, 3.15, 3.54, 3.96, 4.32, 5.25, 5.65];

struct Outline {
    rig: RetargetedRig2d,
    artwork: Mobject,
    offset: [f64; 2],
}

impl Outline {
    fn new(mut artwork: Mobject, x: f64, color: Color) -> Result<Self, AuthoringError> {
        artwork.insert_n_curves(28)?;
        let (bottom, top) = artwork
            .path_query()?
            .points()?
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), &(_, y)| {
                (min.min(y), max.max(y))
            });
        let height = top - bottom;
        let joints = [
            ("base", None, [0.0, bottom + 0.21 * height]),
            ("lower", Some("base"), [0.0, 0.37 * height]),
            ("upper", Some("lower"), [0.0, 0.36 * height]),
        ]
        .into_iter()
        .map(|(name, parent, offset)| Joint2d {
            name: name.into(),
            parent: parent.map(str::to_owned),
            offset,
            rest_angle: 0.0,
        })
        .collect();
        let tracks = [("shoulder", SHOULDER), ("wrist", WRIST)].map(|(joint, keys)| JointTrack2d {
            joint: joint.into(),
            keys: keys
                .iter()
                .map(|&[time, angle]| [time as f32, angle as f32])
                .collect(),
        });
        let mapping = [("shoulder", "lower", -0.44), ("wrist", "upper", -0.15)].map(
            |(source, target, gain)| JointMapping2d {
                source: source.into(),
                target: target.into(),
                gain,
            },
        );
        let rig = RetargetedRig2d::new(joints, &tracks, &mapping)?;
        artwork.set_fill(color.red.into(), color.green.into(), color.blue.into(), 1.0)?;
        artwork.disable_stroke()?;
        Ok(Self {
            rig,
            artwork,
            offset: [x, FLOOR - bottom],
        })
    }

    fn drawing(&self, scene: &mut Scene, time: f32) -> Result<Mobject, AuthoringError> {
        let pose = self.rig.pose(time)?;
        let mut object = pose.target_for(scene, &self.artwork, None)?;
        object.shift(self.offset[0], self.offset[1])?;
        Ok(object)
    }
}

pub fn program() -> Result<LiveProgram<RetargetedPoses>, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let paper = scene.rectangle(14.4, 8.3)?;
    scene.set_fill(&paper, Color::from_hex(0xFCF3DF), 1.0)?;
    scene.set_stroke_width(&paper, 0.0)?;
    scene.add(&paper)?;
    let cast = [
        Outline::new(scene.square(2.05)?, -4.49, Color::from_hex(0xE88971))?,
        Outline::new(scene.circle(1.06)?, -0.24, Color::from_hex(0x64AF94))?,
        Outline::new(
            scene.geometry(ManimGeometryOptions::star(5, 1.35, Some(0.80), 2, None)?)?,
            3.92,
            Color::from_hex(0xF1B952),
        )?,
    ];
    let mut pose_objects = |time| {
        cast.iter()
            .map(|body| body.drawing(&mut scene, time))
            .collect::<Result<Vec<_>, _>>()
    };
    let source = pose_objects(0.0)?;
    let mut previous = 0.96;
    let mut steps = vec![(previous, None)];
    for time in POSE_TIMES {
        let target = match time {
            3.15 | 4.32 => None,
            _ => Some(pose_objects(time as f32)?),
        };
        steps.push((time - previous, target));
        previous = time;
    }
    steps.push((7.2 - previous, None));
    scene.add_many(&source.iter().map(Into::into).collect::<Vec<_>>())?;
    Ok(scene
        .into_live_program(RetargetedPoses::new(source, steps))
        .map_err(|error| error.to_string())?)
}

#[cfg(all(test, feature = "prepared-prefix"))]
mod tests {
    use crate::RustHostCallbackTable;

    #[test]
    fn direct_bend_plays_and_seeks_back_to_the_rest_outline() {
        let mut player = super::program().unwrap().into_playback().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        while player.prepare_until(&mut callbacks, 8.0).unwrap() {}
        let frame = |player: &crate::PlaybackProgram<_>| player.session().unwrap().frame().clone();
        player.advance_to(&mut callbacks, 0.0).unwrap();
        let rest = frame(&player);
        assert_eq!(rest.time, 0.0);
        player.advance_to(&mut callbacks, 2.85).unwrap();
        let bend = frame(&player);
        assert_eq!(
            bend.time,
            2.85,
            "buffering {} seek {}",
            player.buffering(),
            player.can_seek_to(2.85)
        );
        assert!(rest.objects != bend.objects || rest.render_geometries != bend.render_geometries);
        player.seek(0.0).unwrap();
        assert_eq!(frame(&player), rest);
        player.seek(2.85).unwrap();
        assert_eq!(frame(&player), bend);
    }
}
