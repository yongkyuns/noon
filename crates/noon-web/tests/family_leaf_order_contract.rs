use noon_core::SemanticStore;
use noon_web::{FrontendFamilyTranslation, Mobject};

fn nested_family() -> (
    SemanticStore,
    noon_core::SemanticNodeId,
    [noon_core::SemanticNodeId; 3],
) {
    let mut store = SemanticStore::new();
    let first = store.insert_authoring_object();
    let second = store.insert_authoring_object();
    let third = store.insert_authoring_object();
    let nested = store.insert_family();
    let root = store.insert_family();

    store.add_member(nested, second).unwrap();
    store.add_member(nested, third).unwrap();
    store.add_member(root, first).unwrap();
    store.add_member(root, nested).unwrap();

    (store, root, [first, second, third])
}

#[test]
fn nested_family_translation_uses_authoritative_semantic_leaf_order() {
    let authoring_store =
        std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
    let (store, root, [first, second, third]) = nested_family();
    let mut translation = FrontendFamilyTranslation::begin(&store, root, 0.25, -0.5).unwrap();
    let mut first_handle =
        Mobject::manim_square(std::rc::Rc::clone(&authoring_store), 1.0).unwrap();
    let mut second_handle =
        Mobject::manim_square(std::rc::Rc::clone(&authoring_store), 1.0).unwrap();
    let mut third_handle =
        Mobject::manim_square(std::rc::Rc::clone(&authoring_store), 1.0).unwrap();

    translation.apply(first, &mut first_handle).unwrap();
    assert_eq!(first_handle.wire_translation().unwrap(), (0.25, -0.5));

    let second_before = second_handle.wire_translation().unwrap();
    let error = translation
        .apply(third, &mut second_handle)
        .expect_err("frontend wrapper order must not override semantic family order");
    assert!(error.contains("leaf mismatch"));
    assert_eq!(second_handle.wire_translation().unwrap(), second_before);

    translation.apply(second, &mut second_handle).unwrap();
    translation.apply(third, &mut third_handle).unwrap();
    translation.finish().unwrap();

    assert_eq!(second_handle.wire_translation().unwrap(), (0.25, -0.5));
    assert_eq!(third_handle.wire_translation().unwrap(), (0.25, -0.5));
}

#[test]
fn family_translation_refuses_incomplete_leaf_traversal() {
    let authoring_store =
        std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
    let (store, root, [first, _, _]) = nested_family();
    let mut translation = FrontendFamilyTranslation::begin(&store, root, 1.0, 0.0).unwrap();
    let mut handle = Mobject::manim_square(std::rc::Rc::clone(&authoring_store), 1.0).unwrap();

    translation.apply(first, &mut handle).unwrap();
    let error = translation
        .finish()
        .expect_err("partially traversed semantic families must fail closed");

    assert!(error.contains("applied 1 of 3 leaves"));
}
