//! Paired VectorScene/LinearTransformationScene matrix fixture.

use crate::{
    AnimationOptions, Color, ExecutionSession, LinearTransformationOptions, MobjectTarget,
    RateFunction, Scene,
};

const SWAP_AXES: [f64; 4] = [0.0, 1.0, 1.0, 0.0];

/// Build the ordinary retained LTS defaults and animate the axis swap through
/// shared family Transform tracks. The symmetric matrix gives Manim's exact
/// default `path_arc` of zero, which this fixture can exercise without an
/// unsupported curved transform payload.
pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut lts = scene
        .linear_transformation_setup(&LinearTransformationOptions::default())
        .map_err(|error| error.to_string())?;
    lts.add_vector(&mut scene, 2.0, 1.0, Color::from_hex(0xFFFF00))
        .map_err(|error| error.to_string())?;

    let source_plane = lts
        .foreground_plane()
        .ok_or("foreground plane is disabled")?
        .family();
    let target_plane_copy = source_plane
        .copy_family()
        .map_err(|error| error.to_string())?;
    let target_plane = target_plane_copy.root().clone();
    target_plane
        .apply_matrix(&SWAP_AXES, 2, 2, 0.0, 0.0)
        .map_err(|error| error.to_string())?;

    let target_vectors = lts
        .vector_matrix_target_family(&scene, &SWAP_AXES, 2, 2, (0.0, 0.0))
        .map_err(|error| error.to_string())?;
    let options = AnimationOptions::new()
        .run_time(3.0)
        .rate_func(RateFunction::Smooth)
        .path_arc(0.0);
    let vectors = lts.basis_vectors().ok_or("basis vectors are disabled")?;
    // One structural family payload covers the grid and all arrows, so the
    // shared execution-session family-transform activation can capture the
    // complete LTS source and target leaf sets in one atomic publication.
    let source = scene
        .family(&[
            MobjectTarget::Family(source_plane),
            MobjectTarget::Family(vectors),
        ])
        .map_err(|error| error.to_string())?;
    let target = scene
        .family(&[
            MobjectTarget::Family(&target_plane),
            MobjectTarget::Family(&target_vectors),
        ])
        .map_err(|error| error.to_string())?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    scene
        .live(&mut session)
        .declare_and_activate_family_transform_to(&source, &target, options)
        .map_err(|error| error.to_string())?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_runtime::TimelineWakeState;

    #[test]
    fn lts_fixture_uses_smooth_three_second_ordinary_family_transform() {
        let mut forward = session().unwrap();
        assert_eq!(
            forward.wake_state().timeline(),
            TimelineWakeState::Continuous
        );
        let initial = forward.frame().clone();
        forward.advance_to(1.5).unwrap();
        let middle = forward.frame().clone();
        assert_ne!(middle, initial);
        forward.advance_to(3.0).unwrap();
        assert_eq!(
            forward.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );
        let endpoint = forward.frame().clone();
        assert_ne!(endpoint, initial);

        let mut sought = session().unwrap();
        sought.seek(1.5).unwrap();
        assert_eq!(sought.frame(), &middle);
        sought.seek(3.0).unwrap();
        assert_eq!(sought.frame(), &endpoint);
    }
}
