use super::*;
use noon_core::{
    SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectState, StoredGeometry,
};

#[test]
fn defaults_match_manim_matrix_buffers() {
    assert_eq!(MatrixOptions::default().v_buff, 0.8);
    assert_eq!(MatrixOptions::default().h_buff, 1.3);
    assert_eq!(MatrixOptions::default().bracket_h_buff, 0.25);
    assert_eq!(MatrixOptions::default().bracket_v_buff, 0.25);
}

#[test]
fn shape_rejects_empty_and_ragged_input() {
    assert!(matches!(
        matrix_shape::<f64>(&[]),
        Err(MatrixAuthoringError::EmptyMatrix)
    ));
    assert!(matches!(
        matrix_shape(&[vec![1.0], vec![2.0, 3.0]]),
        Err(MatrixAuthoringError::RaggedRows {
            expected: 1,
            actual: 2
        })
    ));
}

#[test]
fn durable_nested_rows_reconstruct_shape_and_original_identities() {
    let store = Rc::new(RefCell::new(noon_core::SemanticStore::new()));
    let first = Mobject::new(
        Rc::clone(&store),
        SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }),
    )
    .unwrap();
    let second = Mobject::new(
        Rc::clone(&store),
        SemanticObjectState::new(StoredGeometry::Circle { radius: 2.0 }),
    )
    .unwrap();
    let left = Mobject::new(
        Rc::clone(&store),
        SemanticObjectState::new(StoredGeometry::Circle { radius: 0.25 }),
    )
    .unwrap();
    let right = Mobject::new(
        Rc::clone(&store),
        SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 }),
    )
    .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let row = transaction.create_node(SemanticNodeCreation::family());
    transaction.add_member(row, first.node_id());
    transaction.add_member(row, second.node_id());
    let entries = transaction.create_node(SemanticNodeCreation::family());
    transaction.add_member(entries, row);
    let root = transaction.create_node(SemanticNodeCreation::family());
    transaction.add_member(root, entries);
    transaction.add_member(root, left.node_id());
    transaction.add_member(root, right.node_id());
    let result = transaction.apply(&mut store.borrow_mut()).unwrap();
    let root = MobjectFamily::from_node(Rc::clone(&store), result.resolve(root).unwrap()).unwrap();
    let matrix = Matrix::from_family(root).unwrap();
    assert_eq!(matrix.shape().unwrap(), (1, 2));
    assert_eq!(
        matrix.entries().unwrap(),
        vec![first.clone(), second.clone()]
    );
    assert_eq!(
        matrix.rows().unwrap(),
        vec![vec![first.clone(), second.clone()]]
    );
    assert_eq!(matrix.columns().unwrap(), vec![vec![first], vec![second]]);
    assert_eq!(matrix.left_bracket(), &left);
    assert_eq!(matrix.right_bracket(), &right);
}

#[test]
fn mobject_entries_reject_duplicates_before_compilation() {
    let store = Rc::new(RefCell::new(noon_core::SemanticStore::new()));
    let object = Mobject::new(
        Rc::clone(&store),
        SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }),
    )
    .unwrap();
    assert!(matches!(
        validate_entries(&store, &[vec![object.clone(), object]]),
        Err(MatrixAuthoringError::DuplicateEntry)
    ));
}
