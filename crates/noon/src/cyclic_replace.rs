//! Shared Manim-style cyclic target construction for flat semantic families.

use crate::{
    AuthoringError, FamilyCopy, Mobject, MobjectFamily, MobjectTarget, Scene,
    UnsupportedAuthoringOperation,
};
use noon_core::{
    SemanticMutationTransaction, SemanticNodeId, SemanticNodeKind, SemanticObjectProperty,
    SemanticVec3,
};
use std::rc::Rc;

fn direct_object_members(family: &MobjectFamily) -> Result<Vec<SemanticNodeId>, AuthoringError> {
    family.validate()?;
    let store = family.integration_store().borrow();
    let members = store
        .semantic_family_members_checked(family.node_id())
        .map_err(AuthoringError::from)?
        .to_vec();
    if members.is_empty()
        || members.iter().any(|member| {
            !store
                .node(*member)
                .is_some_and(|node| matches!(node.kind(), SemanticNodeKind::AuthoringObject))
        })
    {
        return Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::CyclicReplaceFamilyTopology,
        ));
    }
    Ok(members)
}

fn stage_cyclic_positions(
    transaction: &mut SemanticMutationTransaction,
    pending: &crate::family_copy::PendingSemanticCopy,
    positions: &[(SemanticNodeId, (f64, f64), SemanticVec3)],
) {
    for (index, &(member, from, mut translation)) in positions.iter().enumerate() {
        let (_, to, _) = positions[(index + 1) % positions.len()];
        translation.x += to.0 - from.0;
        translation.y += to.1 - from.1;
        transaction.set_property(
            pending.copied_token(member),
            SemanticObjectProperty::Translation,
            translation,
        );
    }
}

fn members(family: &MobjectFamily) -> Result<Vec<Mobject>, AuthoringError> {
    direct_object_members(family)?
        .into_iter()
        .map(|member| Mobject::from_node(Rc::clone(family.integration_store()), member))
        .collect()
}

impl MobjectFamily {
    /// Copy this flat family and cyclically move each copied member to the next
    /// authored member position, matching ManimCE `CyclicReplace` target creation.
    pub fn cyclic_replace_target(&self) -> Result<FamilyCopy, AuthoringError> {
        self.cyclic_replace_target_with_references(&[])
    }

    /// Preserve detached wrapper metadata references in the same atomic copy used
    /// by ordinary family copying, then apply the shared cyclic placement.
    pub fn cyclic_replace_target_with_references(
        &self,
        references: &[MobjectTarget<'_>],
    ) -> Result<FamilyCopy, AuthoringError> {
        let members = members(self)?;
        let positions = members
            .iter()
            .map(|member| {
                Ok((
                    member.node_id(),
                    member.center()?,
                    member.state()?.transform.translation,
                ))
            })
            .collect::<Result<Vec<_>, AuthoringError>>()?;
        let (mut transaction, pending) =
            crate::family_copy::prepare_authored_copy(self.into(), references)?;
        stage_cyclic_positions(&mut transaction, &pending, &positions);
        let result = transaction.apply(&mut self.integration_store().borrow_mut())?;
        pending.resolve(&result, MobjectFamily::from_node)
    }
}

impl Scene {
    /// Capture the current family and rotate the copied members' positions.
    pub fn cyclic_replace_target(
        &mut self,
        family: &MobjectFamily,
    ) -> Result<FamilyCopy, AuthoringError> {
        self.cyclic_replace_target_with_references(family, &[])
    }

    pub fn cyclic_replace_target_with_references(
        &mut self,
        family: &MobjectFamily,
        references: &[MobjectTarget<'_>],
    ) -> Result<FamilyCopy, AuthoringError> {
        MobjectTarget::from(family).require_store(self.integration_store())?;
        let Some((store, root, execution)) = crate::scene::host_execution::parts_mut(self) else {
            return family.cyclic_replace_target_with_references(references);
        };
        crate::effective_capture::require_target_capture(execution, &store.borrow())?;
        let members = members(family)?;
        let mut positions = members
            .iter()
            .map(|member| {
                let center =
                    crate::family_layout::effective_mobject_layout(store, execution, member)?
                        .center;
                Ok((member.node_id(), center, SemanticVec3::ZERO))
            })
            .collect::<Result<Vec<_>, AuthoringError>>()?;
        let indices: std::collections::BTreeMap<_, _> = positions
            .iter()
            .enumerate()
            .map(|(index, (node, _, _))| (*node, index))
            .collect();
        let (mut transaction, pending) = crate::family_copy::prepare_running_copy(
            store,
            execution,
            family.into(),
            references,
            |object, state| {
                if let Some(&index) = indices.get(&object.node_id()) {
                    positions[index].2 = state.transform.translation;
                }
            },
        )?;
        stage_cyclic_positions(&mut transaction, &pending, &positions);
        let result = Scene::publish_running_transaction(store, root, execution, transaction)?;
        pending.resolve(&result, MobjectFamily::from_node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyclic_target_publishes_copy_and_placement_as_one_revision() {
        let mut scene = Scene::new();
        let first = scene.circle(1.0).unwrap();
        let mut second = scene.square(1.0).unwrap();
        second.shift(3.0, 1.0).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let mut running = scene.into_execution().unwrap();
        let revision = running.scene().revision();

        let copied = running.scene_mut().cyclic_replace_target(&family).unwrap();

        assert_eq!(running.scene().revision(), revision.checked_next().unwrap());
        assert_eq!(
            running.session().publication_context().scene_revision(),
            running.scene().revision()
        );
        assert_eq!(
            copied.mobject(&first).unwrap().center().unwrap(),
            second.center().unwrap()
        );
        assert_eq!(
            copied.mobject(&second).unwrap().center().unwrap(),
            first.center().unwrap()
        );
    }
}
