//! Native Rust counterpart of the 3D sketch path demo.
use crate::{
    AnimationOptions, Color, ContinuationEffects, ContinuationScene, ContinuationStep,
    DeclaredAnimation, LiveContinuation, LiveProgram, RateFunction, Scene, ScriptError,
    SemanticStyle, SemanticVec3, SemanticWorldTransform3D, StyleUpdate, Vec2, VectorPath,
};
use noon_core::{ManimCamera3DProfile, SemanticRotation3D};

pub struct SketchPathOrbit {
    movement: DeclaredAnimation,
    step: u8,
}

impl LiveContinuation for SketchPathOrbit {
    const EFFECTS: ContinuationEffects = ContinuationEffects::SceneOnly;
    type Error = ScriptError;

    fn resume(&mut self, scene: &mut ContinuationScene) -> Result<ContinuationStep, ScriptError> {
        let segment = match self.step {
            0 | 2 => scene.live().wait_segment(0.4)?,
            1 => scene.live().play_animation(&self.movement)?,
            _ => return Ok(ContinuationStep::Finished),
        };
        self.step += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<SketchPathOrbit>, ScriptError> {
    let mut scene = Scene::new();
    let start = ManimCamera3DProfile {
        phi: 0.32,
        theta: -1.85,
        gamma: 0.0,
        focal_distance: 10.0,
        zoom: 1.5,
        frame_height: 8.0,
        frame_center: SemanticVec3::ZERO,
    };
    let camera = scene.camera_3d_profile(start, 0.1, 100.0)?;

    let panel = scene.rectangle(2.65, 1.85)?;
    scene.set_world_transform(
        &panel,
        SemanticWorldTransform3D {
            translation: SemanticVec3::new(-0.9, 0.5, 0.0),
            rotation: SemanticRotation3D::from_axis_angle(
                SemanticVec3::new(1.0, 0.0, 0.0),
                std::f64::consts::FRAC_PI_6,
            )
            .expect("finite panel tilt"),
            ..SemanticWorldTransform3D::IDENTITY
        },
    )?;
    scene.set_style(
        &panel,
        StyleUpdate {
            fill_color: Some(Color::from_hex(0x236B8E)),
            fill_opacity: Some(1.0),
            ..StyleUpdate::sketch_stroke(Color::from_hex(0x5CD0B3), Some(0.04), None, 1.4, 7)?
        },
    )?;

    let curve = scene.path(
        VectorPath::new().move_to(Vec2::new(-2.25, -0.58)).cubic_to(
            Vec2::new(-0.65, -1.8),
            Vec2::new(0.7, 1.2),
            Vec2::new(2.55, -0.4),
        ),
        SemanticStyle::default(),
    )?;
    scene.set_world_transform(&curve, SemanticWorldTransform3D::IDENTITY)?;
    scene.set_fill_opacity(&curve, 0.0)?;
    scene.set_style(
        &curve,
        StyleUpdate::sketch_stroke(Color::from_hex(0xF7D96F), Some(0.05), None, 1.3, 17)?,
    )?;

    let occluder = scene.rectangle(1.15, 1.45)?;
    scene.set_style(
        &occluder,
        StyleUpdate {
            fill_color: Some(Color::from_hex(0xCF5044)),
            fill_opacity: Some(1.0),
            stroke_width: Some(0.0),
            ..StyleUpdate::default()
        },
    )?;
    scene.set_world_transform(
        &occluder,
        SemanticWorldTransform3D {
            translation: SemanticVec3::new(0.4, -0.18, 1.15),
            ..SemanticWorldTransform3D::IDENTITY
        },
    )?;
    for object in [&occluder, &panel, &curve] {
        scene.add(object)?;
    }
    let movement = scene.declare_camera_profile_move(
        &camera,
        ManimCamera3DProfile {
            phi: 0.85,
            theta: -0.6,
            ..start
        },
        AnimationOptions::new()
            .run_time(2.2)
            .rate_func(RateFunction::Linear),
    )?;
    Ok(scene.into_live_program(SketchPathOrbit { movement, step: 0 })?)
}

#[cfg(all(test, feature = "prepared-prefix"))]
#[test]
fn spatial_sketch_paths_preserve_camera_and_strokes_when_scrubbed() {
    let mut player = program().unwrap().into_playback().unwrap();
    let mut callbacks = crate::RustHostCallbackTable::new();
    while player.prepare_until(&mut callbacks, 3.1).unwrap() {}
    assert_eq!(player.end_time(), Some(3.0));

    let frame = |player: &crate::PlaybackProgram<_>| player.session().unwrap().frame().clone();
    let camera = |frame: &noon_runtime::FrameState| {
        frame
            .objects
            .iter()
            .find_map(|object| object.camera_profile())
            .unwrap()
    };
    let start = frame(&player);
    player.advance_to(&mut callbacks, 1.3).unwrap();
    let mid = frame(&player);
    assert_ne!(camera(&start), camera(&mid));
    assert_eq!(
        mid.objects
            .iter()
            .filter_map(|object| {
                let stroke = object.style.sketch_stroke?;
                assert!(object.world_transform().is_some());
                Some(stroke.seed())
            })
            .collect::<Vec<_>>(),
        [7, 17]
    );
    player.advance_to(&mut callbacks, 2.75).unwrap();
    let end = frame(&player);
    assert_ne!(camera(&mid), camera(&end));

    player.seek(0.0).unwrap();
    assert_eq!(frame(&player), start);
    player.seek(1.3).unwrap();
    assert_eq!(frame(&player), mid);
    player.advance_to(&mut callbacks, 2.75).unwrap();
    assert_eq!(frame(&player), end);
}
