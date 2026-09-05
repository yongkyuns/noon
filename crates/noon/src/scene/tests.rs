use super::*;
#[test]
fn membership_preserves_identity_isolates_roots_and_rejects_foreign_stores() {
    let store = Rc::new(RefCell::new(SemanticStore::new()));
    let mut first = Scene::with_store(Rc::clone(&store));
    let mut second = Scene::with_store(Rc::clone(&store));
    let object = first.circle(1.0).unwrap();
    let id = object.node_id();
    first.add(&object).unwrap();
    first.add(&object).unwrap();
    assert!(first
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_some());
    assert!(second
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_none());
    second.add(&object).unwrap();
    first.remove(&object).unwrap();
    assert!(first
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_none());
    assert!(second
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_some());
    first.add(&object).unwrap();
    assert_eq!(object.node_id(), id);
    let mut foreign = Scene::new();
    assert!(foreign.add(&object).is_err());
    assert!(foreign
        .execution_session()
        .unwrap()
        .frame()
        .objects
        .is_empty());
}
