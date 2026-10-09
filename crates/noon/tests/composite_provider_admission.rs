//! Exercise provider-gated production code from an external test crate.
//! The library is compiled without cfg(test). No font or TeX process is needed.
#![cfg(any(feature = "native-text", feature = "latex"))]

use noon::{CompositeEntryHandle, MobjectTable, MobjectTarget, Scene, TableOptions};

#[test]
fn table_entries_preserve_family_roots_in_the_production_library() {
    let mut scene = Scene::new();
    let first = scene.circle(1.0).unwrap();
    let mut second = scene.circle(0.5).unwrap();
    second.shift(3.0, 0.0).unwrap();
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let separation = second.center().unwrap().0 - first.center().unwrap().0;

    let rows = vec![vec![MobjectTarget::from(&family)]];
    let table = MobjectTable::from_target_rows(&mut scene, rows).unwrap();
    assert_eq!(table.shape().unwrap(), (1, 1));
    assert_eq!(
        table.get_entry(0, 0).unwrap(),
        CompositeEntryHandle::Family(family)
    );
    assert_eq!(
        second.center().unwrap().0 - first.center().unwrap().0,
        separation
    );
}

#[test]
fn live_table_admission_keeps_one_session_and_rejects_duplicate_entries() {
    let mut scene = Scene::new();
    let first = scene.circle(0.5).unwrap();
    let second = scene.circle(1.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.publication_context();
    let authored = first.state().unwrap();
    let rejected = MobjectTable::from_rows_in_live_session(
        &mut scene.live(&mut session),
        vec![vec![first.clone(), first.clone()]],
        TableOptions::default(),
    );
    assert!(rejected.is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(first.state().unwrap(), authored);

    let table = MobjectTable::from_rows_in_live_session(
        &mut scene.live(&mut session),
        vec![vec![first.clone(), second.clone()]],
        TableOptions::default(),
    )
    .unwrap();
    assert_eq!(table.shape().unwrap(), (1, 2));
    assert_eq!(
        table.get_entries().unwrap(),
        vec![
            CompositeEntryHandle::Mobject(first),
            CompositeEntryHandle::Mobject(second),
        ]
    );
    assert_eq!(session.frame().time, 0.0);
}

#[cfg(feature = "latex")]
#[test]
fn matrix_queries_preserve_original_entries_without_compiling_tex() {
    let mut scene = Scene::new();
    let first = scene.circle(0.5).unwrap();
    let second = scene.circle(1.0).unwrap();
    let left = scene.circle(0.25).unwrap();
    let right = scene.circle(0.25).unwrap();
    let row = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let entries = scene.family(&[(&row).into()]).unwrap();
    let root = scene
        .family(&[(&entries).into(), (&left).into(), (&right).into()])
        .unwrap();
    let matrix = noon::Matrix::from_family(root).unwrap();
    assert_eq!(matrix.shape().unwrap(), (1, 2));
    assert_eq!(
        matrix.entries().unwrap(),
        vec![
            CompositeEntryHandle::Mobject(first),
            CompositeEntryHandle::Mobject(second),
        ]
    );
    assert_eq!(matrix.left_bracket(), &left);
    assert_eq!(matrix.right_bracket(), &right);
}

#[cfg(feature = "latex")]
mod latex_labels {
    use noon::plot_presentation::{NumberLabelAuthoringError, NumberLabelOptions};
    use noon::{DviFontResource, LatexBackend, LatexFormat, ManimNumberLineOptions, Scene};

    // Empty selections and invalid options must not need a compiler. A valid
    // nonempty selection must reach this explicit host backend, never native
    // shaping. Backend failure must leave the semantic store unchanged.
    #[derive(Default)]
    struct RejectingBackend {
        compile_calls: usize,
    }

    impl LatexBackend for RejectingBackend {
        fn identity(&self) -> &str {
            "isolated-latex-label-provider-regression"
        }

        fn format(&self) -> LatexFormat {
            LatexFormat::Preloaded
        }

        fn compile(&mut self, _: &str) -> Result<Vec<u8>, String> {
            self.compile_calls += 1;
            Err("intentional isolated LaTeX backend failure".into())
        }

        fn font(&mut self, _: &str) -> Result<DviFontResource, String> {
            panic!("failed compilation must not request fonts")
        }
    }

    #[test]
    fn empty_decimal_labels_publish_without_native_shaping_or_tex() {
        let mut scene = Scene::new();
        let line = scene
            .number_line(&ManimNumberLineOptions::new([-2.0, 2.0, 1.0]))
            .unwrap();
        let options = NumberLabelOptions::default();
        let mut backend = RejectingBackend::default();
        let labels = line
            .add_decimal_numbers(&mut backend, Some(&[]), &options)
            .unwrap();
        assert_eq!(backend.compile_calls, 0);
        labels.validate().unwrap();
        let store = scene.integration_store().borrow();
        assert!(store
            .semantic_family_members_checked(labels.node_id())
            .unwrap()
            .is_empty());
        assert!(store
            .semantic_family_members_checked(line.family().node_id())
            .unwrap()
            .contains(&labels.node_id()));
    }

    #[test]
    fn latex_label_validation_and_backend_errors_do_not_publish() {
        let mut scene = Scene::new();
        let line = scene
            .number_line(&ManimNumberLineOptions::new([-2.0, 2.0, 1.0]))
            .unwrap();
        let revision = scene.integration_store().borrow().scene_revision();
        let mut backend = RejectingBackend::default();
        let invalid = NumberLabelOptions {
            direction: [0.0, 0.0],
            ..Default::default()
        };
        assert!(matches!(
            line.add_decimal_numbers(&mut backend, Some(&[1.0]), &invalid),
            Err(NumberLabelAuthoringError::Authoring(_))
        ));
        assert_eq!(backend.compile_calls, 0);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );

        let error = line
            .add_decimal_numbers(&mut backend, Some(&[1.0]), &NumberLabelOptions::default())
            .unwrap_err();
        assert!(matches!(&error, NumberLabelAuthoringError::Numeric(_)));
        assert!(error
            .to_string()
            .contains("intentional isolated LaTeX backend failure"));
        assert_eq!(backend.compile_calls, 1);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
    }
}
