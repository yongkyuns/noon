//! Pinned camera/dot observations from ManimCE 0.21.0, not Noon-generated goldens.
//!
//! Upstream construct: ManimCommunity/manim@861cd4849b17db1db3515b531ffe80b297848f93,
//! docs/source/examples.rst, FollowingGraphCamera. The 960x540 Cairo oracle uses
//! 30 Hz. Frames 0..89 are materialized; frame 90 is post-construct completion.
//! Source artifact: run 37370684776, attempt 2, artifact 11373182134,
//! sha256 700160f4ed30246b3178fcc763f8af47b46b7279ed08fbec41c634e0340ab8b4.
//! That attempt's Python execution failed during imports, but the independent
//! Manim reference capture completed. This test does not claim raster parity.

use noon::example_scenes::following_graph_camera::program;
use noon::{Color, LiveProgramStatus};

// frame index; effective camera x/y/height, followed dot x/y.
const REFERENCE: &[(u32, [f64; 5])] = &[
    (
        0,
        [
            0.0,
            0.0,
            8.0,
            -4.909090909090909,
            -2.454545454545454,
        ],
    ),
    (
        15,
        [
            -2.454545454545454,
            -1.2272727272727268,
            6.0,
            -4.909090909090909,
            -2.454545454545454,
        ],
    ),
    (
        29,
        [
            -4.896039124086225,
            -2.4480195620431124,
            4.010634787781595,
            -4.909090909090909,
            -2.454545454545454,
        ],
    ),
    (
        30,
        [
            -4.909090909090909,
            -2.454545454545454,
            4.0,
            -4.909090909090909,
            -2.454545454545454,
        ],
    ),
    (
        31,
        [
            -4.583252773227603,
            -2.294038012221474,
            4.0,
            -4.583252773227603,
            -2.2940380122214736,
        ],
    ),
    (
        36,
        [
            -2.833890263027189,
            -1.9387829054009238,
            4.000000000000001,
            -2.833890263027189,
            -1.9387829054009238,
        ],
    ),
    (
        45,
        [
            0.23169337244368604,
            -2.9999999731797073,
            4.0,
            0.23169337244368593,
            -2.9999999731797073,
        ],
    ),
    (
        54,
        [
            3.2973427185724886,
            -1.9387734834070023,
            4.0,
            3.2973427185724886,
            -1.9387734834070023,
        ],
    ),
    (
        59,
        [
            5.046834895797698,
            -2.2941332978892746,
            4.0,
            5.046834895797699,
            -2.2941332978892746,
        ],
    ),
    (
        60,
        [
            5.3724850481120505,
            -2.454545454545454,
            4.0,
            5.3724850481120505,
            -2.454545454545454,
        ],
    ),
    (
        61,
        [
            5.358201238525435,
            -2.448019562043112,
            4.010634787781594,
            5.3724850481120505,
            -2.454545454545454,
        ],
    ),
    (
        75,
        [
            2.6862425240560253,
            -1.2272727272727268,
            6.0,
            5.3724850481120505,
            -2.454545454545454,
        ],
    ),
    (
        89,
        [
            0.014283809586615348,
            -0.0065258925023423675,
            7.989365212218406,
            5.3724850481120505,
            -2.454545454545454,
        ],
    ),
    (
        90,
        [
            0.0,
            0.0,
            8.0,
            5.3724850481120505,
            -2.454545454545454,
        ],
    ),
];

fn observe(indices: impl IntoIterator<Item = u32>) -> Vec<(u32, [f64; 5])> {
    let (mut program, mut callbacks) = program().unwrap();
    assert!(matches!(
        program.resume().unwrap(),
        LiveProgramStatus::Awaiting(_)
    ));
    let dot_id = {
        let mut dots = program
            .session()
            .frame()
            .objects
            .iter()
            .filter(|object| object.style.fill == Some(Color::ORANGE));
        let id = dots.next().expect("one orange moving dot").id;
        assert!(dots.next().is_none());
        id
    };
    let mut observed = Vec::new();
    for index in indices {
        let time = f64::from(index) / 30.0;
        let status = program.drive_to(&mut callbacks, time).unwrap();
        if matches!(status, LiveProgramStatus::PublicationPending(_)) {
            let publication = program.take_renderer_publication().context();
            program.admit_publication(publication).unwrap();
        }
        let session = program.session();
        assert_eq!(session.frame().time, time);
        let camera = session.camera().unwrap();
        let (slot, dot) = session
            .frame()
            .objects
            .iter()
            .enumerate()
            .find(|(_, object)| object.id == dot_id)
            .expect("path following preserves the moving dot identity");
        assert!(session.frame().is_present(slot));
        assert_eq!(dot.style.fill, Some(Color::ORANGE));
        observed.push((
            index,
            [
                f64::from(camera.center.x),
                f64::from(camera.center.y),
                f64::from(camera.height),
                f64::from(dot.transform.translation.x),
                f64::from(dot.transform.translation.y),
            ],
        ));
        // Resume the actual compiled continuation only at its logical barriers.
        // This exercises updater registration/removal and Restore activation,
        // rather than seeking a separately reconstructed final execution plan.
        match index {
            30 | 60 => {
                assert!(matches!(
                    program.resume().unwrap(),
                    LiveProgramStatus::Awaiting(_)
                ));
                assert_eq!(program.session().camera().unwrap(), camera);
            }
            90 => assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished),
            _ => {}
        }
    }
    observed
}

#[test]
fn direct_camera_matches_pinned_manim_at_sparse_and_dense_barriers() {
    let dense = observe(0..=90);
    let sparse = observe(REFERENCE.iter().map(|(index, _)| *index));
    assert_eq!(dense.len(), 91);
    assert_eq!(sparse.len(), REFERENCE.len());
    for (&(index, expected), &(sparse_index, actual)) in REFERENCE.iter().zip(&sparse) {
        assert_eq!(index, sparse_index);
        for (field, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert!(
                actual.is_finite() && (actual - expected).abs() <= 1e-6,
                "Manim frame {index}, field {field}: {actual} != {expected}"
            );
        }
        let dense_values = dense[index as usize].1;
        for (field, (dense_value, sparse_value)) in dense_values.iter().zip(actual).enumerate() {
            assert!(
                (dense_value - sparse_value).abs() <= 1e-6,
                "dense/sparse frame {index}, field {field} differs"
            );
        }
    }
}
