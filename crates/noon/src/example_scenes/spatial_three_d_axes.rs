//! Shared ThreeDAxes/camera qualification scene for native and direct WASM.

use crate::{
    AnimationOptions, Color, DeclaredAnimation, ExecutionSession, ManimThreeDAxes,
    ManimThreeDAxesOptions, RateFunction, Scene, SemanticSpatialCompositionDomain, SemanticVec3,
    SemanticWorldTransform3D,
};
use noon_core::{ManimCamera3DProfile, SemanticRotation3D};

fn start_profile() -> ManimCamera3DProfile {
    ManimCamera3DProfile {
        phi: 0.6,
        theta: -1.2,
        gamma: 0.0,
        focal_distance: 5.0,
        zoom: 1.0,
        frame_height: 8.0,
        frame_center: SemanticVec3::ZERO,
    }
}

fn end_profile() -> ManimCamera3DProfile {
    ManimCamera3DProfile {
        phi: 0.8,
        theta: -0.1,
        gamma: 0.2,
        focal_distance: 5.0,
        zoom: 1.1,
        frame_height: 8.0,
        frame_center: SemanticVec3::new(0.3, 0.0, 0.0),
    }
}

pub fn scene() -> Result<
    (
        Scene,
        ManimThreeDAxes,
        crate::Mobject,
        crate::Mobject,
        DeclaredAnimation,
    ),
    String,
> {
    let mut scene = Scene::new();
    let camera = scene
        .camera_3d_profile(start_profile(), 0.1, 100.0)
        .map_err(|error| error.to_string())?;
    let mut axes_options = ManimThreeDAxesOptions::default();
    axes_options.axis_overrides[0].tips = Some(false);
    axes_options.axis_overrides[0].color = Some(Color::RED);
    axes_options.axis_overrides[0].stroke_width = Some(0.04);
    axes_options.axis_overrides[1].color = Some(Color::GREEN);
    axes_options.axis_overrides[1].tick_size = Some(0.15);
    axes_options.axis_overrides[2].color = Some(Color::BLUE);
    let axes = scene
        .three_d_axes(&axes_options)
        .map_err(|error| error.to_string())?;
    scene
        .add_in_spatial_composition_domain(
            crate::MobjectTarget::Family(axes.family()),
            SemanticSpatialCompositionDomain::World,
        )
        .map_err(|error| error.to_string())?;
    let x_label = scene
        .text(crate::Text::new("x"))
        .map_err(|error| error.to_string())?;
    let y_label = scene
        .text(crate::Text::new("y"))
        .map_err(|error| error.to_string())?;
    let z_label = scene
        .text(crate::Text::new("z"))
        .map_err(|error| error.to_string())?;
    let labels = axes
        .create_axis_label_targets(
            &[
                (0, crate::MobjectTarget::Object(&x_label)),
                (1, crate::MobjectTarget::Object(&y_label)),
                (2, crate::MobjectTarget::Object(&z_label)),
            ],
            0.1,
            false,
        )
        .map_err(|error| error.to_string())?;
    scene
        .add_in_spatial_composition_domain(
            crate::MobjectTarget::Family(&labels),
            SemanticSpatialCompositionDomain::World,
        )
        .map_err(|error| error.to_string())?;
    let mut point = scene.circle(0.16).map_err(|error| error.to_string())?;
    point
        .set_fill(
            Color::RED.red.into(),
            Color::RED.green.into(),
            Color::RED.blue.into(),
            1.0,
        )
        .map_err(|error| error.to_string())?;
    point
        .set_stroke_width(0.0)
        .map_err(|error| error.to_string())?;
    let position = axes
        .authored_frame()
        .map_err(|error| error.to_string())?
        .c2p(2.0, -1.0, 1.5)
        .ok_or("ThreeDAxes c2p rejected finite coordinates")?;
    let world = SemanticWorldTransform3D::new(
        position,
        SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .ok_or("invalid point world transform")?;
    scene
        .set_world_transform(&point, world)
        .map_err(|error| error.to_string())?;
    scene
        .add_in_spatial_composition_domain(
            crate::MobjectTarget::Object(&point),
            SemanticSpatialCompositionDomain::World,
        )
        .map_err(|error| error.to_string())?;
    let movement = scene
        .declare_camera_profile_move(
            &camera,
            end_profile(),
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .map_err(|error| error.to_string())?;
    Ok((scene, axes, point, camera, movement))
}

pub fn session() -> Result<ExecutionSession, String> {
    let (scene, _, _, _, movement) = scene()?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    session
        .activate_animation_segment(
            &scene.integration_store().borrow(),
            movement.node_id(),
            AnimationOptions::new(),
        )
        .map_err(|error| error.to_string())?;
    Ok(session)
}

/// Circle screen-stroke qualification under the same finite moving 3D camera.
pub fn circle_screen_stroke_session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let camera = scene
        .camera_3d_profile(start_profile(), 0.1, 100.0)
        .map_err(|error| error.to_string())?;
    let mut circle = scene.circle(0.65).map_err(|error| error.to_string())?;
    circle
        .set_fill(1.0, 1.0, 1.0, 0.0)
        .map_err(|error| error.to_string())?;
    circle
        .set_stroke_color(1.0, 1.0, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    circle
        .set_stroke_width(0.04)
        .map_err(|error| error.to_string())?;
    circle
        .set_stroke_width_mode("screen_space")
        .map_err(|error| error.to_string())?;
    let pose = SemanticWorldTransform3D::new(
        SemanticVec3::new(-1.3, 1.35, 0.0),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.45)
            .ok_or("invalid Circle tilt")?,
        SemanticVec3::new(1.1, 0.7, 1.0),
    )
    .ok_or("invalid Circle world pose")?;
    scene
        .set_world_transform(&circle, pose)
        .map_err(|error| error.to_string())?;
    scene
        .add_in_spatial_composition_domain(
            crate::MobjectTarget::Object(&circle),
            SemanticSpatialCompositionDomain::World,
        )
        .map_err(|error| error.to_string())?;
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let movement = scene
        .declare_camera_profile_move(&camera, end_profile(), options)
        .map_err(|error| error.to_string())?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    session
        .activate_animation_segment(
            &scene.integration_store().borrow(),
            movement.node_id(),
            AnimationOptions::new(),
        )
        .map_err(|error| error.to_string())?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_axes_and_coordinate_point_share_the_finite_camera_track() {
        let (scene, axes, point, camera, _) = scene().unwrap();
        let frame = axes.authored_frame().unwrap();
        let point_state = point.state().unwrap();
        let expected = frame.c2p(2.0, -1.0, 1.5).unwrap();
        assert_eq!(point_state.transform.translation, expected);
        assert!(axes.tip(0).unwrap().is_none());
        assert!(axes.tip(1).unwrap().is_some());
        assert!(axes.tip(2).unwrap().is_some());
        let x_ticks = axes.axis(0).unwrap().ticks().unwrap();
        let x_tick_count = scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(x_ticks.node_id())
            .unwrap()
            .len();
        assert_eq!(x_tick_count, 12, "tipless X axis keeps both endpoint ticks");
        let mut session = session().unwrap();
        assert_eq!(session.frame().objects.len(), 98);
        for (time, expected_profile) in [(0.0, start_profile()), (1.0, end_profile())] {
            session.advance_to(time).unwrap();
            assert_eq!(
                session
                    .effective_camera_profile(camera.node_id())
                    .unwrap()
                    .0,
                expected_profile
            );
        }
        assert!(scene.integration_store().borrow().text_resources().len() >= 3);
    }
}
