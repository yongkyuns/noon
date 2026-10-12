//! Actual native retained rendering against the pinned Manim image oracle.
#![cfg(feature = "image-decode")]

use noon::{
    example_scenes::raster_image, LiveProgramStatus, RasterImageSampling, RustHostCallbackTable,
};
use std::path::{Path, PathBuf};

mod raster_support;
use raster_support::{Raster, SIZE};

fn compare(pixels: &[u8], name: &str, oracle: &Path, output: &Path) -> bool {
    image::save_buffer(
        output.join(name),
        pixels,
        SIZE,
        SIZE,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let expected = image::open(oracle.join(name)).unwrap().to_rgba8();
    assert_eq!(expected.dimensions(), (SIZE, SIZE));
    let mut sum = 0u64;
    let mut outliers = 0u64;
    let mut maximum = 0;
    assert_eq!(pixels.len(), expected.as_raw().len());
    for (actual, expected) in pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_raw().as_chunks::<4>().0)
    {
        let mut largest = 0;
        for channel in 0..3 {
            let error = actual[channel].abs_diff(expected[channel]);
            sum += u64::from(error);
            largest = largest.max(error);
        }
        maximum = maximum.max(largest);
        outliers += u64::from(largest > 8);
    }
    let mean = sum as f64 / f64::from(SIZE * SIZE * 3);
    let fraction = outliers as f64 / f64::from(SIZE * SIZE);
    eprintln!("{name}: mean={mean:.5}, outliers={fraction:.5}, max={maximum}");
    mean <= 1.0 && fraction <= 0.008
}

#[test]
#[ignore = "requires Vulkan and artifacts from scripts/image-manim-reference.py"]
fn native_images_match_manim_sampling_opacity_and_lifecycle() {
    let oracle = PathBuf::from(
        std::env::var("NOON_IMAGE_MANIM_DIRECTORY").expect("pinned Manim oracle directory"),
    );
    let output = PathBuf::from(
        std::env::var("NOON_IMAGE_ARTIFACTS").expect("qualification artifact directory"),
    )
    .join("native");
    std::fs::create_dir_all(&output).unwrap();
    let mut raster = pollster::block_on(Raster::new());
    let mut failures = Vec::new();
    for (name, sampling) in [
        ("nearest", RasterImageSampling::Nearest),
        ("bilinear", RasterImageSampling::Linear),
        ("bicubic", RasterImageSampling::Bicubic),
    ] {
        for opacity in [1.0, 0.5] {
            let mut session = raster_image::sampling_session(sampling, opacity).unwrap();
            let pixels = raster.capture(&session.take_renderer_publication());
            let name = format!("{name}-{opacity:.1}.png");
            if !compare(&pixels, &name, &oracle, &output) {
                failures.push(name);
            }
        }
    }
    let mut program = raster_image::program().unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    for time in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5] {
        for attempt in 0..16 {
            assert!(attempt < 15, "image continuation did not settle at {time}");
            match program.status() {
                LiveProgramStatus::ReadyToResume => {
                    program.resume().unwrap();
                }
                LiveProgramStatus::PublicationPending(expected) => {
                    let publication = program.take_renderer_publication();
                    assert_eq!(publication.context(), expected);
                    raster.capture(&publication);
                    program.admit_publication(expected).unwrap();
                }
                LiveProgramStatus::Awaiting(_) => {
                    if program.session().frame().time == time {
                        break;
                    }
                    program.drive_to(&mut callbacks, time).unwrap();
                }
                LiveProgramStatus::Finished => break,
                LiveProgramStatus::Terminal => panic!("terminal image program"),
            }
        }
        assert_eq!(program.session().frame().time, time);
        let pixels = raster.capture(&program.take_renderer_publication());
        let name = format!("lifecycle-{time:.1}.png");
        if !compare(&pixels, &name, &oracle, &output) {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "Manim image raster mismatch: {failures:?}"
    );
}
