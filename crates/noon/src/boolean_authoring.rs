//! Boolean constructors snapshot selected geometry; results use the ordinary path API.
use crate::{AuthoringError, ManimGeometryOptions, Mobject};
use noon_core::{SemanticObjectState, SemanticStore};
pub use noon_geometry::{BooleanOperation, BooleanPathError};
use std::{cell::RefCell, rc::Rc};

/// Prepare one inert boolean result while leaving authored-vs-effective snapshot policy to the
/// caller. Provenance, handle validation, world-path extraction and boolean construction are shared.
pub(crate) fn prepare_boolean_options<E>(
    store: &Rc<RefCell<SemanticStore>>,
    operation: BooleanOperation,
    operands: &[Mobject],
    mut foreign_store: impl FnMut() -> E,
    mut snapshot: impl FnMut(
        &SemanticStore,
        &Mobject,
        SemanticObjectState,
    ) -> Result<SemanticObjectState, E>,
) -> Result<ManimGeometryOptions, E>
where
    E: From<AuthoringError>,
{
    let store_ref = store.borrow();
    let paths = operands
        .iter()
        .map(|object| {
            if !Rc::ptr_eq(store, object.integration_store()) {
                return Err(foreign_store());
            }
            let state = object.state().map_err(E::from)?;
            let state = snapshot(&store_ref, object, state)?;
            crate::path_editing::world_path(&store_ref, &state).map_err(E::from)
        })
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
        prepare_boolean_options(
            first.integration_store(),
            operation,
            operands,
            || AuthoringError::ForeignStore,
            |_, _, state| Ok(state),
        )
    }
}
