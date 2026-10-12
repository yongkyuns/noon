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
    let vector = lts
        .add_animated_vector(&mut scene, 2.0, 1.0, Color::from_hex(0xF7D96F))
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
    let basis = lts.basis_arrows();
    if basis.len() != 2 {
        return Err("basis vectors are disabled".into());
    }
    let source_vectors = scene
        .family(&[
            MobjectTarget::Family(basis[0].family()),
            MobjectTarget::Family(basis[1].family()),
            MobjectTarget::Family(vector.family()),
        ])
        .map_err(|error| error.to_string())?;
    // One structural family payload covers the grid and all arrows, so the
    // shared execution-session family-transform activation can capture the
    // complete LTS source and target leaf sets in one atomic publication.
    let source = scene
        .family(&[
            MobjectTarget::Family(source_plane),
            MobjectTarget::Family(&source_vectors),
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
    {
        let mut live = scene.live(&mut session);
        let intro = live
            .declare_and_activate_arrow_grow(
                &vector,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Smooth),
            )
            .map_err(|error| error.to_string())?;
        live.advance_segment_to(intro, 1.0)
            .map_err(|error| error.to_string())?;
        live.complete_segment(intro)
            .map_err(|error| error.to_string())?;
        live.declare_and_activate_family_transform_to(&source, &target, options)
            .map_err(|error| error.to_string())?;
    }
    // The builder activated both segments while constructing the timeline.
    // Return at authored time zero so direct and browser consumers see the
    // Arrow entrance before the following matrix transform.
    session.seek(0.0).map_err(|error| error.to_string())?;
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
        let yellow = Color::from_hex(0xF7D96F); // ManimCE 0.21 YELLOW_C.
        assert_eq!(
            initial
                .objects
                .iter()
                .filter(|row| row.style.fill == Some(yellow) || row.style.stroke == Some(yellow))
                .count(),
            2,
        );
        forward.advance_to(0.5).unwrap();
        let grow_midpoint = forward.frame().clone();
        assert_ne!(grow_midpoint, initial);
        forward.advance_to(2.5).unwrap();
        let transform_midpoint = forward.frame().clone();
        assert_ne!(transform_midpoint, grow_midpoint);
        forward.advance_to(4.0).unwrap();
        assert_eq!(
            forward.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );
        let endpoint = forward.frame().clone();
        assert_ne!(endpoint, initial);

        let mut sought = session().unwrap();
        sought.seek(0.5).unwrap();
        assert_eq!(sought.frame(), &grow_midpoint);
        sought.seek(2.5).unwrap();
        assert_eq!(sought.frame(), &transform_midpoint);
        sought.seek(4.0).unwrap();
        assert_eq!(sought.frame(), &endpoint);
    }
}
