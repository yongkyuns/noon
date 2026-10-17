#![cfg(all(feature = "native-text", feature = "bundled-fonts"))]

//! Qualify the diagram example through the ordinary retained runtime.

use noon::{example_scenes::diagram_composition, GeometryRef, Rect, Vec2};
use noon_runtime::FrameState;

const IMU_BOX: usize = 0;
const IMU_LABEL: usize = 1;
const EKF_BOX: usize = 2;
const EKF_LABEL: usize = 3;
const SHAFT: usize = 4;
const TIP: usize = 5;

// Expected positions come from the authored schedule, independently of the
// runtime's interpolation and family traversal: (time, common y, EKF center x).
const SAMPLES: [(f64, f32, f32); 9] = [
    (0.0, 0.0, 1.4),
    (0.25, 0.0, 1.4),
    (0.75, 0.375, 1.4),
    (1.25, 0.75, 1.4),
    (1.5, 0.75, 1.4),
    (2.0, 0.75, 1.9),
    (2.5, 0.75, 2.4),
    (2.75, 0.75, 2.4),
    (3.0, 0.75, 2.4),
];

fn geometry_bounds(frame: &FrameState, index: usize) -> Rect {
    let geometry = frame.render_geometry(index).expect("geometry leaf");
    // A retained path morph is one source/target pair plus a progress scalar.
    // Observe the same ordered correspondence consumed by the renderer, rather
    // than accidentally measuring only the source path at an interior time.
    let interpolated;
    let geometry = if let GeometryRef::VectorPath(source) = geometry {
        if let Some(target) = source.morph_target() {
            interpolated = GeometryRef::path(
                noon_geometry::interpolate_path_preserving_order(
                    source,
                    target,
                    frame.morph(index),
                )
                .expect("valid retained path correspondence"),
            );
            &interpolated
        } else {
            geometry
        }
    } else {
        geometry
    };
    geometry
        .world_bounds(frame.render_transform(index))
        .expect("finite geometry bounds")
}

fn assert_composition(frame: &FrameState, initial: &FrameState, y: f32, ekf_x: f32) {
    let close = |actual: f32, expected: f32| {
        assert!(
            (actual - expected).abs() < 2e-5,
            "at t={}: {actual} != {expected}",
            frame.time
        );
    };
    let point_close = |actual: Vec2, expected: Vec2| {
        close(actual.x, expected.x);
        close(actual.y, expected.y);
    };
    assert_eq!(frame.objects.len(), 6, "targets must remain detached");
    for (index, original) in initial.objects.iter().enumerate() {
        assert!(frame.is_present(index), "missing leaf {index}");
        assert_eq!(
            frame.objects[index].id, original.id,
            "leaf identity changed"
        );
        assert_eq!(frame.objects[index].style, original.style);
        close(frame.appearance(index), 1.0);
    }

    for (shape, label, x) in [(IMU_BOX, IMU_LABEL, -1.4), (EKF_BOX, EKF_LABEL, ekf_x)] {
        let bounds = geometry_bounds(frame, shape);
        point_close(bounds.center(), Vec2::new(x, y));
        close(bounds.max.x - bounds.min.x, 1.6);
        close(bounds.max.y - bounds.min.y, 0.8);
        let text = &frame.objects[label];
        assert!(text.text().is_some());
        assert_eq!(text.content, initial.objects[label].content);
        assert_eq!(text.text_bounds, initial.objects[label].text_bounds);
        point_close(
            frame
                .render_transform(label)
                .transform_point(text.text_bounds.expect("retained text bounds").center()),
            bounds.center(),
        );
    }

    // Transform may represent the shaft as an analytic Line or an equivalent
    // retained path. Its actual horizontal extent must be identical in either.
    let shaft = geometry_bounds(frame, SHAFT);
    point_close(shaft.min, Vec2::new(-0.6, y));
    point_close(shaft.max, Vec2::new(ekf_x - 1.0, y));
    close(frame.objects[SHAFT].style.stroke_width, 0.03);

    let tip = geometry_bounds(frame, TIP);
    close(tip.min.x, ekf_x - 1.0);
    close(tip.max.x, ekf_x - 0.8);
    close(tip.min.y, y - 0.1);
    close(tip.max.y, y + 0.1);
    close(frame.objects[TIP].style.stroke_width, 0.0);

    // The visible arrow apex meets the right shape's left side throughout the
    // scripted extension, and its shaft starts at the left shape's right side.
    close(tip.max.x, geometry_bounds(frame, EKF_BOX).min.x);
    close(shaft.min.x, geometry_bounds(frame, IMU_BOX).max.x);
}

#[test]
fn coordinated_group_and_member_motion_preserve_labels_and_connections() {
    let mut session = diagram_composition::session().unwrap();
    assert_eq!(diagram_composition::DURATION, 3.0);
    let initial = session.frame().clone();
    for (time, y, ekf_x) in SAMPLES {
        let frame = session.advance_to(time).unwrap();
        assert_composition(frame, &initial, y, ekf_x);
    }
}

#[test]
fn direct_seek_and_backward_replay_match_forward_evaluation() {
    let mut session = diagram_composition::session().unwrap();
    let forward: Vec<_> = SAMPLES
        .iter()
        .map(|&(time, _, _)| session.advance_to(time).unwrap().clone())
        .collect();
    for expected in forward.iter().rev().chain(forward.iter()) {
        let actual = session.seek(expected.time).unwrap();
        assert_eq!(actual, expected, "replay differs at {}", expected.time);
    }
}
