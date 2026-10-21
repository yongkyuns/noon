//! Ordered direct-member reads share the semantic store's count and rank index.
use crate::{AuthoringError, CompositeEntryHandle, MobjectFamily};
use std::rc::Rc;

impl MobjectFamily {
    /// Count direct members without materializing their ordered sequence.
    pub fn member_count(&self) -> Result<usize, AuthoringError> {
        self.integration_store()
            .borrow()
            .semantic_family_checked(self.node_id())
            .map(|node| node.member_count())
            .map_err(Into::into)
    }

    /// Count the root (only when it owns points) plus its direct members.
    pub fn split_count(&self) -> Result<usize, AuthoringError> {
        Ok(self.member_count()? + usize::from(self.get_num_points()? != 0))
    }

    /// Read a normalized split index against current points and membership.
    /// A host may normalize against an earlier count and mutate the root before
    /// this read. The current own-point predicate decides whether rank zero is self.
    pub fn split_at(&self, index: usize) -> Result<Option<CompositeEntryHandle>, AuthoringError> {
        if self.get_num_points()? != 0 {
            if index == 0 {
                return Ok(Some(CompositeEntryHandle::Family(self.clone())));
            }
            self.member_at(index - 1)
        } else {
            self.member_at(index)
        }
    }

    /// Snapshot self inclusion and direct members without allocating semantic nodes.
    pub fn split_members(&self) -> Result<Vec<CompositeEntryHandle>, AuthoringError> {
        let include_self = self.get_num_points()? != 0;
        let ids = self
            .integration_store()
            .borrow()
            .semantic_family_checked(self.node_id())?
            .members();
        let mut members = Vec::with_capacity(ids.len() + usize::from(include_self));
        if include_self {
            members.push(CompositeEntryHandle::Family(self.clone()));
        }
        for id in ids {
            members.push(CompositeEntryHandle::from_node(
                Rc::clone(self.integration_store()),
                id,
            )?);
        }
        Ok(members)
    }

    /// Read one current direct member through the shared rank index.
    ///
    /// This does not normalize negative indices or capture sibling membership.
    /// A caller can normalize against an earlier count, run a host callback,
    /// then observe the current member here. A missing rank returns `None`.
    pub fn member_at(&self, index: usize) -> Result<Option<CompositeEntryHandle>, AuthoringError> {
        let node = self
            .integration_store()
            .borrow()
            .semantic_family_member_at_checked(self.node_id(), index)
            .map_err(AuthoringError::from)?;
        node.map(|node| CompositeEntryHandle::from_node(Rc::clone(self.integration_store()), node))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;

    #[test]
    fn indexed_members_track_edits_without_replacing_roots_or_allocating_nodes() {
        let mut scene = Scene::new();
        let a = scene.square(0.2).unwrap();
        let b = scene.circle(0.3).unwrap();
        let nested = scene.family(&[(&a).into()]).unwrap();
        let group = scene.family(&[(&nested).into(), (&b).into()]).unwrap();
        let before = scene.integration_store().borrow().len();
        assert_eq!(group.member_count().unwrap(), 2);
        assert_eq!(
            group.member_at(0).unwrap(),
            Some(CompositeEntryHandle::Family(nested.clone()))
        );
        assert_eq!(
            group.member_at(1).unwrap(),
            Some(CompositeEntryHandle::Mobject(b.clone()))
        );
        assert!(group.member_at(2).unwrap().is_none());
        group.remove((&nested).into()).unwrap();
        assert_eq!(group.member_count().unwrap(), 1);
        assert_eq!(
            group.member_at(0).unwrap(),
            Some(CompositeEntryHandle::Mobject(b))
        );
        assert!(group.member_at(1).unwrap().is_none());
        group.add((&nested).into()).unwrap();
        assert_eq!(
            group.member_at(1).unwrap(),
            Some(CompositeEntryHandle::Family(nested))
        );
        assert_eq!(scene.integration_store().borrow().len(), before);
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        transaction.remove_node(group.node_id());
        transaction
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        assert!(group.member_count().is_err());
        assert!(group.member_at(0).is_err());
    }
}
