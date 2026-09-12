use super::*;
use crate::DashedVMobjectOptions;

impl LiveSession<'_> {
    /// Create one dashed copy from a coherent effective source publication.
    pub fn dashed_vmobject(
        &mut self,
        source: &Mobject,
        options: DashedVMobjectOptions,
    ) -> Result<Mobject, LiveSessionError> {
        self.require_mobject(source)?;
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let captured = self.capture_mobject_state(source)?;
        let mut store = self.store.borrow_mut();
        let (mut state, path) =
            crate::dashed_vmobject_authoring::prepare_dashed_vmobject(&store, &captured, options)?;
        let result = store.with_geometry_path(path, |store, handle| {
            state.content = noon_core::StoredGeometry::Resource(handle).into();
            self.session
                .apply_semantic_transaction_at_root(
                    store,
                    self.root,
                    crate::path_editing::subcurve_creation(state),
                )
                .map_err(LiveSessionError::from)
        })?;
        let id = crate::path_editing::created_subcurve_id(&result);
        drop(store);
        Mobject::from_node(Rc::clone(self.store), id).map_err(LiveSessionError::from)
    }
}
