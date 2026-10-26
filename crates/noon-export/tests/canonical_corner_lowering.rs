//! Equivalent sampled lines and semantic-precision corners use the same pixels.
#![cfg(all(
    not(target_arch = "wasm32"),
    feature = "native-text",
    feature = "bundled-fonts"
))]
use noon::{ManimAxesOptions, ManimGeometryOptions, Scene};
use noon_core::VectorPath;
use noon_export::{CaptureOptions, SessionCapture};

fn capture_reference_curves(corners: bool) -> Vec<u8> {
    let mut scene = Scene::new();
    let axes = scene
        .axes(&ManimAxesOptions::new([0., 10., 2.], [0., 3., 1.], 10., 4.))
        .unwrap();
    let data = noon::synchronized_plotting_example::RECORDINGS
        .iter()
        .map(|row| {
            row.iter()
                .map(|&[time, value]| noon::plot_presentation::TimedPlotSample { time, value })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let rows = data.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let plan = noon::synchronized_plot_presentation::SynchronizedTimeSeriesPlan::new(
        axes.authored_frame().unwrap(),
        &rows,
        [0., 10.],
        noon::synchronized_plotting_example::RUN_TIME,
    )
    .unwrap();
    for row in plan.series() {
        let mut options = if corners {
            ManimGeometryOptions::path(VectorPath::new()).unwrap()
        } else {
            ManimGeometryOptions::sampled_plot(row.points()).unwrap()
        };
        options.set_color(1., 1., 1., 1.).unwrap();
        options.set_stroke_width(0.018).unwrap();
        options.set_object_opacity(0.2).unwrap();
        let mut reference = scene.geometry(options).unwrap();
        if corners {
            let points = row
                .points()
                .iter()
                .map(|&[x, y]| (x, y))
                .collect::<Vec<_>>();
            reference.set_points_as_corners_f64(&points).unwrap();
        }
        scene.add(&reference).unwrap();
    }
    let mut session = scene.execution_session().unwrap();
    assert_eq!(session.frame().objects.len(), 2);
    let mut capture = SessionCapture::new(&session, CaptureOptions::new(960, 540)).unwrap();
    let frame = capture.capture(&mut session).unwrap();
    assert_eq!(frame.receipt.published_time, 0.0);
    frame.rgba.to_vec()
}

#[test]
#[ignore = "requires native GPU; selected by the canonical corner output gate"]
fn canonical_corner_controls_preserve_polyline_rendering() {
    let sampled = capture_reference_curves(false);
    let corners = capture_reference_curves(true);
    assert!(
        sampled
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[..3].iter().any(|&v| v > 0))
            .count()
            > 100
    );
    if let Some(path) = std::env::var_os("NOON_CORNER_NATIVE_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("sampled.rgba"), &sampled).unwrap();
        std::fs::write(path.join("corners.rgba"), &corners).unwrap();
    }
    let differences = sampled
        .as_chunks::<4>()
        .0
        .iter()
        .zip(corners.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differences, 0,
        "equivalent canonical line controls changed pixels"
    );
}
