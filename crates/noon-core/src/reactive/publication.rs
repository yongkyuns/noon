use serde::{Deserialize, Serialize};

macro_rules! define_publication_revision {
    ($(#[$meta:meta])* $name:ident) => {
        #[derive(
            Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize,
        )]
        $(#[$meta])*
        pub struct $name(u64);

        impl $name {
            pub const fn new(raw: u64) -> Self {
                Self(raw)
            }

            pub const fn get(self) -> u64 {
                self.0
            }

            pub const fn checked_next(self) -> Option<Self> {
                match self.0.checked_add(1) {
                    Some(next) => Some(Self(next)),
                    None => None,
                }
            }
        }
    };
}

define_publication_revision!(
    /// Revision of the coherently published authoritative Semantic Scene.
    ///
    /// Semantic identity generations are intentionally a separate domain: replacing a
    /// node may change identity without defining a scene publication clock, while one
    /// atomic semantic transaction may touch several identities and still publish only
    /// one scene revision.
    SceneRevision
);

define_publication_revision!(
    /// Revision of the coherently published derived execution projection.
    ///
    /// This changes when executable schedule/dependency/resource mapping changes, not
    /// merely because authored time or an effective reactive value advances.
    ExecutionRevision
);

define_publication_revision!(
    /// Epoch of the latest coherent effective runtime frame publication.
    ///
    /// A pure timeline/reactive/host update may advance this epoch while both authored
    /// scene and execution revisions remain unchanged.
    FrameEpoch
);

/// Exact semantic/execution/effective context carried by one published frame view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublicationContext {
    scene_revision: SceneRevision,
    execution_revision: ExecutionRevision,
    frame_epoch: FrameEpoch,
}

impl PublicationContext {
    pub const fn new(
        scene_revision: SceneRevision,
        execution_revision: ExecutionRevision,
        frame_epoch: FrameEpoch,
    ) -> Self {
        Self {
            scene_revision,
            execution_revision,
            frame_epoch,
        }
    }

    pub const fn scene_revision(self) -> SceneRevision {
        self.scene_revision
    }

    pub const fn execution_revision(self) -> ExecutionRevision {
        self.execution_revision
    }

    pub const fn frame_epoch(self) -> FrameEpoch {
        self.frame_epoch
    }

    pub const fn with_execution_revision(self, execution_revision: ExecutionRevision) -> Self {
        Self {
            execution_revision,
            ..self
        }
    }

    pub const fn with_frame_epoch(self, frame_epoch: FrameEpoch) -> Self {
        Self {
            frame_epoch,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SemanticMutationTransaction, SemanticNodeId, SemanticObjectProperty, SemanticObjectState,
        SemanticStore, StoredGeometry,
    };

    #[test]
    fn publication_domains_are_independent_typed_values() {
        let context = PublicationContext::new(
            SceneRevision::new(2),
            ExecutionRevision::new(5),
            FrameEpoch::new(11),
        );
        assert_eq!(context.scene_revision().get(), 2);
        assert_eq!(context.execution_revision().get(), 5);
        assert_eq!(context.frame_epoch().get(), 11);
        assert_eq!(
            SceneRevision::new(2).checked_next(),
            Some(SceneRevision::new(3))
        );
        assert_eq!(FrameEpoch::new(u64::MAX).checked_next(), None);
    }

    #[test]
    fn semantic_transaction_mints_one_scene_revision_only_for_changed_commit() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let initial = store.scene_revision();

        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_property(object, SemanticObjectProperty::ObjectOpacity, 0.5_f64);
        transaction.apply(&mut store).unwrap();
        let committed = store.scene_revision();
        assert_eq!(committed, initial.checked_next().unwrap());

        let mut no_op = SemanticMutationTransaction::new();
        no_op.set_property(object, SemanticObjectProperty::ObjectOpacity, 0.5_f64);
        no_op.apply(&mut store).unwrap();
        assert_eq!(store.scene_revision(), committed);

        let mut failed = SemanticMutationTransaction::new();
        failed.set_property(
            SemanticNodeId::new(u32::MAX, 0),
            SemanticObjectProperty::ObjectOpacity,
            0.75_f64,
        );
        assert!(failed.apply(&mut store).is_err());
        assert_eq!(store.scene_revision(), committed);
    }
}
