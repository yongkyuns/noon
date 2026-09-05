//! Paired Rust proof for live property and structural membership publication.
use noon::{Mobject, Scene};
use noon_core::{
    SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectProperty, SemanticObjectState,
    SemanticVec3, StoredGeometry,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let anchor = scene.circle(0.5)?;
    let toggled = scene.circle(1.0)?;
    // Author detached objects before runtime bootstrap so the session starts at the
    // same semantic revision and later publishes only their membership.
    let appended = scene.square(1.5)?;
    scene.add(&anchor)?;
    scene.add(&toggled)?;
    let root = scene.root();
    let store = std::rc::Rc::clone(scene.store());
    let mut session = scene.execution_session()?;
    let anchor_slot = session.execution_slot_for_frame_index(0).unwrap();

    {
        let mut live = scene.live(&mut session);
        live.remove(&toggled)?;
        assert!(live.effective(&toggled).is_err());
        live.add(&toggled)?;
        live.add(&appended)?;
        live.set_translation(&appended, 2.0, -1.0)?;
        assert_eq!(live.effective(&appended)?.transform.translation.x, 2.0);

        // Rust can also use the transaction-local vocabulary directly without a
        // wrapper-side placeholder identity.
        let mut transaction = SemanticMutationTransaction::new();
        let pending = transaction.create_node(SemanticNodeCreation::object(
            SemanticObjectState::new(StoredGeometry::Circle { radius: 0.25 }),
        ));
        transaction
            .set_property(
                pending,
                SemanticObjectProperty::Translation,
                SemanticVec3::new(3.0, 1.0, 0.0),
            )
            .add_member(root, pending);
        let result = live.apply(transaction)?;
        let pending = Mobject::from_node(
            std::rc::Rc::clone(&store),
            result.resolve(pending).expect("pending node committed"),
        )?;
        assert_eq!(live.effective(&pending)?.transform.translation.x, 3.0);
    }

    assert_eq!(session.execution_slot_for_frame_index(0), Some(anchor_slot));
    Ok(())
}
