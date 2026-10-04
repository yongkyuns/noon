//! Mixed World/FixedOrientation/FixedFrame text under an ordinary camera-profile track.

use crate::{
    AnimationOptions, Color, DeclaredAnimation, ExecutionSession, MathTypst, MobjectTarget,
    RateFunction, Scene, SemanticPaint, SemanticSpatialCompositionDomain, SemanticStyle,
    SemanticVec3, SemanticWorldTransform3D, Typst, VectorPath,
};
use noon_core::{ManimCamera3DProfile, SemanticRotation3D};

fn from_profile() -> ManimCamera3DProfile {
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

fn to_profile() -> ManimCamera3DProfile {
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

fn pose(translation: SemanticVec3) -> SemanticWorldTransform3D {
    pose_with_scale(translation, SemanticVec3::new(1.0, 1.0, 1.0))
}

fn pose_with_scale(translation: SemanticVec3, scale: SemanticVec3) -> SemanticWorldTransform3D {
    SemanticWorldTransform3D::new(translation, SemanticRotation3D::IDENTITY, scale)
        .expect("finite identity transform")
}

fn fit_frame_text(
    object: &mut crate::Mobject,
    center: SemanticVec3,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let anchor = crate::LayoutAnchor::from(&*object);
    anchor
        .rescale_to_fit(width, crate::LayoutDimension::Width, true)
        .map_err(|error| error.to_string())?;
    anchor
        .rescale_to_fit(height, crate::LayoutDimension::Height, true)
        .map_err(|error| error.to_string())?;
    object
        .move_to(center.x, center.y)
        .map_err(|error| error.to_string())
}

pub fn scene() -> Result<(Scene, crate::Mobject, DeclaredAnimation), String> {
    let mut scene = Scene::new();
    let camera = scene
        .camera_3d_profile(from_profile(), 0.1, 100.0)
        .map_err(|error| error.to_string())?;
    let background = scene
        .path(
            VectorPath::new()
                .move_to(crate::Vec2::new(-3.0, -3.0))
                .line_to(crate::Vec2::new(3.0, -3.0))
                .line_to(crate::Vec2::new(3.0, 3.0))
                .line_to(crate::Vec2::new(-3.0, 3.0))
                .close(),
            SemanticStyle {
                fill: Some(SemanticPaint::Solid(Color::rgba(0.08, 0.12, 0.22, 1.0))),
                fill_opacity: 1.0,
                stroke: None,
                stroke_width: 0.0,
                object_opacity: 1.0,
                ..SemanticStyle::default()
            },
        )
        .map_err(|error| error.to_string())?;
    let mut world_label = scene
        .typst(Typst::new("#text(fill: red)[World label]"))
        .map_err(|error| error.to_string())?;
    let mut formula = scene
        .math_typst(MathTypst::new("frac(x, 2)"))
        .map_err(|error| error.to_string())?;
    let mut left = scene
        .typst(Typst::new("#text(fill: red)[Fixed label]"))
        .map_err(|error| error.to_string())?;
    let mut right = scene
        .typst(Typst::new("#text(fill: red)[Anchor label]"))
        .map_err(|error| error.to_string())?;
    let mut hud = scene
        .typst(Typst::new("#text(fill: yellow)[Fixed frame]"))
        .map_err(|error| error.to_string())?;

    scene
        .set_world_transform(&background, pose(SemanticVec3::ZERO))
        .map_err(|error| error.to_string())?;
    for (object, center, width, height) in [
        (
            &mut world_label,
            SemanticVec3::new(-3.2, 2.6, 0.1),
            1.8,
            0.28,
        ),
        (&mut formula, SemanticVec3::new(2.2, 2.6, 0.1), 0.8, 0.55),
        (&mut left, SemanticVec3::new(-1.0, -2.0, 0.3), 1.2, 0.24),
        (&mut right, SemanticVec3::new(1.0, -2.0, -0.3), 1.2, 0.24),
    ] {
        fit_frame_text(object, center, width, height)?;
        let mut world = object
            .world_transform()
            .map_err(|error| error.to_string())?;
        world.translation.z = center.z;
        scene
            .set_world_transform(object, world)
            .map_err(|error| error.to_string())?;
    }
    fit_frame_text(&mut hud, SemanticVec3::new(-3.4, -3.4, 0.0), 1.1, 0.22)?;
    left.set_object_opacity(0.5)
        .map_err(|error| error.to_string())?;
    right
        .set_object_opacity(0.5)
        .map_err(|error| error.to_string())?;
    let family = scene
        .family(&[MobjectTarget::Object(&left), MobjectTarget::Object(&right)])
        .map_err(|error| error.to_string())?;
    scene
        .add_all_in_spatial_composition_domain(
            &[
                MobjectTarget::Object(&background),
                MobjectTarget::Object(&world_label),
                MobjectTarget::Object(&formula),
            ],
            SemanticSpatialCompositionDomain::World,
        )
        .map_err(|error| error.to_string())?;
    scene
        .add_in_spatial_composition_domain(
            MobjectTarget::Family(&family),
            SemanticSpatialCompositionDomain::FixedOrientation,
        )
        .map_err(|error| error.to_string())?;
    scene
        .add_in_spatial_composition_domain(
            MobjectTarget::Object(&hud),
            SemanticSpatialCompositionDomain::FixedFrame,
        )
        .map_err(|error| error.to_string())?;

    let movement = scene
        .declare_camera_profile_move(
            &camera,
            to_profile(),
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .map_err(|error| error.to_string())?;
    Ok((scene, camera, movement))
}

pub fn session() -> Result<ExecutionSession, String> {
    let (scene, _, movement) = scene()?;
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
    use noon_core::SemanticSpatialCompositionDomain as Domain;

    #[test]
    fn fitting_retains_constructor_text_units_and_matches_authored_dimensions() {
        let mut scene = Scene::new();
        let mut text = scene.typst(Typst::new("Fixed frame")).unwrap();
        fit_frame_text(&mut text, SemanticVec3::new(-3.4, -3.4, 0.0), 1.1, 0.22).unwrap();
        assert!((text.width().unwrap() - 1.1).abs() < 1e-9);
        assert!((text.height().unwrap() - 0.22).abs() < 1e-9);
        let center = text.center().unwrap();
        assert!((center.0 + 3.4).abs() < 1e-9);
        assert!((center.1 + 3.4).abs() < 1e-9);
    }

    #[test]
    fn camera_label_fixture_keeps_exact_family_center_and_moves_one_profile_track() {
        let (scene, camera, _) = scene().unwrap();
        let mut session = session().unwrap();
        for (time, expected) in [
            (0.0, from_profile()),
            (
                0.5,
                ManimCamera3DProfile::interpolate(from_profile(), to_profile(), 0.5).unwrap(),
            ),
            (1.0, to_profile()),
        ] {
            session.advance_to(time).unwrap();
            assert_eq!(
                session
                    .effective_camera_profile(camera.node_id())
                    .unwrap()
                    .0,
                expected
            );
        }
        assert_eq!(session.frame().objects.len(), 7);
        let family_rows: Vec<_> = session
            .frame()
            .objects
            .iter()
            .filter_map(|row| row.spatial.as_deref())
            .filter(|spatial| spatial.composition_domain == Domain::FixedOrientation)
            .collect();
        assert_eq!(family_rows.len(), 2);
        assert_eq!(
            family_rows[0].fixed_orientation_center,
            family_rows[1].fixed_orientation_center
        );
        let fixed_center = family_rows[0]
            .fixed_orientation_center
            .expect("fixed family has a shared authored geometry center");
        assert!((fixed_center.x).abs() < 1e-9);
        assert!((fixed_center.y + 2.0).abs() < 1e-9);
        assert!((fixed_center.z).abs() < 1e-9);
        assert_eq!(
            session
                .frame()
                .objects
                .iter()
                .filter_map(|row| row.spatial.as_deref())
                .filter(|spatial| spatial.composition_domain == Domain::FixedFrame)
                .count(),
            1
        );
        assert!(scene.integration_store().borrow().text_resources().len() >= 5);
    }
}
