//! Shared cold/live observations for Brace geometry and persistent label placement.
use super::*;
use crate::ExecutionSession;

pub(crate) fn prepare_geometry(
    store: &Rc<RefCell<SemanticStore>>,
    execution: Option<&ExecutionSession>,
    target: &LayoutAnchor,
    label: Option<&LayoutAnchor>,
    options: BraceOptions,
) -> Result<PreparedBraceGeometry, AuthoringError> {
    require_anchor_store(store, target)?;
    if let Some(execution) = execution {
        execution.require_published_store(&store.borrow())?;
    }
    if let Some(label) = label {
        require_label_placement(store, execution, label)?;
    }
    crate::geometry_authoring::prepare_brace_geometry_with_transform(
        target,
        options.direction,
        options.buff,
        options.sharpness,
        |node, authored| {
            let mut transform = authored
                .transform
                .as_planar()
                .ok_or(AuthoringError::NonFiniteObjectState)?;
            if let Some(execution) =
                execution.filter(|execution| execution.semantic_object_is_reachable(node))
            {
                let store = store.borrow();
                let effective = execution.effective_semantic_object(&store, node)?;
                if !effective.authored_content_layout_applicable() {
                    return Err(AuthoringError::Unsupported(
                        crate::UnsupportedAuthoringOperation::EffectiveFamilyLayoutRenderOverride,
                    ));
                }
                let value = effective.object.transform;
                let retain = |old: f64, new: f32| {
                    if old as f32 == new {
                        old
                    } else {
                        f64::from(new)
                    }
                };
                transform.translation.x = retain(transform.translation.x, value.translation.x);
                transform.translation.y = retain(transform.translation.y, value.translation.y);
                transform.scale.x = retain(transform.scale.x, value.scale.x);
                transform.scale.y = retain(transform.scale.y, value.scale.y);
                transform.rotation_z = retain(transform.rotation_z, value.rotation);
            }
            Ok(transform)
        },
    )
}

pub(crate) fn require_label_placement(
    store: &Rc<RefCell<SemanticStore>>,
    execution: Option<&ExecutionSession>,
    label: &LayoutAnchor,
) -> Result<(), AuthoringError> {
    require_anchor_store(store, label)?;
    if let Some(execution) = execution {
        execution.require_published_store(&store.borrow())?;
        for &node in label.layout()?.leaves() {
            crate::family_layout::placement_authored_transform(
                store,
                execution,
                &Mobject::from_node(Rc::clone(store), node)?,
            )?;
        }
    }
    Ok(())
}
