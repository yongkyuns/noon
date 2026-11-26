//! Native counterpart of the Python cube sketch wireframe.
use crate::{
    AnimationOptions, AuthoringError, Color, ContinuationEffects, ContinuationScene,
    ContinuationStep, DeclaredAnimation, LiveContinuation, LiveProgram, MobjectFamily,
    MobjectTarget, RateFunction, Scene, ScriptError, SemanticVec3, SketchStroke,
};
use noon_core::ManimCamera3DProfile;

pub struct CubeOrbit {
    movement: DeclaredAnimation,
    step: u8,
}

impl LiveContinuation for CubeOrbit {
    const EFFECTS: ContinuationEffects = ContinuationEffects::SceneOnly;
    type Error = ScriptError;

    fn resume(&mut self, scene: &mut ContinuationScene) -> Result<ContinuationStep, ScriptError> {
        let segment = match self.step {
            0 | 2 => scene.live().wait_segment(0.25)?,
            1 => scene.live().play_animation(&self.movement)?,
            _ => return Ok(ContinuationStep::Finished),
        };
        self.step += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    orbit(|scene| {
        scene.cube_sketch_wireframe(
            2.2,
            Color::from_hex(0x236B8E),
            1.0,
            Color::from_hex(0xACEAD7),
            0.05,
            SketchStroke::new(1.15, 27).expect("valid cube sketch"),
        )
    })
}

pub(super) fn orbit(
    build: impl FnOnce(&mut Scene) -> Result<MobjectFamily, AuthoringError>,
) -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    orbit_between([0.64, -1.12], [1.1, 0.67], build)
}

pub(super) fn orbit_between(
    [phi, theta]: [f64; 2],
    [end_phi, end_theta]: [f64; 2],
    build: impl FnOnce(&mut Scene) -> Result<MobjectFamily, AuthoringError>,
) -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    let mut scene = Scene::new();
    let start = ManimCamera3DProfile {
        phi,
        theta,
        gamma: 0.0,
        focal_distance: 10.0,
        zoom: 1.65,
        frame_height: 8.0,
        frame_center: SemanticVec3::ZERO,
    };
    let camera = scene.camera_3d_profile(start, 0.1, 100.0)?;
    let object = build(&mut scene)?;
    scene.add_many(&[MobjectTarget::Family(&object)])?;
    let movement = scene.declare_camera_profile_move(
        &camera,
        ManimCamera3DProfile {
            phi: end_phi,
            theta: end_theta,
            ..start
        },
        AnimationOptions::new()
            .run_time(2.5)
            .rate_func(RateFunction::Linear),
    )?;
    Ok(scene.into_live_program(CubeOrbit { movement, step: 0 })?)
}

#[cfg(all(test, feature = "prepared-prefix"))]
#[test]
fn cube_sketch_wireframe_preserves_edge_geometry_when_scrubbed() {
    let mut player = program().unwrap().into_playback().unwrap();
    let mut callbacks = crate::RustHostCallbackTable::new();
    while player.prepare_until(&mut callbacks, 3.1).unwrap() {}
    assert_eq!(player.end_time(), Some(3.0));

    let frame = |player: &crate::PlaybackProgram<_>| player.session().unwrap().frame().clone();
    let edges = |frame: &noon_runtime::FrameState| {
        frame
            .objects
            .iter()
            .filter_map(|object| {
                let stroke = object.style.sketch_stroke?;
                assert!(object.content.geometry().is_some());
                Some((
                    object.content.clone(),
                    object.world_transform().unwrap(),
                    stroke,
                ))
            })
            .collect::<Vec<_>>()
    };
    let start = frame(&player);
    let wireframe = edges(&start);
    assert_eq!(wireframe.len(), 12);
    assert!(wireframe
        .iter()
        .enumerate()
        .all(|(i, (_, _, stroke))| stroke.seed() == 27 + i as u32));

    player.advance_to(&mut callbacks, 1.3).unwrap();
    let mid = frame(&player);
    assert_ne!(mid.objects, start.objects);
    assert_eq!(edges(&mid), wireframe);
    player.advance_to(&mut callbacks, 2.8).unwrap();
    let end = frame(&player);
    assert_eq!(edges(&end), wireframe);
    player.seek(0.0).unwrap();
    assert_eq!(frame(&player), start);
    player.seek(1.3).unwrap();
    assert_eq!(frame(&player), mid);
    player.advance_to(&mut callbacks, 2.8).unwrap();
    assert_eq!(frame(&player), end);
}
