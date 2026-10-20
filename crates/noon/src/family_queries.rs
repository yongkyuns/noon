//! Ordered direct-member reads share the semantic store's count and rank index.
use crate::{AuthoringError, CompositeEntryHandle, MobjectFamily};
use std::rc::Rc;

/// A retained direct-member list view, independent of any language frontend.
/// In-place edits stay visible; adding/prepending replaces the member container.
#[derive(Clone, Debug)]
pub struct FamilyMemberView {
    store: Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    view: noon_core::SemanticMemberView,
}
impl FamilyMemberView {
    pub fn len(&self) -> Result<usize, AuthoringError> {
        self.view
            .len(&self.store.borrow())
            .map_err(member_view_error)
    }
    pub fn is_empty(&self) -> Result<bool, AuthoringError> {
        self.len().map(|len| len == 0)
    }
    pub fn member_at(&self, index: usize) -> Result<Option<CompositeEntryHandle>, AuthoringError> {
        self.view
            .member_at(&self.store.borrow(), index)
            .map_err(member_view_error)?
            .map(|node| CompositeEntryHandle::from_node(Rc::clone(&self.store), node))
            .transpose()
    }
}
fn member_view_error(error: noon_core::SemanticMemberViewError) -> AuthoringError {
    match error {
        noon_core::SemanticMemberViewError::ForeignStore => AuthoringError::ForeignStore,
        noon_core::SemanticMemberViewError::Store(error) => error.into(),
    }
}

impl MobjectFamily {
    /// Capture direct-member container identity without copying its order.
    pub fn member_view(&self) -> Result<FamilyMemberView, AuthoringError> {
        self.validate()?;
        let view = self
            .integration_store()
            .borrow_mut()
            .member_view(self.node_id())?;
        Ok(FamilyMemberView {
            store: Rc::clone(self.integration_store()),
            view,
        })
    }

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
    fn retained_member_lists_follow_native_authoring_and_live_publication() {
        for running in [false, true] {
            let mut scene = Scene::new();
            let a = scene.square(0.2).unwrap();
            let b = scene.circle(0.3).unwrap();
            let c = scene.circle(0.4).unwrap();
            let family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
            scene.add_many(&[(&family).into()]).unwrap();
            if running {
                let execution = scene.execution_session().unwrap();
                scene.install_execution(execution);
            }
            let old = family.member_view().unwrap();
            let revision = scene.revision();
            if running {
                scene
                    .owned_live()
                    .add_family_members(&family, &[(&c).into()])
                    .unwrap();
            } else {
                family.add_many(&[(&c).into()]).unwrap();
            }
            assert_eq!(scene.revision(), revision.checked_next().unwrap());
            assert_eq!(old.len().unwrap(), 2);
            let current = family.member_view().unwrap();
            assert_eq!(current.len().unwrap(), 3);
            if running {
                scene
                    .owned_live()
                    .remove_family_members(&family, &[(&a).into()])
                    .unwrap();
            } else {
                family.remove_many(&[(&a).into()]).unwrap();
            }
            assert_eq!(current.len().unwrap(), 2);
            assert_eq!(
                old.member_at(0).unwrap().unwrap().as_target().node_id(),
                a.node_id()
            );
            assert_eq!(
                current.member_at(0).unwrap().unwrap().as_target().node_id(),
                b.node_id()
            );
            let revision = scene.revision();
            if running {
                assert!(scene
                    .owned_live()
                    .add_family_members(&family, &[(&family).into()])
                    .is_err());
            } else {
                assert!(family.add_many(&[(&family).into()]).is_err());
            }
            assert_eq!(scene.revision(), revision);
            assert_eq!(current.len().unwrap(), 2);
            // Even an empty add replaces the list; a later in-place removal
            // must leave this captured list unchanged.
            if running {
                scene.owned_live().add_family_members(&family, &[]).unwrap();
            } else {
                family.add_many(&[]).unwrap();
            }
            if running {
                scene
                    .owned_live()
                    .remove_family_members(&family, &[(&b).into()])
                    .unwrap();
            } else {
                family.remove_many(&[(&b).into()]).unwrap();
            }
            assert_eq!(current.len().unwrap(), 2);
            assert_eq!(family.member_count().unwrap(), 1);
            if running {
                family
                    .effective_own_path_query(scene.owned_execution())
                    .unwrap();
            }
        }
    }

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
