use super::{
    HostCallbackId, SemanticNodeId, SemanticNodeKind, SemanticSceneOperationError, SemanticStore,
    SemanticUpdaterRegistration, SemanticUpdaterRegistrationError,
};

impl SemanticStore {
    /// Return authored host-updater registrations for one target semantic node.
    ///
    /// Registration order is semantic authoring state. The host owns executable
    /// callable code keyed by [`HostCallbackId`]; Runtime scheduling is a lowering
    /// concern and is deliberately absent here.
    pub fn semantic_updater_registrations(
        &self,
        target: SemanticNodeId,
    ) -> Result<&[SemanticUpdaterRegistration], SemanticSceneOperationError> {
        target_node_checked(self, target)?;
        Ok(self
            .node(target)
            .expect("semantic updater target validated above")
            .host_updaters())
    }

    pub(crate) fn insert_semantic_updater_registration(
        &mut self,
        target: SemanticNodeId,
        registration: SemanticUpdaterRegistration,
        position: Option<usize>,
    ) -> Result<(), UpdaterRegistrationEditError> {
        let registrations = self
            .node_mut(target)
            .expect("semantic updater target validated above")
            .host_updaters_mut();
        insert_updater_registration(registrations, registration, position)
    }

    pub(crate) fn close_first_semantic_updater_registration(
        &mut self,
        target: SemanticNodeId,
        callback: HostCallbackId,
        inactive_from: f64,
    ) -> Result<bool, UpdaterRegistrationEditError> {
        let registrations = self
            .node_mut(target)
            .expect("semantic updater target validated above")
            .host_updaters_mut();
        close_first_updater_registration(registrations, callback, inactive_from)
    }

    pub(crate) fn close_all_semantic_updater_registrations(
        &mut self,
        target: SemanticNodeId,
        inactive_from: f64,
    ) -> Result<bool, UpdaterRegistrationEditError> {
        let registrations = self
            .node_mut(target)
            .expect("semantic updater target validated above")
            .host_updaters_mut();
        close_all_updater_registrations(registrations, inactive_from)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdaterRegistrationEditError {
    InvalidActivationInterval,
    PositionOutOfBounds { position: usize, active: usize },
}

pub(crate) fn insert_updater_registration(
    registrations: &mut Vec<SemanticUpdaterRegistration>,
    registration: SemanticUpdaterRegistration,
    position: Option<usize>,
) -> Result<(), UpdaterRegistrationEditError> {
    let active_positions = registrations
        .iter()
        .enumerate()
        .filter_map(|(index, existing)| {
            existing
                .is_active_at(registration.active_from())
                .then_some(index)
        })
        .collect::<Vec<_>>();
    let position = position.unwrap_or(active_positions.len());
    if position > active_positions.len() {
        return Err(UpdaterRegistrationEditError::PositionOutOfBounds {
            position,
            active: active_positions.len(),
        });
    }
    let insertion = active_positions
        .get(position)
        .copied()
        .or_else(|| active_positions.last().map(|index| index + 1))
        .unwrap_or_else(|| {
            registrations
                .iter()
                .position(|existing| existing.active_from() > registration.active_from())
                .unwrap_or(registrations.len())
        });
    registrations.insert(insertion, registration);
    Ok(())
}

pub(crate) fn close_first_updater_registration(
    registrations: &mut [SemanticUpdaterRegistration],
    callback: HostCallbackId,
    inactive_from: f64,
) -> Result<bool, UpdaterRegistrationEditError> {
    let Some(registration) = registrations.iter_mut().find(|registration| {
        registration.callback() == callback
            && registration.is_open()
            && registration.active_from() <= inactive_from
    }) else {
        return Ok(false);
    };
    registration.close(inactive_from).map_err(
        |SemanticUpdaterRegistrationError::InvalidActivationInterval| {
            UpdaterRegistrationEditError::InvalidActivationInterval
        },
    )?;
    Ok(true)
}

pub(crate) fn close_all_updater_registrations(
    registrations: &mut [SemanticUpdaterRegistration],
    inactive_from: f64,
) -> Result<bool, UpdaterRegistrationEditError> {
    if !inactive_from.is_finite() || inactive_from < 0.0 {
        return Err(UpdaterRegistrationEditError::InvalidActivationInterval);
    }
    let mut changed = false;
    for registration in registrations.iter_mut().filter(|registration| {
        registration.is_open() && registration.active_from() <= inactive_from
    }) {
        registration
            .close(inactive_from)
            .expect("all open updater intervals validated above");
        changed = true;
    }
    Ok(changed)
}

fn target_node_checked(
    store: &SemanticStore,
    id: SemanticNodeId,
) -> Result<(), SemanticSceneOperationError> {
    let node = store
        .node(id)
        .ok_or(SemanticSceneOperationError::UnknownNode(id))?;
    let is_target = match node.kind() {
        SemanticNodeKind::Family => true,
        SemanticNodeKind::AuthoringObject => node.semantic_object_state().is_some(),
        SemanticNodeKind::Object(_)
        | SemanticNodeKind::Signal(_)
        | SemanticNodeKind::Animation(_) => false,
    };
    if !is_target {
        return Err(SemanticSceneOperationError::NotSemanticAuthoringNode(id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SemanticMutationImpact, SemanticMutationStats, SemanticMutationTransaction,
        SemanticMutationTransactionError, SemanticNodeCreation, SemanticObjectProperty,
        SemanticObjectState, StoredGeometry,
    };

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    fn add(
        store: &mut SemanticStore,
        target: SemanticNodeId,
        callback: HostCallbackId,
        active_from: f64,
        position: Option<usize>,
    ) {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(target, callback, active_from, position);
        transaction.apply(store).unwrap();
    }

    #[test]
    fn updater_occurrences_are_ordered_scene_owned_authoring_state() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let first = HostCallbackId::new(7);
        let second = HostCallbackId::new(3);

        add(&mut store, target, first, 0.0, None);
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        add(&mut store, target, second, 0.0, Some(0));
        add(&mut store, target, first, 1.0, None);

        assert_eq!(
            store
                .semantic_updater_registrations(target)
                .unwrap()
                .iter()
                .map(|registration| registration.callback())
                .collect::<Vec<_>>(),
            vec![second, first, first]
        );
        assert_eq!(
            store
                .semantic_updater_registrations(target)
                .unwrap()
                .iter()
                .filter(|registration| registration.is_active_at(0.0))
                .count(),
            2
        );
        assert_eq!(store.last_mutation_stats().slots_written, 1);
    }

    #[test]
    fn updater_declarations_survive_detach_and_readd() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let callback = HostCallbackId::new(11);
        add(&mut store, target, callback, 0.0, None);

        store.attach_semantic_object(target).unwrap();
        store.detach_semantic_object(target).unwrap();
        assert_eq!(
            store.semantic_updater_registrations(target).unwrap()[0].callback(),
            callback
        );
        store.attach_semantic_object(target).unwrap();
        assert_eq!(
            store.semantic_updater_registrations(target).unwrap()[0].callback(),
            callback
        );
    }

    #[test]
    fn semantic_families_can_own_updater_declarations() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let callback = HostCallbackId::new(4);

        add(&mut store, family, callback, 0.0, None);
        assert_eq!(
            store.semantic_updater_registrations(family).unwrap()[0].callback(),
            callback
        );
    }

    #[test]
    fn state_less_and_stale_targets_are_rejected_before_mutation() {
        let mut store = SemanticStore::new();
        let identity_only = store.insert_authoring_object();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(identity_only, HostCallbackId::new(1), 0.0, None);
        assert!(transaction.apply(&mut store).is_err());

        let stale = object(&mut store, 2.0);
        store.remove_node(stale).unwrap();
        assert_eq!(
            store.semantic_updater_registrations(stale),
            Err(SemanticSceneOperationError::UnknownNode(stale))
        );
    }

    #[test]
    fn removing_updater_is_local_order_preserving_and_idempotent() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let first = HostCallbackId::new(1);
        let removed = HostCallbackId::new(2);
        let last = HostCallbackId::new(3);
        for callback in [first, removed, last, removed] {
            add(&mut store, target, callback, 0.0, None);
        }
        add(&mut store, target, removed, 10.0, None);

        let mut remove = SemanticMutationTransaction::new();
        remove.remove_updater(target, removed, 2.0);
        let result = remove.apply(&mut store).unwrap();
        assert_eq!(result.impacts().len(), 1);
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        let registrations = store.semantic_updater_registrations(target).unwrap();
        assert_eq!(registrations.len(), 5);
        assert_eq!(registrations[1].inactive_from(), Some(2.0));
        assert_eq!(registrations[3].inactive_from(), None);
        assert_eq!(registrations[4].active_from(), 10.0);

        let mut clear = SemanticMutationTransaction::new();
        clear.clear_updaters(target, 4.0);
        clear.apply(&mut store).unwrap();
        assert_eq!(
            store
                .semantic_updater_registrations(target)
                .unwrap()
                .iter()
                .filter(|registration| registration.is_active_at(4.0))
                .count(),
            0
        );
        assert_eq!(
            store.semantic_updater_registrations(target).unwrap()[4].inactive_from(),
            None
        );

        let revision = store.scene_revision();
        let mut absent = SemanticMutationTransaction::new();
        absent.remove_updater(target, removed, 5.0);
        let result = absent.apply(&mut store).unwrap();
        assert!(result.impacts().is_empty());
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(
            store.last_mutation_stats(),
            SemanticMutationStats::default()
        );
    }

    #[test]
    fn invalid_late_updater_interval_rolls_back_prior_property_write() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let before = store
            .node(target)
            .and_then(|node| node.semantic_object_state())
            .unwrap()
            .clone();
        let revision = store.scene_revision();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_property(target, SemanticObjectProperty::ObjectOpacity, 0.25);
        transaction.add_updater(target, HostCallbackId::new(99), f64::NAN, None);

        assert!(transaction.apply(&mut store).is_err());
        assert_eq!(
            store
                .node(target)
                .and_then(|node| node.semantic_object_state())
                .unwrap(),
            &before
        );
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(
            store.last_mutation_stats(),
            SemanticMutationStats::default()
        );

        let mut out_of_bounds = SemanticMutationTransaction::new();
        out_of_bounds.set_property(target, SemanticObjectProperty::ObjectOpacity, 0.5);
        out_of_bounds.add_updater(target, HostCallbackId::new(99), 0.0, Some(1));
        assert!(matches!(
            out_of_bounds.apply(&mut store),
            Err(
                SemanticMutationTransactionError::UpdaterPositionOutOfBounds {
                    index: 1,
                    position: 1,
                    active: 0,
                    ..
                }
            )
        ));
        assert_eq!(
            store
                .node(target)
                .and_then(|node| node.semantic_object_state())
                .unwrap(),
            &before
        );
        assert_eq!(store.scene_revision(), revision);
    }

    #[test]
    fn pending_family_can_receive_updater_in_the_same_transaction() {
        let mut store = SemanticStore::new();
        let mut transaction = SemanticMutationTransaction::new();
        let family = transaction.create_node(SemanticNodeCreation::family());
        transaction.add_updater(family, HostCallbackId::new(21), 0.0, None);

        let result = transaction.apply(&mut store).unwrap();
        let family = result.resolve(family).unwrap();
        assert_eq!(store.scene_revision().get(), 1);
        assert!(result
            .impacts()
            .contains(&SemanticMutationImpact::NodeAdded { node: family }));
        assert!(result
            .impacts()
            .contains(&SemanticMutationImpact::UpdaterRegistrations { target: family }));
        assert_eq!(
            store.semantic_updater_registrations(family).unwrap()[0].callback(),
            HostCallbackId::new(21)
        );
    }

    #[test]
    fn updater_mutation_does_not_touch_unrelated_semantic_nodes() {
        let mut store = SemanticStore::new();
        let targets = (0..1_000)
            .map(|index| object(&mut store, index as f32 + 1.0))
            .collect::<Vec<_>>();
        let target = targets[500];

        add(&mut store, target, HostCallbackId::new(99), 0.0, None);
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert!(targets.iter().enumerate().all(|(index, id)| {
            index == 500 || store.node(*id).unwrap().host_updaters().is_empty()
        }));
    }
}
