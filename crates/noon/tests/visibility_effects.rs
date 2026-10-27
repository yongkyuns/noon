//! Visibility-role plumbing, before the high-level gesture is exposed.
//! Uses the ordinary Scene transaction, lowering and runtime publication path.
use noon::{
    effects::{EffectUpdate, Glow, NoiseDissolve, Pixels},
    ExecutionSession, Scene,
};
use noon_core::{SemanticMutationTransaction, SemanticNodeId};

fn row(session: &ExecutionSession, owner: SemanticNodeId) -> &noon_runtime::FrameObjectState {
    let object = session.execution_object_id(owner).unwrap();
    session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == object)
        .unwrap()
}

fn source_plan(
    session: &ExecutionSession,
    owner: SemanticNodeId,
) -> Option<&std::sync::Arc<noon_core::ComposedSource>> {
    match &row(session, owner).content {
        noon_core::ObjectContentRef::Composed(source) => Some(source),
        _ => None,
    }
}

#[test]
fn visibility_publishes_last_for_objects_and_composed_families_without_replacing_sources() {
    for composed in [false, true] {
        let mut scene = Scene::new();
        let leaf = scene.circle(0.4).unwrap();
        let family = scene.family(&[(&leaf).into()]).unwrap();
        let unrelated = scene.circle(0.2).unwrap();
        if composed {
            scene.add_many(&[(&family).into()]).unwrap();
        } else {
            scene.add(&leaf).unwrap();
        }
        scene.add(&unrelated).unwrap();
        let owner = if composed {
            family.node_id()
        } else {
            leaf.node_id()
        };
        let target = || -> noon::MobjectTarget<'_> {
            if composed {
                (&family).into()
            } else {
                (&leaf).into()
            }
        };
        let mut session = scene.execution_session().unwrap();
        let leaf_execution = session.execution_object_id(leaf.node_id()).unwrap();
        let unrelated_before = row(&session, unrelated.node_id()).clone();
        let mut tx = SemanticMutationTransaction::new();
        scene
            .integration_store()
            .borrow()
            .stage_visibility_mask(
                owner,
                NoiseDissolve::new(7, 0.08)
                    .unwrap()
                    .with_progress(1.0)
                    .unwrap(),
                &mut tx,
            )
            .unwrap();
        scene.live(&mut session).apply(tx).unwrap();
        let visibility = scene
            .integration_store()
            .borrow()
            .visibility_effect(owner)
            .unwrap()
            .unwrap();
        let owner_execution = session.execution_object_id(owner).unwrap();
        let original_source = source_plan(&session, owner).map(std::sync::Arc::clone);
        assert_eq!(composed, original_source.is_some());
        scene
            .live(&mut session)
            .add_effect(
                target(),
                Glow::new(
                    EffectUpdate::default()
                        .radius(Pixels(2.0))
                        .source(noon_core::GlowSource::Silhouette),
                )
                .unwrap(),
                "visibility",
            )
            .unwrap();
        let authored = if composed {
            family.get_effect("visibility").unwrap()
        } else {
            leaf.get_effect("visibility").unwrap()
        };
        assert_ne!(authored.node_id(), visibility);
        assert_eq!(
            row(&session, owner).effects.as_ref().unwrap().attachments(),
            &[authored.node_id(), visibility]
        );
        assert_eq!(
            row(&session, owner).effects.as_ref().unwrap().definitions()[1]
                .as_noise_dissolve()
                .unwrap()
                .progress(),
            1.0
        );
        assert_eq!(session.execution_object_id(owner), Some(owner_execution));
        assert_eq!(
            session.execution_object_id(leaf.node_id()),
            Some(leaf_execution)
        );
        for seed in 0..16 {
            session.take_renderer_publication();
            let mut tx = SemanticMutationTransaction::new();
            let definition = NoiseDissolve::new(seed, 0.4)
                .unwrap()
                .with_progress(f64::from(seed % 2))
                .unwrap();
            scene
                .integration_store()
                .borrow()
                .stage_visibility_mask(owner, definition, &mut tx)
                .unwrap();
            scene.live(&mut session).apply(tx).unwrap();
            assert_eq!(
                session
                    .take_renderer_publication()
                    .changes()
                    .object_indices()
                    .len(),
                1
            );
            assert_eq!(
                session
                    .last_structural_publication_stats()
                    .preparation
                    .object_states_lowered,
                0
            );
            assert_eq!(
                row(&session, owner)
                    .effects
                    .as_ref()
                    .unwrap()
                    .get(visibility)
                    .unwrap()
                    .definition,
                definition.into()
            );
            assert_eq!(row(&session, unrelated.node_id()), &unrelated_before);
            if let Some(original) = &original_source {
                assert!(std::sync::Arc::ptr_eq(
                    original,
                    source_plan(&session, owner).unwrap()
                ));
            }
        }
        let mut remove = SemanticMutationTransaction::new();
        remove.remove_node(visibility);
        scene.live(&mut session).apply(remove).unwrap();
        assert_eq!(
            row(&session, owner).effects.as_ref().unwrap().attachments(),
            &[authored.node_id()]
        );
        let before = session.frame().clone();
        let context = session.publication_context();
        let revision = scene.revision();
        let mut stale = SemanticMutationTransaction::new();
        stale.update_effect(authored.node_id(), EffectUpdate::default().intensity(2.0));
        stale.update_effect(visibility, EffectUpdate::default().progress(0.5));
        assert!(scene.live(&mut session).apply(stale).is_err());
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), context);
        assert_eq!(scene.revision(), revision);
    }
}
