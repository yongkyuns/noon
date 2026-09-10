use noon::{ManimGeometryOptions, Scene};

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

#[test]
fn coarse_path_arrangement_uses_anchors_and_grid_uses_dimensions() {
    for live_mode in [false, true] {
        for grid in [false, true] {
            let mut scene = Scene::new();
            let first = scene
                .geometry(ManimGeometryOptions::arc(1., -0.3, 1.8, 3, 0., 0.).unwrap())
                .unwrap();
            let second = first.copy_handle().unwrap();
            let content = first.state().unwrap().content;
            let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
            let original_center = family.layout().unwrap().center();
            let dimension_width = first.width().unwrap();
            let anchor_width =
                first.critical_point(1., 0.).unwrap().0 - first.critical_point(-1., 0.).unwrap().0;
            assert!((dimension_width - anchor_width).abs() > 0.02);
            let unrelated = scene.circle(0.1).unwrap();
            let unrelated_state = unrelated.state().unwrap();
            scene.add_many(&[(&family).into()]).unwrap();
            let revision = scene.revision();
            let mut session = scene.execution_session().unwrap();
            if live_mode {
                let mut live = scene.live(&mut session);
                if grid {
                    live.arrange_family_in_grid(&family, Some(1), Some(2), 0.25, 0.25)
                        .unwrap();
                } else {
                    live.arrange_family(&family, 1., 0., 0.25, true).unwrap();
                }
                close(
                    live.effective_layout(&first).unwrap().center.0,
                    first.center().unwrap().0,
                );
            } else if grid {
                family
                    .arrange_in_grid(Some(1), Some(2), 0.25, 0.25)
                    .unwrap();
            } else {
                family.arrange(1., 0., 0.25, true).unwrap();
            }
            let distance = second.center().unwrap().0 - first.center().unwrap().0;
            close(
                distance,
                if grid { dimension_width } else { anchor_width } + 0.25,
            );
            let center = family.layout().unwrap().center();
            close(center.0, if grid { original_center.0 } else { 0. });
            close(center.1, if grid { original_center.1 } else { 0. });
            assert_eq!(scene.revision(), revision.checked_next().unwrap());
            assert_eq!(first.state().unwrap().content, content);
            assert_eq!(second.state().unwrap().content, content);
            assert_eq!(unrelated.state().unwrap(), unrelated_state);
        }
    }
}

#[test]
fn paired_path_arrangement_runs_the_normal_execution_session() {
    let mut session = noon::example_scenes::path_arrangement::session().unwrap();
    assert_eq!(session.frame().objects.len(), 4);
    session.seek(0.).unwrap();
    let before = session.frame().objects.clone();
    session.seek(0.2).unwrap();
    assert_eq!(session.frame().objects, before);
}
