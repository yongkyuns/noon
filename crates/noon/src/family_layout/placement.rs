//! Observe only authored or proven settled-callback affine placement.
use super::*;

pub(super) fn transform(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    object: &Mobject,
    allow_settled_callback: bool,
) -> Result<(Transform2D, bool), AuthoringError> {
    if !Rc::ptr_eq(store, object.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    let authored = object.state()?;
    let authored_transform = Transform2D {
        translation: authored.transform.translation.lower_xy_f32()?,
        rotation: authoring_render_f64(
            "move_to authored rotation",
            authored
                .transform
                .planar_rotation()
                .ok_or(AuthoringError::NonFiniteObjectState)?,
        )? as f32,
        scale: authored.transform.scale.lower_xy_f32()?,
    };
    let store_ref = store.borrow();
    match execution.effective_semantic_object(&store_ref, object.node_id()) {
        Ok(observed) if !observed.authored_content_layout_applicable() => {
            return Err(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::PlacementRenderOverride,
            ));
        }
        Ok(observed) if observed.object.transform != authored_transform => {
            if allow_settled_callback
                && execution.placement_uses_settled_callback_transform(
                    &store_ref,
                    object.node_id(),
                    authored_transform,
                    observed.object.transform,
                )
            {
                return Ok((observed.object.transform, true));
            }
            return Err(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver,
            ));
        }
        Ok(_) | Err(ExecutionSessionPublicationError::UnknownObject(_)) => {}
        Err(error) => return Err(error.into()),
    }
    Ok((authored_transform, false))
}
