use super::*;
use noon_core::{
    SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectState, StoredGeometry,
};

struct RuleBackend;
impl LatexBackend for RuleBackend {
    fn identity(&self) -> &str {
        "matrix-rule-fixture"
    }
    fn format(&self) -> crate::LatexFormat {
        crate::LatexFormat::Preloaded
    }
    fn font(&mut self, _: &str) -> Result<crate::DviFontResource, String> {
        Err("font-free fixture".into())
    }
    fn compile(&mut self, _: &str) -> Result<Vec<u8>, String> {
        let mut dvi = vec![247, 2];
        for value in [25_400_000u32, 473_628_672, 1000] {
            dvi.extend(value.to_be_bytes());
        }
        dvi.push(0);
        dvi.push(139);
        dvi.extend([0; 44]);
        dvi.push(132);
        dvi.extend(655_360i32.to_be_bytes());
        dvi.extend(327_680i32.to_be_bytes());
        dvi.push(140);
        dvi.push(248);
        dvi.extend([0; 28]);
        dvi.push(249);
        dvi.extend([0; 4]);
        dvi.push(2);
        dvi.extend([223; 4]);
        Ok(dvi)
    }
}

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
        vec![
            MatrixEntry::Mobject(first.clone()),
            MatrixEntry::Mobject(second.clone())
        ]
    );
    assert_eq!(
        matrix.rows().unwrap(),
        vec![vec![
            MatrixEntry::Mobject(first.clone()),
            MatrixEntry::Mobject(second.clone())
        ]]
    );
    assert_eq!(
        matrix.columns().unwrap(),
        vec![
            vec![MatrixEntry::Mobject(first)],
            vec![MatrixEntry::Mobject(second)]
        ]
    );
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

#[test]
fn text_constructor_admits_baselines_and_centers_entries_with_brackets() {
    let store = Rc::new(RefCell::new(noon_core::SemanticStore::new()));
    let matrix = Matrix::from_rows_in_store(
        Rc::clone(&store),
        &mut RuleBackend,
        [["a", "b"], ["c", "d"]],
        MatrixOptions::default(),
    )
    .unwrap();
    assert_eq!(matrix.shape().unwrap(), (2, 2));
    for entry in matrix.entries().unwrap() {
        assert!(entry
            .state()
            .unwrap()
            .text_presentation_baseline()
            .is_some());
    }
    let bounds = matrix.family().layout_bounds().unwrap().unwrap();
    assert!(((bounds.min_x + bounds.max_x) * 0.5).abs() < 1.0e-6);
}

#[test]
fn copied_matrix_reconstructs_independent_durable_topology() {
    let store = Rc::new(RefCell::new(noon_core::SemanticStore::new()));
    let matrix = Matrix::from_rows_in_store(
        Rc::clone(&store),
        &mut RuleBackend,
        [["a", "b"], ["c", "d"]],
        MatrixOptions::default(),
    )
    .unwrap();
    let copied = matrix.family().copy_family().unwrap();
    let copy = Matrix::from_family(copied.root().clone()).unwrap();

    assert_eq!(copy.shape().unwrap(), (2, 2));
    assert_ne!(copy.family().node_id(), matrix.family().node_id());
    assert_ne!(
        copy.entries().unwrap()[0].node_id(),
        matrix.entries().unwrap()[0].node_id()
    );
    assert_ne!(
        copy.left_bracket().node_id(),
        matrix.left_bracket().node_id()
    );
}

#[test]
fn stale_live_matrix_rolls_back_compiled_resources_and_topology() {
    let mut scene = Scene::new();
    let mut execution = scene.execution_session().unwrap();
    // Deliberately advance the scene after recording execution's publication
    // context, so the live admission is rejected after its text has prepared.
    scene.circle(0.1).unwrap();
    let revision = scene.revision();
    let count = scene.integration_store().borrow().len();
    let resources = {
        let store = scene.integration_store().borrow();
        (
            store.geometry_resources().len(),
            store.text_resources().len(),
        )
    };
    let mut live = crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
    assert!(Matrix::from_rows_in_live_session(
        &mut live,
        &mut RuleBackend,
        [["a", "b"]],
        MatrixOptions::default(),
    )
    .is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), count);
    let store = scene.integration_store().borrow();
    assert_eq!(
        (
            store.geometry_resources().len(),
            store.text_resources().len()
        ),
        resources
    );
}

#[test]
fn driven_entries_reject_atomically_in_both_execution_ownership_modes() {
    for scene_owned in [false, true] {
        let mut scene = Scene::new();
        let entry = scene.square(1.0).unwrap();
        let pointer = scene.pointer_position_signal().unwrap();
        scene.bind_native_translation(&entry, &pointer).unwrap();
        scene.add(&entry).unwrap();
        let mut execution = scene.execution_session().unwrap();
        execution
            .set_native_state_input(
                noon_core::NativeStateSource::PointerPosition,
                noon_core::NativeInputValue::Vec2(noon_core::Vec2::new(3.0, 1.0)),
            )
            .unwrap();
        let authored = entry.state().unwrap();
        let revision = scene.revision();
        let count = scene.integration_store().borrow().len();
        let resources = scene.integration_store().borrow().text_resources().len();
        let result = if scene_owned {
            scene.install_execution(execution);
            MobjectMatrix::from_rows(&mut scene, &mut RuleBackend, [[entry.clone()]])
        } else {
            MobjectMatrix::from_rows_in_live_session(
                &mut scene.live(&mut execution),
                &mut RuleBackend,
                [[entry.clone()]],
                MatrixOptions::default(),
            )
        };
        assert!(matches!(
            result,
            Err(MatrixAuthoringError::Semantic(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver
            )))
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(entry.state().unwrap(), authored);
        assert_eq!(scene.integration_store().borrow().len(), count);
        assert_eq!(
            scene.integration_store().borrow().text_resources().len(),
            resources
        );
    }
}
