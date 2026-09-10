use noon_core::{GeometryResourceError, SemanticStore, Vec2, VectorPath};

#[test]
fn failed_path_publication_discards_only_the_unpublished_resource() {
    let mut store = SemanticStore::new();
    let path = || VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE);
    let old = store.insert_geometry_path(path()).unwrap();
    let before = store.geometry_resources().stats();
    let revision = store.scene_revision();
    let mut rejected = None;
    let result: Result<(), GeometryResourceError> =
        store.with_geometry_path(path(), |store, fresh| {
            rejected = Some(fresh);
            // An admission error is returned after allocation, as when publication
            // rejects a candidate. The original retained resource remains valid.
            store
                .insert_geometry_path(VectorPath::new().move_to(Vec2::new(f32::NAN, 0.)))
                .map(|_| ())
        });
    assert!(result.is_err());
    assert_eq!(store.geometry_resources().stats(), before);
    assert_eq!(store.scene_revision(), revision);
    assert!(store.geometry_resources().get(old).is_some());
    assert!(store.geometry_resources().get(rejected.unwrap()).is_none());
    let accepted = store
        .with_geometry_path(path(), |_, fresh| Ok::<_, GeometryResourceError>(fresh))
        .unwrap();
    assert!(store.geometry_resources().get(accepted).is_some());
    assert!(store.geometry_resources().get(rejected.unwrap()).is_none());
}
