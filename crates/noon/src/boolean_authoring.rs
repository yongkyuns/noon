//! Boolean constructors snapshot selected geometry; results use the ordinary path API.
use crate::{AuthoringError, ManimGeometryOptions, Mobject};
use noon_core::{SemanticObjectState, SemanticStore};
pub use noon_geometry::{BooleanOperation, BooleanPathError};

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

impl ManimGeometryOptions {
    /// Prepare a detached boolean result from authored world-space operands.
    /// No semantic identity or resource is allocated until these options are
    /// consumed by `Scene::geometry`. Result paint uses ordinary VMobject defaults;
    /// operand paint, membership, identity and content are unchanged.
    /// For current runtime geometry use `LiveSession::boolean_geometry_options`.
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
