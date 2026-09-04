use noon::ExecutionSession;
use noon_core::{SemanticObjectState, SemanticStore, StoredGeometry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = SemanticStore::new();
    let circle = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.5,
    }));
    store.attach_to_scene(circle)?;

    let session = ExecutionSession::from_semantic_store(&store)?;
    noon_native::run(session)?;
    Ok(())
}
