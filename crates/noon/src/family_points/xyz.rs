//! XYZ family payloads reuse the same identity and path publication as leaves.
use super::*;
use noon_core::SemanticVec3;

pub fn publish_family_points_3d(
    store: &Rc<std::cell::RefCell<SemanticStore>>,
    root: noon_core::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    family: &MobjectFamily,
    points: &[SemanticVec3],
) -> Result<(), AuthoringError> {
    crate::MobjectTarget::from(family).require_store(store)?;
    if let Some(object) = family.own_object()? {
        return crate::path_editing::publish_running_object_edit(
            store,
            root,
            execution,
            &object,
            crate::path_editing::PathEdit::RawPoints3d(points),
        );
    }
    crate::path_editing::require_running_path_resource_admission(store, root, execution)?;
    let path = crate::path_editing::raw_points_path_3d(points)?;
    publish_initial_family_path(
        &mut store.borrow_mut(),
        family.node_id(),
        path,
        |store, tx| {
            execution
                .apply_semantic_transaction_at_root(store, root, tx)
                .map(|_| ())
                .map_err(AuthoringError::from)
        },
    )
}

pub fn publish_append_family_points_3d(
    store: &Rc<std::cell::RefCell<SemanticStore>>,
    root: noon_core::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    family: &MobjectFamily,
    points: &[SemanticVec3],
) -> Result<(), AuthoringError> {
    crate::MobjectTarget::from(family).require_store(store)?;
    let own = family.own_object()?;
    if points.is_empty() {
        crate::path_editing::require_running_path_resource_admission(store, root, execution)?;
        return Ok(());
    }
    match own {
        Some(object) => crate::path_editing::publish_running_object_edit(
            store,
            root,
            execution,
            &object,
            crate::path_editing::PathEdit::AppendRawPoints3d(points),
        ),
        None => publish_family_points_3d(store, root, execution, family, points),
    }
}

impl MobjectFamily {
    pub fn set_points_3d(&self, points: &[SemanticVec3]) -> Result<(), AuthoringError> {
        if let Some(mut object) = self.own_object()? {
            return object.set_points_3d(points);
        }
        let path = crate::path_editing::raw_points_path_3d(points)?;
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

    pub fn append_points_3d(&self, points: &[SemanticVec3]) -> Result<(), AuthoringError> {
        match self.own_object()? {
            Some(mut object) => object.append_points_3d(points),
            None if points.is_empty() => Ok(()),
            None => self.set_points_3d(points),
        }
    }
}

impl crate::Scene {
    pub fn set_family_points_3d(
        &mut self,
        family: &MobjectFamily,
        points: &[SemanticVec3],
    ) -> Result<(), AuthoringError> {
        self.family_points_3d(family, points, false)
    }

    pub fn append_family_points_3d(
        &mut self,
        family: &MobjectFamily,
        points: &[SemanticVec3],
    ) -> Result<(), AuthoringError> {
        self.family_points_3d(family, points, true)
    }

    fn family_points_3d(
        &mut self,
        family: &MobjectFamily,
        points: &[SemanticVec3],
        append: bool,
    ) -> Result<(), AuthoringError> {
        crate::MobjectTarget::from(family).require_store(self.integration_store())?;
        if self.running_execution().is_some() {
            self.with_running_execution(|store, root, execution| {
                if append {
                    publish_append_family_points_3d(store, root, execution, family, points)
                } else {
                    publish_family_points_3d(store, root, execution, family, points)
                }
            })
        } else if append {
            family.append_points_3d(points)
        } else {
            family.set_points_3d(points)
        }
    }
}

#[cfg(test)]
mod tests;
