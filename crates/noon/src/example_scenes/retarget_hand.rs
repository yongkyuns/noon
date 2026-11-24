//! Direct Rust counterpart of the gallery's inward fold and hand turn.
use crate::{
    AnimationOptions, AuthoringError, Color, ContinuationScene, ContinuationStep, Joint2d,
    JointTrack2d, LiveContinuation, LiveProgram, Mobject, RateFunction, RetargetedRig2d,
    RigArtwork2d, Scene, SemanticAnimationCompositionKind, SemanticPaint, SemanticStyle,
    TransformToRequest, Vec2, VectorPath,
};

const FOLD: [[f32; 3]; 5] = [
    [0.0, 0.0, 0.0],
    [0.35, 0.13, 0.72],
    [0.55, 0.24, 1.72],
    [0.80, 0.39, 2.90],
    [1.0, 0.0, 0.0],
];
const VIEWS: [&str; 5] = ["palm", "palm", "edge", "back", "palm"];
const INK: Color = Color::from_hex(0x303047);
const GLOVE: Color = Color::from_hex(0xFFD075);
const COATS: [Color; 2] = [Color::from_hex(0x4B8279), Color::from_hex(0xB95F56)];

fn rig(x: f64, upper: f64, forearm: f64) -> Result<RetargetedRig2d, AuthoringError> {
    let joints = [
        ("shoulder", None, [x + 0.46, 0.47], -0.25),
        ("elbow", Some("shoulder"), [upper, 0.0], 0.0),
        ("wrist", Some("elbow"), [forearm, 0.0], 0.0),
    ];
    let tracks = [("shoulder", 1), ("elbow", 2)].map(|(joint, column)| JointTrack2d {
        joint: joint.into(),
        keys: FOLD.iter().map(|row| [row[0], row[column]]).collect(),
    });
    RetargetedRig2d::new(
        joints
            .into_iter()
            .map(|(name, parent, offset, rest_angle)| Joint2d {
                name: name.into(),
                parent: parent.map(str::to_owned),
                offset,
                rest_angle,
            })
            .collect(),
        &tracks,
        &[],
    )
}

fn filled(color: Color) -> SemanticStyle {
    SemanticStyle {
        fill: Some(SemanticPaint::Solid(color)),
        stroke: Some(SemanticPaint::Solid(INK)),
        stroke_width: 0.025,
        ..Default::default()
    }
}

fn vector([x, y]: [f64; 2]) -> Vec2 {
    Vec2::new(x as f32, y as f32)
}

fn hand_outline(top: f64, bottom: f64, width: f64) -> Vec<[f64; 2]> {
    [
        [-0.05, 0.0],
        [-0.05, 0.83 * top],
        [0.08, top],
        [0.15, top],
        [0.19, top],
        [0.25, top * 0.63],
        [0.31, top * 0.71],
        [0.50, top * 0.85],
        [0.59, top * 0.52],
        [0.59, 0.0],
        [0.59, bottom * 0.52],
        [0.50, bottom * 0.85],
        [0.31, bottom * 0.71],
        [0.25, bottom * 0.63],
        [0.19, bottom],
        [0.15, bottom],
        [0.08, bottom],
        [-0.05, 0.83 * bottom],
        [-0.05, 0.0],
    ]
    .into_iter()
    .map(|[x, y]| [width * x, y])
    .collect()
}

struct Arm {
    hand: RigArtwork2d,
    rig: RetargetedRig2d,
    sleeve_color: Color,
    layer: f64,
}

impl Arm {
    fn new(
        x: f64,
        upper: f64,
        forearm: f64,
        hand_width: f64,
        sleeve_color: Color,
        layer: f64,
    ) -> Result<Self, AuthoringError> {
        let rig = rig(x, upper, forearm)?;
        let cues = FOLD
            .iter()
            .zip(VIEWS)
            .map(|(row, name)| (row[0], name.into()))
            .collect::<Vec<_>>();
        let hand = rig.artwork_track(
            "wrist",
            vec![
                ("palm".into(), hand_outline(0.15, -0.26, hand_width)),
                ("edge".into(), hand_outline(0.055, -0.055, hand_width)),
                ("back".into(), hand_outline(0.26, -0.15, hand_width)),
            ],
            &cues,
        )?;
        Ok(Self {
            hand,
            rig,
            sleeve_color,
            layer,
        })
    }

    fn drawings(&self, scene: &mut Scene, key: usize) -> Result<[Mobject; 3], AuthoringError> {
        let pose = self.rig.pose(FOLD[key][0])?;
        let wrist = pose.joint("wrist")?;
        let [sx, sy, _] = pose.joint("shoulder")?;
        let [ex, ey, _] = pose.joint("elbow")?;
        let sleeve = VectorPath::new()
            .move_to(vector([sx, sy]))
            .line_to(vector([ex, ey]))
            .line_to(vector([wrist[0], wrist[1]]));
        let glove = self.hand.points_at(FOLD[key][0])?;
        let stroke = |color, width| SemanticStyle {
            fill: None,
            stroke: Some(SemanticPaint::Solid(color)),
            stroke_width: width,
            ..Default::default()
        };
        let mut hand = VectorPath::new().move_to(vector(glove[0]));
        for &[a, b, c] in glove[1..].as_chunks::<3>().0 {
            hand = hand.cubic_to(vector(a), vector(b), vector(c));
        }
        let drawings = [
            scene.path(sleeve.clone(), stroke(INK, 0.33))?,
            scene.path(sleeve, stroke(self.sleeve_color, 0.29))?,
            scene.path(hand.close(), filled(GLOVE))?,
        ];
        for object in &drawings {
            object.set_z_index(self.layer)?;
        }
        Ok(drawings)
    }
}

type PoseStep = (f64, Option<Vec<Mobject>>);

pub struct RetargetedPoses {
    source: Vec<Mobject>,
    steps: std::vec::IntoIter<PoseStep>,
}

impl RetargetedPoses {
    pub(super) fn new(source: Vec<Mobject>, steps: Vec<PoseStep>) -> Self {
        Self {
            source,
            steps: steps.into_iter(),
        }
    }
}

impl LiveContinuation for RetargetedPoses {
    type Error = String;
    const EFFECTS: crate::ContinuationEffects = crate::ContinuationEffects::SceneOnly;

    fn resume(&mut self, scene: &mut ContinuationScene) -> Result<ContinuationStep, String> {
        let Some((duration, target)) = self.steps.next() else {
            return Ok(ContinuationStep::Finished);
        };
        let live = &mut scene.live();
        let segment = if let Some(target) = target {
            let options = AnimationOptions::new()
                .run_time(duration)
                .rate_func(RateFunction::Smooth);
            let children: Vec<_> = self
                .source
                .iter()
                .zip(&target)
                .map(|(source, target)| {
                    TransformToRequest::point_correspondence(source, target, options)
                })
                .collect();
            live.declare_and_activate_transform_composition(
                SemanticAnimationCompositionKind::Parallel,
                &children,
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new().rate_func(RateFunction::Linear),
            )
        } else {
            live.wait_segment(duration)
        }
        .map_err(|error| error.to_string())?;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<RetargetedPoses>, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let arms = [
        Arm::new(-3.76, 0.76, 1.27, 1.0, Color::from_hex(0x78AA93), 1.0)?,
        Arm::new(2.20, 0.57, 0.99, 0.84, Color::from_hex(0xDB8875), -1.0)?,
    ];
    let mut paper = scene.rectangle(14.4, 8.3)?;
    let background = Color::from_hex(0xF7ECD7);
    paper.set_fill(
        f64::from(background.red),
        f64::from(background.green),
        f64::from(background.blue),
        1.0,
    )?;
    paper.set_z_index(-2.0)?;
    scene.add(&paper)?;
    for (x, coat) in [-3.76, 2.20].into_iter().zip(COATS) {
        let mut torso = scene.rectangle(1.65, 1.73)?;
        torso.set_fill(
            f64::from(coat.red),
            f64::from(coat.green),
            f64::from(coat.blue),
            1.0,
        )?;
        torso.set_translation(x, 0.02)?;
        scene.add(&torso)?;
    }
    let mut pose_objects = |key| -> Result<Vec<Mobject>, AuthoringError> {
        let left = arms[0].drawings(&mut scene, key)?;
        let right = arms[1].drawings(&mut scene, key)?;
        Ok(left.into_iter().chain(right).collect())
    };
    let targets = [
        pose_objects(0)?,
        pose_objects(1)?,
        pose_objects(2)?,
        pose_objects(3)?,
    ];
    let source = pose_objects(0)?;
    scene.add_many(&source.iter().map(Into::into).collect::<Vec<_>>())?;
    let steps = [
        (0.75, None),
        (0.75, Some(1)),
        (0.75, Some(2)),
        (0.25, None),
        (0.75, Some(3)),
        (0.75, None),
        (0.75, Some(2)),
        (0.75, Some(1)),
        (0.75, Some(0)),
        (0.5, None),
    ]
    .into_iter()
    .map(|(duration, target)| (duration, target.map(|key| targets[key].clone())))
    .collect();
    Ok(scene
        .into_live_program(RetargetedPoses::new(source, steps))
        .map_err(|error| error.to_string())?)
}

#[cfg(all(test, feature = "prepared-prefix"))]
mod tests {
    use super::*;
    use crate::RustHostCallbackTable;

    #[test]
    fn direct_playback_and_seeks_keep_the_pose_and_painter_order() {
        let mut player = program().unwrap().into_playback().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        while player.prepare_until(&mut callbacks, 7.0).unwrap() {}
        let frame = |player: &crate::PlaybackProgram<RetargetedPoses>| {
            player.session().unwrap().frame().clone()
        };

        player.advance_to(&mut callbacks, 0.0).unwrap();
        let rest = frame(&player);
        assert_eq!(rest.time, 0.0);
        player.advance_to(&mut callbacks, 2.25).unwrap();
        let edge = frame(&player);
        assert_eq!(
            edge.time,
            2.25,
            "buffering {} seek {}",
            player.buffering(),
            player.can_seek_to(2.25)
        );
        player.advance_to(&mut callbacks, 3.5).unwrap();
        let folded = frame(&player);
        assert_eq!(folded.time, 3.5);
        assert!(rest.objects != edge.objects);
        assert!(edge.objects != folded.objects);
        let glove_layers: Vec<_> = folded
            .objects
            .iter()
            .filter(|object| object.style.fill == Some(GLOVE))
            .map(|object| object.z_index)
            .collect();
        assert_eq!(glove_layers.len(), 2);
        assert!(glove_layers.contains(&1.0) && glove_layers.contains(&-1.0));

        player.seek(0.0).unwrap();
        assert_eq!(frame(&player), rest);
        player.seek(3.5).unwrap();
        assert_eq!(frame(&player), folded);
        player.seek(2.25).unwrap();
        assert_eq!(frame(&player), edge);
    }
}
