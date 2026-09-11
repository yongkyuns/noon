use noon::{ManimArrow, ManimArrowOptions, Scene, StoredGeometry, Vec2};
use noon_core::SemanticObjectContent;

fn shaft_endpoints(arrow: &ManimArrow) -> (Vec2, Vec2) {
    let state = arrow.shaft().state().expect("shaft state");
    let SemanticObjectContent::Geometry(StoredGeometry::Line { start, end }) = state.content else {
        panic!("Arrow shaft must remain an analytic Line");
    };
    (start, end)
}

#[test]
fn arrow_vector_and_double_arrow_share_one_semantic_family_contract() {
    let mut scene = Scene::new();

    let mut arrow_options = ManimArrowOptions::arrow(-3.0, 1.2, -0.5, 1.2).unwrap();
    arrow_options
        .set_color(88.0 / 255.0, 196.0 / 255.0, 221.0 / 255.0, 1.0)
        .unwrap();
    let arrow = scene.manim_arrow(arrow_options).unwrap();

    let mut vector_options = ManimArrowOptions::vector(2.0, 1.0).unwrap();
    vector_options.set_translation(-0.5, -1.0).unwrap();
    vector_options
        .set_color(247.0 / 255.0, 217.0 / 255.0, 111.0 / 255.0, 1.0)
        .unwrap();
    let vector = scene.manim_arrow(vector_options).unwrap();

    let mut double_options = ManimArrowOptions::double_arrow(1.0, 1.2, 3.5, 1.2).unwrap();
    double_options
        .set_color(252.0 / 255.0, 98.0 / 255.0, 85.0 / 255.0, 1.0)
        .unwrap();
    let double = scene.manim_arrow(double_options).unwrap();

    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(arrow.family().node_id())
            .unwrap(),
        vec![arrow.shaft().node_id(), arrow.end_tip().node_id()]
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(double.family().node_id())
            .unwrap(),
        vec![
            double.shaft().node_id(),
            double.end_tip().node_id(),
            double.start_tip().unwrap().node_id()
        ]
    );

    let (vector_start, _) = shaft_endpoints(&vector);
    assert_eq!(vector_start, Vec2::ZERO);

    scene
        .add_many(&[
            arrow.family().into(),
            vector.family().into(),
            double.family().into(),
        ])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    {
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        live.complete_segment(wait).unwrap();
    }
}

#[test]
fn arrow_construction_rejects_invalid_values_without_publishing() {
    let scene = Scene::new();
    let revision = scene.integration_store().borrow().scene_revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();

    assert!(ManimArrowOptions::arrow(f64::NAN, 0.0, 1.0, 0.0).is_err());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        resources
    );
}

#[test]
fn buff_and_tip_caps_match_pinned_manim_reference_cases() {
    let scene = Scene::new();

    let mut oversized_buff = ManimArrowOptions::arrow(-0.2, 0.0, 0.2, 0.0).unwrap();
    oversized_buff.set_buff(0.25).unwrap();
    let oversized = scene.manim_arrow(oversized_buff).unwrap();
    let (start, _) = shaft_endpoints(&oversized);
    assert!((start.x + 0.2).abs() < 1.0e-6);

    let mut short = ManimArrowOptions::arrow(0.0, 0.0, 0.4, 0.0).unwrap();
    short.set_buff(0.0).unwrap();
    let short = scene.manim_arrow(short).unwrap();
    let (_, end) = shaft_endpoints(&short);
    assert!((end.x - 0.3).abs() < 1.0e-6);
    assert!((short.shaft().state().unwrap().style.stroke_width - 0.02).abs() < 1.0e-12);
}
