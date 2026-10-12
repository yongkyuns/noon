#![cfg(all(feature = "native-text", feature = "bundled-fonts"))]
use noon::{LiveContinuation, LiveProgram, LiveProgramStatus, RustHostCallbackTable, YELLOW};

fn drive<C: LiveContinuation<Error = String>>(program: &mut LiveProgram<C>, time: f64) {
    let mut callbacks = RustHostCallbackTable::new();
    for _ in 0..64 {
        match program.status() {
            LiveProgramStatus::ReadyToResume => {
                program.resume().unwrap();
            }
            LiveProgramStatus::PublicationPending(_) => {
                let context = program.take_renderer_publication().context();
                program.admit_publication(context).unwrap();
            }
            LiveProgramStatus::Awaiting(_) => {
                if (program.session().frame().time - time).abs() < 1e-9 {
                    return;
                }
                program.drive_to(&mut callbacks, time).unwrap();
            }
            LiveProgramStatus::Finished => return,
            other => panic!("unexpected status {other:?}"),
        }
    }
    panic!("plotting continuation did not reach {time}");
}

#[test]
fn animated_coordinates_admit_in_order_then_finish_the_wait() {
    let mut program = noon::coordinate_plotting_example::program().unwrap();
    for (time, count) in [(0.25, 1), (0.9, 15), (2.3, 16), (3.8, 17)] {
        drive(&mut program, time);
        assert_eq!(program.session().frame().objects.len(), count);
    }
    drive(&mut program, noon::coordinate_plotting_example::RUN_TIME);
    assert_eq!(program.status(), LiveProgramStatus::Finished);
}

#[test]
fn marker_uses_completed_live_number_line_frame_after_transform() {
    let mut program = noon::animated_number_line_example::program().unwrap();
    for (time, x, y) in [
        (2.75, -2.2, 0.0),
        (4.6, 1.65, 0.0),
        (6.1, 3.3, 0.3),
        (7.25, 0.55, 0.6),
        (8.8, -2.2, 0.6),
    ] {
        drive(&mut program, time);
        let marker = program
            .session()
            .frame()
            .objects
            .iter()
            .find(|object| object.style.fill == Some(YELLOW))
            .unwrap();
        assert!((f64::from(marker.transform.translation.x) - x).abs() < 2e-5);
        assert!((f64::from(marker.transform.translation.y) - y).abs() < 2e-5);
    }
    assert_eq!(program.status(), LiveProgramStatus::Finished);
    assert_eq!(program.session().frame().objects.len(), 22);
}
