//! Focused VectorSpaceScene slice for coordinates, extra transformables and ghosts.

use crate::plot_presentation::NumberLabelOptions;
use crate::{
    AnimationCompositionRequest, AnimationOptions, Color, ExecutionSession,
    LinearTransformationOptions, ManimNumberPlaneOptions, MobjectTarget, RateFunction, Scene,
};

const SWAP_AXES: [f64; 4] = [0.0, 1.0, 1.0, 0.0];
const STRETCH_X: [f64; 4] = [2.0, 0.0, 0.0, 1.0];
const STRETCH_AFTER_SWAP: [f64; 4] = [0.0, 2.0, 1.0, 0.0];

pub fn session() -> Result<ExecutionSession, String> {
    let plane = ManimNumberPlaneOptions {
        x_range: [-1.0, 1.0, 1.0],
        y_range: [-1.0, 1.0, 1.0],
        x_length: Some(4.0),
        y_length: Some(4.0),
        ..Default::default()
    };
    let mut scene = Scene::new();
    let mut lts = scene
        .linear_transformation_setup(&LinearTransformationOptions {
            background_plane: Some(plane),
            foreground_plane: None,
            show_basis_vectors: false,
        })
        .map_err(|error| error.to_string())?;
    let number_label_options = NumberLabelOptions {
        font_size: 24.0,
        buff: f64::from(noon_core::SMALL_BUFF),
        direction: [1.0, -1.0],
        ..NumberLabelOptions::default()
    };
    lts.background_plane()
        .ok_or("background plane is disabled")?
        .add_coordinates(None, None, &number_label_options, &number_label_options)
        .map_err(|error| error.to_string())?;

    let mut square = scene.square(0.5).map_err(|error| error.to_string())?;
    square
        .move_to(0.5, 0.5)
        .map_err(|error| error.to_string())?;
    let square_family = scene
        .family(&[MobjectTarget::from(&square)])
        .map_err(|error| error.to_string())?;
    lts.add_transformable_mobject(&mut scene, &square_family)
        .map_err(|error| error.to_string())?;

    // The square exercises generic ApplyMatrix; the tracked Arrow follows the
    // endpoint-aware vector path and is the only family that receives ghosts.
    let vector = lts
        .add_vector(&mut scene, 0.5, 0.25, Color::from_hex(0xFFFF00))
        .map_err(|error| error.to_string())?
        .clone();
    let vector_family = scene
        .family(&[MobjectTarget::from(vector.family())])
        .map_err(|error| error.to_string())?;
    let first_vector_target = lts
        .vector_matrix_target(&scene, 0, &SWAP_AXES, 2, 2)
        .map_err(|error| error.to_string())?;
    let first_vector_target_family = scene
        .family(&[MobjectTarget::from(first_vector_target.family())])
        .map_err(|error| error.to_string())?;
    let second_vector_target = lts
        .vector_matrix_target(&scene, 0, &STRETCH_AFTER_SWAP, 2, 2)
        .map_err(|error| error.to_string())?;
    let second_vector_target_family = scene
        .family(&[MobjectTarget::from(second_vector_target.family())])
        .map_err(|error| error.to_string())?;

    let first_target_copy = square_family
        .copy_family()
        .map_err(|error| error.to_string())?;
    let first_target_family = first_target_copy.root().clone();
    first_target_family
        .apply_matrix(&SWAP_AXES, 2, 2, 0.0, 0.0)
        .map_err(|error| error.to_string())?;
    let second_target_copy = first_target_family
        .copy_family()
        .map_err(|error| error.to_string())?;
    let second_target_family = second_target_copy.root().clone();
    second_target_family
        .apply_matrix(&STRETCH_X, 2, 2, 0.0, 0.0)
        .map_err(|error| error.to_string())?;
    let first_ghost = faded_copy(&vector_family)?;
    let second_ghost = faded_copy(&first_vector_target_family)?;
    let source_family = scene
        .family(&[
            MobjectTarget::from(&square_family),
            MobjectTarget::from(&vector_family),
        ])
        .map_err(|error| error.to_string())?;
    let first_target_family = scene
        .family(&[
            MobjectTarget::from(&first_target_family),
            MobjectTarget::from(&first_vector_target_family),
        ])
        .map_err(|error| error.to_string())?;
    let second_target_family = scene
        .family(&[
            MobjectTarget::from(&second_target_family),
            MobjectTarget::from(&second_vector_target_family),
        ])
        .map_err(|error| error.to_string())?;

    let options = AnimationOptions::new()
        .run_time(0.5)
        .rate_func(RateFunction::Smooth);
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    {
        let mut live = scene.live(&mut session);
        let pause = live.wait_segment(0.25).map_err(|error| error.to_string())?;
        live.advance_segment_to(pause, pause.end_time())
            .map_err(|error| error.to_string())?;
        live.complete_segment(pause)
            .map_err(|error| error.to_string())?;
        let first = live
            .declare_and_activate_composition(
                &matrix_step(&source_family, &first_target_family, &first_ghost, options),
                AnimationOptions::new(),
            )
            .map_err(|error| error.to_string())?;
        live.advance_segment_to(first, first.end_time())
            .map_err(|error| error.to_string())?;
        live.complete_segment(first)
            .map_err(|error| error.to_string())?;

        let pause = live.wait_segment(0.25).map_err(|error| error.to_string())?;
        live.advance_segment_to(pause, pause.end_time())
            .map_err(|error| error.to_string())?;
        live.complete_segment(pause)
            .map_err(|error| error.to_string())?;
        live.declare_and_activate_composition(
            &matrix_step(
                &source_family,
                &second_target_family,
                &second_ghost,
                options,
            ),
            AnimationOptions::new(),
        )
        .map_err(|error| error.to_string())?;
    }
    session.seek(0.0).map_err(|error| error.to_string())?;
    Ok(session)
}

fn matrix_step<'a>(
    source: &'a crate::MobjectFamily,
    target_state: &'a crate::MobjectFamily,
    ghosts: &'a [crate::Mobject],
    options: AnimationOptions,
) -> AnimationCompositionRequest<'a> {
    let mut children = vec![AnimationCompositionRequest::FamilyTransformTo {
        source,
        target_state,
        options,
    }];
    children.extend(
        ghosts
            .iter()
            .map(|target| AnimationCompositionRequest::Add { target, options }),
    );
    AnimationCompositionRequest::Composition {
        kind: noon_core::SemanticAnimationCompositionKind::Parallel,
        options: AnimationOptions::new().rate_func(RateFunction::Linear),
        children,
    }
}

fn faded_copy(source: &crate::MobjectFamily) -> Result<Vec<crate::Mobject>, String> {
    let copy = source.copy_family().map_err(|error| error.to_string())?;
    let ghost = copy.root().clone();
    ghost.fade(0.7).map_err(|error| error.to_string())?;
    let store = ghost.integration_store();
    let leaves = store
        .borrow()
        .ordered_leaf_nodes(ghost.node_id())
        .map_err(|error| error.to_string())?;
    leaves
        .into_iter()
        .map(|node| {
            crate::Mobject::from_node(std::rc::Rc::clone(store), node)
                .map_err(|error| error.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::session;
    use noon_runtime::TimelineWakeState;

    fn present_count(frame: &noon_runtime::FrameState) -> usize {
        frame.presences.iter().filter(|present| **present).count()
    }

    #[test]
    fn vector_ghosts_enter_at_their_authored_boundaries_and_seek_matches_forward_playback() {
        let mut forward = session().unwrap();
        assert_eq!(forward.frame().objects.len(), 15);
        let initial = forward.frame().clone();
        let initial_count = present_count(&initial);

        forward.advance_to(0.25).unwrap();
        let after_first_ghost = forward.frame().clone();
        assert_eq!(present_count(&after_first_ghost), initial_count + 2);
        forward.advance_to(0.75).unwrap();
        let after_first_transform = forward.frame().clone();
        assert_eq!(present_count(&after_first_transform), initial_count + 2);
        forward.advance_to(1.0).unwrap();
        let after_second_ghost = forward.frame().clone();
        assert_eq!(present_count(&after_second_ghost), initial_count + 4);
        forward.advance_to(1.5).unwrap();
        let endpoint = forward.frame().clone();
        assert_eq!(present_count(&endpoint), initial_count + 4);
        assert_eq!(
            forward.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );

        // Seek the same execution/resource owner. Independently authored
        // sessions deliberately have distinct retained text arena identities.
        let mut sought = forward;
        for (time, expected) in [
            (0.0, initial),
            (0.25, after_first_ghost),
            (0.75, after_first_transform),
            (1.0, after_second_ghost),
            (1.5, endpoint),
        ] {
            sought.seek(time).unwrap();
            assert_eq!(
                sought.frame(),
                &expected,
                "frame differs at authored time {time}"
            );
        }
        assert_eq!(sought.wake_state().timeline(), TimelineWakeState::Quiescent);
    }
}
