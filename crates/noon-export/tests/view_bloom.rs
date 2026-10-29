//! The normal capture host resolves the authored camera view without GPU code
//! in the creator scene. The reference uses a separate full-2D f64 convolution.
#![cfg(not(target_arch = "wasm32"))]

use noon::{
    effects::{Bloom, EffectUpdate, Pixels},
    Scene,
};
use noon_core::ViewColorProfile;
use noon_export::{CaptureOptions, CapturePixelFormat, SessionCapture};
const SIZE: u32 = 16;
fn decode(x: f64) -> f64 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}
fn reference(sigma: f64, threshold: f64, intensity: f64) -> Vec<u8> {
    let source = [
        0.5 + 0.5 * decode(0.25),
        0.5 + 0.5 * decode(0.5),
        0.5 + 0.5 * decode(0.75),
    ];
    let luminance = 0.2126 * source[0] + 0.7152 * source[1] + 0.0722 * source[2];
    let extract = if luminance > threshold {
        (luminance - threshold) / luminance
    } else {
        0.0
    };
    let radius = (3.0 * sigma).ceil() as i32;
    let weight = |x: i32, y: i32| {
        if sigma == 0.0 {
            f64::from(x == 0 && y == 0)
        } else {
            (-f64::from(x * x + y * y) / (2.0 * sigma * sigma)).exp()
        }
    };
    let mass: f64 = (-radius..=radius)
        .flat_map(|y| (-radius..=radius).map(move |x| weight(x, y)))
        .sum();
    let mut output = Vec::new();
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let mut retained_mass = 0.0;
            for yy in (y - radius).max(0)..=(y + radius).min(SIZE as i32 - 1) {
                for xx in (x - radius).max(0)..=(x + radius).min(SIZE as i32 - 1) {
                    retained_mass += weight(xx - x, yy - y) / mass;
                }
            }
            for color in source {
                let linear = color * (1.0 + intensity * extract * retained_mass);
                let mapped = linear / (1.0 + linear);
                let encoded = if mapped <= 0.0031308 {
                    12.92 * mapped
                } else {
                    1.055 * mapped.powf(1.0 / 2.4) - 0.055
                };
                output.push((encoded * 255.0).round() as u8);
            }
            output.push(255);
        }
    }
    output
}
#[test]
#[ignore = "requires a real native float adapter; missing capability fails qualification"]
fn session_capture_resolves_camera_bloom_and_switches_working_profiles() {
    let mut scene = Scene::new();
    let mut camera = scene.camera_frame().unwrap();
    camera
        .set_view_color_profile(ViewColorProfile::LinearHdr)
        .unwrap();
    camera.set_object_opacity(0.75).unwrap();
    camera
        .add_effect(
            Bloom::new(
                EffectUpdate::default()
                    .radius(Pixels(1.25))
                    .threshold(0.25)
                    .intensity(1.0),
            )
            .unwrap(),
            "bloom",
        )
        .unwrap();
    let mut panel = scene.rectangle(8.0, 8.0).unwrap();
    panel.set_fill(1.0, 1.0, 1.0, 0.5).unwrap();
    panel.disable_stroke().unwrap();
    scene.add(&panel).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut options = CaptureOptions::new(SIZE, SIZE);
    options.background = [0.25, 0.5, 0.75];
    let mut capture = SessionCapture::new(&session, options).unwrap();
    let root = std::env::var_os("NOON_EXPORT_VIEW_BLOOM_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(root) = &root {
        std::fs::create_dir_all(root).unwrap();
    }
    let mut cases = Vec::new();
    let mut base = None;
    let mut maximum = 0;
    for phase in 0..6 {
        match phase {
            1 => scene
                .live(&mut session)
                .set_effect(&camera, "bloom", EffectUpdate::default().intensity(0.0))
                .unwrap(),
            2 => scene
                .live(&mut session)
                .set_effect(
                    &camera,
                    "bloom",
                    EffectUpdate::default()
                        .radius(Pixels(3.0))
                        .threshold(0.0)
                        .intensity(3.0),
                )
                .unwrap(),
            3 => scene
                .live(&mut session)
                .remove_effect(&camera, "bloom")
                .unwrap(),
            4 => scene
                .live(&mut session)
                .set_view_color_profile(&camera, ViewColorProfile::Compatibility)
                .unwrap(),
            5 => scene
                .live(&mut session)
                .set_view_color_profile(&camera, ViewColorProfile::LinearHdr)
                .unwrap(),
            _ => {}
        }
        let frame = capture.capture(&mut session).unwrap();
        let expected = match phase {
            0 => reference(1.25, 0.25, 1.0),
            2 => reference(3.0, 0.0, 3.0),
            4 => [159, 191, 223, 255].repeat((SIZE * SIZE) as usize),
            _ => reference(0.0, 1.0, 0.0),
        };
        assert_eq!(
            frame.format,
            if phase == 4 {
                CapturePixelFormat::RendererRgba8UnormOpaque
            } else {
                CapturePixelFormat::LinearHdrSdrRgba8Opaque
            }
        );
        assert_eq!(frame.rgba.len(), expected.len());
        for (a, e) in frame.rgba.iter().zip(&expected) {
            maximum = maximum.max(a.abs_diff(*e));
            assert!(a.abs_diff(*e) <= 2, "phase={phase}: {a} != {e}");
        }
        if phase == 1 {
            base = Some(frame.rgba.to_vec());
        }
        if phase == 3 || phase == 5 {
            assert_eq!(
                Some(frame.rgba),
                base.as_deref(),
                "exact zero-energy/removal/profile restoration"
            );
        }
        let raw = frame.rgba.to_vec();
        let clean = capture.capture(&mut session).unwrap();
        assert_eq!(clean.rgba, raw);
        if let Some(root) = &root {
            std::fs::write(root.join(format!("{phase}.rgba8")), &raw).unwrap();
        }
        cases.push(serde_json::json!({"phase":phase,"expected":expected,"profile":if phase==4 {"compatibility"} else {"linear_hdr"}}));
    }
    if let Some(root) = root {
        std::fs::write(
            root.join("report.json"),
            serde_json::to_vec(
                &serde_json::json!({"size":[SIZE,SIZE],"cases":cases,"maximum_error":maximum}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    eprintln!("native view capture: {} phases, maximum display error {maximum}, exact removals/profile restoration",cases.len());
}

struct BloomMotion {
    camera: noon::Mobject,
    target: noon::Mobject,
    started: bool,
}
impl noon::LiveContinuation for BloomMotion {
    type Error = noon::LiveSessionError;
    fn resume(
        &mut self,
        live: &mut noon::LiveSession<'_>,
    ) -> Result<noon::ContinuationStep, Self::Error> {
        if self.started {
            return Ok(noon::ContinuationStep::Finished);
        }
        self.started = true;
        live.declare_and_activate_transform_to(
            &self.camera,
            &self.target,
            noon::AnimationOptions::new()
                .run_time(1.0)
                .rate_func(noon::RateFunction::Linear),
        )
        .map(noon::ContinuationStep::Await)
    }
}
#[test]
#[ignore = "requires a real native float adapter; normal authored source/export clock proof"]
fn source_capture_frames_uses_shared_camera_bloom_time_and_output() {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    let mut scene = Scene::new();
    let mut camera = scene.camera_frame().unwrap();
    camera
        .set_view_color_profile(ViewColorProfile::LinearHdr)
        .unwrap();
    camera
        .add_effect(
            Bloom::new(
                EffectUpdate::default()
                    .radius(Pixels(1.25))
                    .threshold(0.25)
                    .intensity(1.0),
            )
            .unwrap(),
            "bloom",
        )
        .unwrap();
    let mut panel = scene.rectangle(8.0, 8.0).unwrap();
    panel.set_fill(1.0, 1.0, 1.0, 0.5).unwrap();
    panel.disable_stroke().unwrap();
    scene.add(&panel).unwrap();
    let mut target = camera.target_editor().unwrap();
    target
        .set_effect("bloom", EffectUpdate::default().intensity(0.0))
        .unwrap();
    let mut program = scene
        .into_live_program(BloomMotion {
            camera,
            target,
            started: false,
        })
        .unwrap();
    let mut options = CaptureOptions::new(SIZE, SIZE);
    options.background = [0.25, 0.5, 0.75];
    let root = std::env::var_os("NOON_EXPORT_VIEW_BLOOM_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(root) = &root {
        std::fs::create_dir_all(root).unwrap();
    }
    let mut times = Vec::new();
    let mut cases = Vec::new();
    let mut maximum = 0;
    noon_export::capture_frames(
        &mut program,
        &mut noon::RustHostCallbackTable::new(),
        ExportFrameOptions {
            frame_rate: FrameRate::new(2, 1).unwrap(),
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 8,
            max_transitions_per_sample: 16,
            final_hold_seconds: 0.0,
        },
        options,
        |frame| {
            let time = frame.observation.requested_time;
            let expected = reference(1.25, 0.25, 1.0 - time);
            assert_eq!(frame.format, CapturePixelFormat::LinearHdrSdrRgba8Opaque);
            for (a, e) in frame.rgba.iter().zip(&expected) {
                maximum = maximum.max(a.abs_diff(*e));
                assert!(a.abs_diff(*e) <= 2, "source time={time}: {a} != {e}");
            }
            let id = times.len();
            times.push(time);
            if let Some(root) = &root {
                std::fs::write(root.join(format!("source-{id}.rgba8")), frame.rgba).unwrap();
            }
            cases.push(serde_json::json!({"id":id,"time":time,"expected":expected}));
            Ok::<_, std::convert::Infallible>(())
        },
    )
    .unwrap();
    assert!(times.contains(&0.0) && times.contains(&0.5));
    assert!(times.iter().all(|time| *time <= 1.0));
    if let Some(root) = root {
        std::fs::write(
            root.join("source-report.json"),
            serde_json::to_vec(
                &serde_json::json!({"size":[SIZE,SIZE],"cases":cases,"maximum_error":maximum}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    eprintln!("native source camera Bloom: times={times:?}, maximum display error={maximum}");
}
