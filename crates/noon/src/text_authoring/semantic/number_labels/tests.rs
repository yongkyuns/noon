use super::*;
use crate::{ManimAxesOptions, ManimNumberLineOptions, Mobject, Scene};

fn counts(scene: &Scene) -> (usize, usize, usize) {
    let store = scene.integration_store().borrow();
    (
        store.geometry_resources().len(),
        store.text_resources().len(),
        store.font_resources().len(),
    )
}
fn leaves(family: &MobjectFamily) -> Vec<Mobject> {
    family
        .integration_store()
        .borrow()
        .ordered_leaf_nodes(family.node_id())
        .unwrap()
        .into_iter()
        .map(|id| Mobject::from_node(Rc::clone(family.integration_store()), id).unwrap())
        .collect()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 2e-5, "{a} != {b}");
}
fn text(object: &Mobject) -> String {
    let resource = object.state().unwrap().content.text().unwrap();
    object
        .integration_store()
        .borrow()
        .text_resources()
        .get(resource)
        .unwrap()
        .source
        .to_string()
}

#[test]
fn attached_labels_are_ordered_text_and_move_with_the_number_line() {
    let mut scene = Scene::new();
    let line = scene
        .number_line(&ManimNumberLineOptions::new([-2.0, 2.0, 1.0]))
        .unwrap();
    let labels = line
        .add_numbers(Some(&[1.0, -1.0, 1.0]), &NumberLabelOptions::default())
        .unwrap();
    let labels = leaves(&labels);
    assert_eq!(
        labels.iter().map(text).collect::<Vec<_>>(),
        ["1", "-1", "1"]
    );
    near(
        labels[0].center().unwrap().0,
        line.authored_frame().unwrap().number_to_point(1.0).unwrap()[0],
    );
    let before: Vec<_> = labels.iter().map(|l| l.state().unwrap().content).collect();
    let count = counts(&scene);
    line.family().shift(2.0, 3.0).unwrap();
    near(labels[0].center().unwrap().0, 3.0);
    for (label, content) in labels.iter().zip(before) {
        assert_eq!(label.state().unwrap().content, content);
    }
    assert_eq!(counts(&scene), count);
    scene.add_many(&[line.family().into()]).unwrap();
    assert_eq!(
        scene
            .execution_session()
            .unwrap()
            .frame()
            .objects
            .iter()
            .filter(|o| o.text().is_some())
            .count(),
        3
    );
}

#[test]
fn detached_labels_attach_only_on_request_and_empty_values_stay_empty() {
    let mut scene = Scene::new();
    let line = scene
        .number_line(&ManimNumberLineOptions::new([0.0, 2.0, 1.0]))
        .unwrap();
    let options = NumberLabelOptions {
        exclude_zero: false,
        ..Default::default()
    };
    let labels = line.get_number_mobjects(None, &options).unwrap();
    assert_eq!(leaves(&labels).len(), 3);
    assert_eq!(
        leaves(line.family())
            .iter()
            .filter(|o| o.state().unwrap().content.text().is_some())
            .count(),
        0
    );
    let count = counts(&scene);
    let empty = line.get_number_mobjects(Some(&[]), &options).unwrap();
    assert!(leaves(&empty).is_empty());
    assert_eq!(counts(&scene), count);
}

#[test]
fn second_axis_preparation_failure_is_atomic_including_fonts() {
    let mut scene = Scene::new();
    let axes = scene
        .axes(&ManimAxesOptions::new(
            [0.0, 2.0, 1.0],
            [0.0, 2.0, 1.0],
            4.0,
            3.0,
        ))
        .unwrap();
    let before = counts(&scene);
    let revision = scene.revision();
    let x = NumberLabelOptions::default();
    let y = NumberLabelOptions {
        font: "No such font family".into(),
        ..x.clone()
    };
    assert!(axes.add_coordinates(None, None, &x, &y).is_err());
    assert_eq!(counts(&scene), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        leaves(axes.family())
            .iter()
            .filter(|o| o.state().unwrap().content.text().is_some())
            .count(),
        0
    );
}

#[test]
fn late_transaction_failure_discards_text_without_interning_fonts() {
    let scene = Scene::new();
    let sentinel = scene.circle(0.5).unwrap();
    let before = sentinel.state().unwrap();
    let revision = scene.revision();
    let count = counts(&scene);
    let frame = NumberLineFrame::centered([0.0, 2.0, 1.0], 2.0, 0.0).unwrap();
    let prepared = prepare(frame, None, &NumberLabelOptions::default()).unwrap();
    // Real shaped resources are inserted before this late non-family-parent
    // failure. No scene cloning, deletion of existing text, or leaked fonts.
    assert!(publish(
        Rc::clone(scene.integration_store()),
        vec![(Some(sentinel.node_id()), prepared)]
    )
    .is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(counts(&scene), count);
    assert_eq!(sentinel.state().unwrap(), before);
    let prepared = prepare(frame, None, &NumberLabelOptions::default()).unwrap();
    assert!(publish(Rc::clone(scene.integration_store()), vec![(None, prepared)]).is_ok());
}

#[test]
fn copied_axes_retain_label_families_without_copying_resources() {
    let mut scene = Scene::new();
    let axes = scene
        .axes(&ManimAxesOptions::new(
            [0.0, 2.0, 1.0],
            [0.0, 2.0, 1.0],
            4.0,
            3.0,
        ))
        .unwrap();
    let x = NumberLabelOptions {
        exclude_zero: false,
        ..Default::default()
    };
    let y = NumberLabelOptions {
        direction: [-1.0, 0.0],
        ..Default::default()
    };
    axes.add_coordinates(None, None, &x, &y).unwrap();
    let count = counts(&scene);
    let copied = axes.family().copy_family().unwrap();
    let cloned = ManimAxes::from_family(copied.root().clone()).unwrap();
    cloned.family().shift(5.0, 0.0).unwrap();
    assert_eq!(counts(&scene), count);
    assert_eq!(
        leaves(cloned.family())
            .iter()
            .filter(|o| o.state().unwrap().content.text().is_some())
            .count(),
        5
    );
    near(
        cloned
            .authored_frame()
            .unwrap()
            .coords_to_point(0.0, 0.0)
            .unwrap()[0]
            - axes
                .authored_frame()
                .unwrap()
                .coords_to_point(0.0, 0.0)
                .unwrap()[0],
        5.0,
    );
}

#[test]
fn invalid_numbers_and_presentation_do_not_allocate_resources() {
    let mut scene = Scene::new();
    let line = scene
        .number_line(&ManimNumberLineOptions::new([0.0, 2.0, 1.0]))
        .unwrap();
    let revision = scene.revision();
    let count = counts(&scene);
    assert!(line
        .add_numbers(Some(&[1.0, f64::NAN]), &NumberLabelOptions::default())
        .is_err());
    for options in [
        NumberLabelOptions {
            direction: [0.0, 0.0],
            ..Default::default()
        },
        NumberLabelOptions {
            font_size: 0.0,
            ..Default::default()
        },
        NumberLabelOptions {
            decimal_places: 13,
            ..Default::default()
        },
    ] {
        assert!(line.add_numbers(None, &options).is_err());
    }
    assert_eq!(counts(&scene), count);
    assert_eq!(scene.revision(), revision);
}

#[test]
fn rejected_provisional_text_handles_never_revive_on_retry() {
    let scene = Scene::new();
    let frame = NumberLineFrame::centered([0.0, 2.0, 1.0], 2.0, 0.0).unwrap();
    let label = prepare(frame, Some(&[1.0]), &NumberLabelOptions::default())
        .unwrap()
        .remove(0);
    let input = (label.resource, label.fonts);
    let mut rejected = None;
    let mut store = scene.integration_store().borrow_mut();
    let revision = store.scene_revision();
    let result = store.apply_glyph_text_transaction::<Error>(vec![input.clone()], |handles| {
        rejected = Some(handles[0]);
        Err(PlotPresentationError::InvalidInput("builder rejected").into())
    });
    assert!(result.is_err());
    let rejected = rejected.unwrap();
    assert!(store.text_resources().get(rejected).is_none());
    assert_eq!(store.text_resources().len(), 0);
    assert_eq!(store.font_resources().len(), 0);
    assert_eq!(store.scene_revision(), revision);
    let mut accepted = None;
    store
        .apply_glyph_text_transaction::<Error>(vec![input], |handles| {
            accepted = Some(handles[0]);
            let mut transaction = SemanticMutationTransaction::new();
            transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
                handles[0],
            )));
            Ok(transaction)
        })
        .unwrap();
    assert_ne!(accepted.unwrap(), rejected);
    assert!(store.text_resources().get(rejected).is_none());
    assert!(store.text_resources().get(accepted.unwrap()).is_some());
}

#[test]
fn conflicting_incoming_fonts_fail_before_any_batch_insertion() {
    let scene = Scene::new();
    let frame = NumberLineFrame::centered([0.0, 2.0, 1.0], 2.0, 0.0).unwrap();
    let label = prepare(frame, Some(&[1.0]), &NumberLabelOptions::default())
        .unwrap()
        .remove(0);
    let face = label.resource.runs[0].font.clone();
    let mut conflicting = FontResourceArena::new();
    conflicting
        .intern_face(&face, std::sync::Arc::<[u8]>::from([1u8, 2, 3]))
        .unwrap();
    let mut store = scene.integration_store().borrow_mut();
    let revision = store.scene_revision();
    let result = store.apply_glyph_text_transaction::<Error>(
        vec![
            (label.resource.clone(), label.fonts),
            (label.resource, conflicting),
        ],
        |_| panic!("font conflict must reject before building a transaction"),
    );
    assert!(matches!(
        result,
        Err(Error::Import(noon_core::SemanticTextImportError::Font(_)))
    ));
    assert_eq!(store.text_resources().len(), 0);
    assert_eq!(store.font_resources().len(), 0);
    assert_eq!(store.scene_revision(), revision);
}

struct MoveLabels {
    source: MobjectFamily,
    target: MobjectFamily,
    started: bool,
}
impl crate::LiveContinuation for MoveLabels {
    type Error = String;
    fn resume(
        &mut self,
        live: &mut crate::LiveSession<'_>,
    ) -> Result<crate::ContinuationStep, String> {
        if self.started {
            return Ok(crate::ContinuationStep::Finished);
        }
        self.started = true;
        live.declare_and_activate_family_transform_to(
            &self.source,
            &self.target,
            crate::AnimationOptions::new()
                .run_time(1.0)
                .rate_func(crate::RateFunction::Linear),
        )
        .map(crate::ContinuationStep::Await)
        .map_err(|error| error.to_string())
    }
}

#[test]
fn effective_label_motion_uses_existing_family_drivers_without_resource_churn() {
    let mut scene = Scene::new();
    let line = scene
        .number_line(&ManimNumberLineOptions::new([-2.0, 2.0, 1.0]))
        .unwrap();
    let labels = line
        .add_numbers(None, &NumberLabelOptions::default())
        .unwrap();
    line.family().scale(-0.8, 1.2).unwrap();
    line.family()
        .rotate(0.25, crate::ManimRotationPivot::Center)
        .unwrap();
    let label = leaves(&labels).remove(0);
    let authored = label.state().unwrap();
    let copied = line.family().copy_family().unwrap();
    copied.root().shift(2.0, 4.0).unwrap();
    scene.add_many(&[line.family().into()]).unwrap();
    let count = counts(&scene);
    let mut program = scene
        .into_live_program(MoveLabels {
            source: line.family().clone(),
            target: copied.root().clone(),
            started: false,
        })
        .unwrap();
    program.resume().unwrap();
    let before = program
        .session()
        .frame()
        .objects
        .iter()
        .find(|o| o.text().is_some())
        .unwrap()
        .clone();
    let revision = program.session().publication_context().scene_revision();
    let mut callbacks = crate::RustHostCallbackTable::new();
    program.drive_to(&mut callbacks, 0.5).unwrap();
    let effective = program
        .session()
        .frame()
        .objects
        .iter()
        .find(|o| o.id == before.id)
        .unwrap();
    near(
        f64::from(effective.transform.translation.x - before.transform.translation.x),
        1.0,
    );
    near(
        f64::from(effective.transform.translation.y - before.transform.translation.y),
        2.0,
    );
    assert_eq!(effective.content, before.content);
    assert_eq!(label.state().unwrap(), authored);
    assert_eq!(
        program.session().publication_context().scene_revision(),
        revision
    );
    let store = line.family().integration_store().borrow();
    assert_eq!(
        (
            store.geometry_resources().len(),
            store.text_resources().len(),
            store.font_resources().len()
        ),
        count
    );
}
