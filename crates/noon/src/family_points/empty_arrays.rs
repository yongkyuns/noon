//! Zero-row array shapes use the existing shared path publication transaction.
use super::*;
use noon_core::PointArrayShape;

impl MobjectFamily {
    pub fn set_empty_point_array(&self, shape: PointArrayShape) -> Result<(), AuthoringError> {
        let path = VectorPath::from_empty_point_array(shape.clone())?;
        if let Some(mut object) = self.own_object()? {
            return object.set_empty_point_array(shape);
        }
        publish_initial_family_path(
            &mut self.integration_store().borrow_mut(),
            self.node_id(),
            path,
            |store, transaction| {
                transaction
                    .apply(store)
                    .map(|_| ())
                    .map_err(AuthoringError::from)
            },
        )
    }
}

impl crate::MobjectTarget<'_> {
    pub fn set_empty_point_array(&self, shape: PointArrayShape) -> Result<(), AuthoringError> {
        match self {
            Self::Object(object) => Mobject::clone(object).set_empty_point_array(shape),
            Self::Family(family) => family.set_empty_point_array(shape),
        }
    }
}

pub fn publish_empty_family_point_array(
    store: &Rc<std::cell::RefCell<SemanticStore>>,
    root: noon_core::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    family: &MobjectFamily,
    shape: &PointArrayShape,
) -> Result<(), AuthoringError> {
    crate::MobjectTarget::from(family).require_store(store)?;
    let path = VectorPath::from_empty_point_array(shape.clone())?;
    if let Some(object) = family.own_object()? {
        return crate::path_editing::publish_running_object_edit(
            store,
            root,
            execution,
            &object,
            crate::path_editing::PathEdit::EmptyPointArray(shape),
        );
    }
    crate::path_editing::require_running_path_resource_admission(store, root, execution)?;
    publish_initial_family_path(
        &mut store.borrow_mut(),
        family.node_id(),
        path,
        |store, transaction| {
            execution
                .apply_semantic_transaction_at_root(store, root, transaction)
                .map(|_| ())
                .map_err(AuthoringError::from)
        },
    )
}

impl crate::LiveSession<'_> {
    pub fn set_empty_point_array(
        &mut self,
        target: crate::MobjectTarget<'_>,
        shape: PointArrayShape,
    ) -> Result<(), crate::LiveSessionError> {
        // The typed target supplies the same store, root and publication owner
        // used by every ordinary path edit; no separate array mutation lane.
        match target {
            crate::MobjectTarget::Object(object) => {
                self.set_empty_mobject_point_array(object, &shape)
            }
            crate::MobjectTarget::Family(family) => {
                self.set_empty_family_point_array(family, &shape)
            }
        }
    }
}
