use noon::Scene;
use noon_core::{SemanticObjectState, SemanticStore, StoredGeometry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let circle = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.5,
    }));
    store.add_semantic_family_member(root, circle)?;

    let scene =
        Scene::with_integration_root(std::rc::Rc::new(std::cell::RefCell::new(store)), root)?;
    let session = scene.into_execution()?;
    noon_native::run(session)?;
    Ok(())
}
