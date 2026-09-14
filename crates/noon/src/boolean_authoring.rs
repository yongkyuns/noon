//! Boolean constructors snapshot selected geometry; results use the ordinary path API.
use crate::{
    AuthoringError, ExecutionSession, ManimGeometryOptions, Mobject, UnsupportedAuthoringOperation,
};
use noon_core::{SemanticObjectState, SemanticStore};
pub use noon_geometry::{BooleanOperation, BooleanPathError};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn boolean_options<E: From<AuthoringError>>(
    store: &SemanticStore,
    operation: BooleanOperation,
    operands: &[Mobject],
    snapshot: impl FnMut(&Mobject) -> Result<SemanticObjectState, E>,
) -> Result<ManimGeometryOptions, E> {
    let states = operands
        .iter()
        .map(snapshot)
        .collect::<Result<Vec<_>, _>>()?;
    let paths = states
        .iter()
        .map(|state| crate::path_editing::world_path(store, state).map_err(E::from))
        .collect::<Result<Vec<_>, _>>()?;
    let path = noon_geometry::boolean_paths(operation, &paths)
        .map_err(|error| E::from(AuthoringError::Boolean(error)))?;
    ManimGeometryOptions::path(path).map_err(E::from)
}

/// Prepare boolean geometry from one coherent execution publication.
///
/// Reachable operands contribute their effective affine transform while detached
/// operands retain authored transforms. Paint, priority, appearance, identity and
/// membership remain authored and are not copied into the detached boolean result.
pub fn effective_boolean_geometry_options(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    operation: BooleanOperation,
    operands: &[Mobject],
) -> Result<ManimGeometryOptions, AuthoringError> {
    let store_ref = store.borrow();
    execution
        .require_published_store(&store_ref)
        .map_err(AuthoringError::from)?;
    boolean_options(&store_ref, operation, operands, |object| {
        if !Rc::ptr_eq(store, object.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        object.validate()?;
        let mut state = object.state()?;
        if execution.semantic_object_is_reachable(object.node_id()) {
            let observed = execution
                .effective_semantic_object(&store_ref, object.node_id())
                .map_err(AuthoringError::from)?;
            if !observed.authored_content_layout_applicable() {
                return Err(AuthoringError::Unsupported(
                    UnsupportedAuthoringOperation::EffectivePathRenderOverride,
                ));
            }
            state.transform = crate::semantic_mobject::semantic_transform_with_effective_affine(
                state.transform,
                observed.object.transform,
            );
        }
        Ok(state)
    })
}

impl ManimGeometryOptions {
    /// Prepare a detached boolean result from authored world-space operands.
    /// No semantic identity or resource is allocated until these options are
    /// consumed by `Scene::geometry`. Result paint uses ordinary VMobject defaults;
    /// operand paint, membership, identity and content are unchanged.
    /// For current runtime geometry use `Scene::effective_boolean_geometry_options`.
    pub fn boolean_geometry(
        operation: BooleanOperation,
        operands: &[Mobject],
    ) -> Result<Self, AuthoringError> {
        let first =
            operands
                .first()
                .ok_or(AuthoringError::Boolean(BooleanPathError::OperandCount {
                    operation,
                    count: 0,
                }))?;
        let store = first.integration_store();
        boolean_options(&store.borrow(), operation, operands, |object| {
            if !std::rc::Rc::ptr_eq(store, object.integration_store()) {
                return Err(AuthoringError::ForeignStore);
            }
            object.state()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;

    #[test]
    fn scene_effective_boolean_requires_running_and_preserves_store_validation() {
        let mut scene = Scene::new();
        let a = scene.square(2.0).unwrap();
        let b = scene.square(2.0).unwrap();
        scene.add(&a).unwrap();
        scene.add(&b).unwrap();

        assert!(matches!(
            scene.effective_boolean_geometry_options(
                BooleanOperation::Union,
                &[a.clone(), b.clone()],
            ),
            Err(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::EffectiveStateUnavailable
            ))
        ));

        let execution = scene.execution_session().unwrap();
        scene.install_execution(execution);
        assert!(scene
            .effective_boolean_geometry_options(BooleanOperation::Union, &[a.clone(), b])
            .is_ok());

        let foreign_scene = Scene::new();
        let foreign = foreign_scene.square(1.0).unwrap();
        assert!(matches!(
            scene.effective_boolean_geometry_options(BooleanOperation::Union, &[a, foreign]),
            Err(AuthoringError::ForeignStore)
        ));
    }
}
